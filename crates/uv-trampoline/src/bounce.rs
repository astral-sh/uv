use std::ffi::{c_void, CStr, CString};
use std::mem::{size_of, size_of_val};
use std::vec::Vec;

use windows::core::{s, PCSTR, PSTR};
use windows::Win32::{
    Foundation::{
        CloseHandle, GetLastError, SetHandleInformation, SetLastError, BOOL,
        ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE, HANDLE_FLAG_INHERIT,
        INVALID_HANDLE_VALUE, MAX_PATH, TRUE,
    },
    Storage::FileSystem::{
        CreateFileA, GetFileSizeEx, ReadFile, SetFilePointerEx, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN,
        FILE_GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING,
    },
    System::Console::{
        GetStdHandle, SetConsoleCtrlHandler, SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    },
    System::Diagnostics::Debug::{
        FormatMessageA, FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM,
        FORMAT_MESSAGE_IGNORE_INSERTS,
    },
    System::Environment::GetCommandLineA,
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectA, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
    },
    System::LibraryLoader::GetModuleFileNameA,
    System::Threading::{
        CreateProcessA, ExitProcess, GetExitCodeProcess, GetStartupInfoA, WaitForInputIdle,
        WaitForSingleObject, INFINITE, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOA,
    },
    UI::WindowsAndMessaging::{
        CreateWindowExA, DestroyWindow, GetMessageA, PeekMessageA, PostMessageA, HWND_MESSAGE, MSG,
        PEEK_MESSAGE_REMOVE_TYPE, WINDOW_EX_STYLE, WINDOW_STYLE,
    },
};

use crate::{eprintln, format};

const MAGIC_NUMBER: [u8; 4] = [b'U', b'V', b'U', b'V'];
const PATH_LEN_SIZE: usize = size_of::<u32>();
const MAX_PATH_LEN: u32 = 32 * 1024;

/// Transform `<command> <arguments>` to `python <command> <arguments>`.
fn make_child_cmdline() -> CString {
    let executable_name: CString = executable_filename();
    let python_exe = find_python_exe(&executable_name);
    let mut child_cmdline = Vec::<u8>::new();

    push_quoted_path(&python_exe, &mut child_cmdline);
    child_cmdline.push(b' ');

    // Use the full executable name because CMD only passes the name of the executable (but not the path)
    // when e.g. invoking `black` instead of `<PATH_TO_VENV>/Scripts/black` and Python then fails
    // to find the file. Unfortunately, this complicates things because we now need to split the executable
    // from the arguments string...
    push_quoted_path(&executable_name, &mut child_cmdline);

    push_arguments(&mut child_cmdline);

    child_cmdline.push(b'\0');

    // Helpful when debugging trampoline issues
    // eprintln!(
    //     "executable_name: '{}'\nnew_cmdline: {}",
    //     std::str::from_utf8(executable_name.to_bytes()).unwrap(),
    //     std::str::from_utf8(child_cmdline.as_slice()).unwrap()
    // );

    CString::from_vec_with_nul(child_cmdline).unwrap_or_else(|_| {
        eprintln!("Child command line is not correctly null terminated");
        exit_with_status(1);
    })
}

fn push_quoted_path(path: &CStr, command: &mut Vec<u8>) {
    command.push(b'"');
    for byte in path.to_bytes() {
        if *byte == b'"' {
            // 3 double quotes: one to end the quoted span, one to become a literal double-quote,
            // and one to start a new quoted span.
            command.extend(br#"""""#);
        } else {
            command.push(*byte);
        }
    }
    command.extend(br#"""#);
}

/// Returns the full path of the executable.
/// See https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-getmodulefilenamea
fn executable_filename() -> CString {
    // MAX_PATH is a lie, Windows paths can be longer.
    // https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#maximum-path-length-limitation
    // But it's a good first guess, usually paths are short, and we should only need a single attempt.
    let mut buffer: Vec<u8> = vec![0; MAX_PATH as usize];
    loop {
        // Call the Windows API function to get the module file name
        let len = unsafe { GetModuleFileNameA(None, &mut buffer) };

        // That's the error condition because len doesn't include the trailing null byte
        if len as usize == buffer.len() {
            let last_error = unsafe { GetLastError() };
            match last_error {
                ERROR_INSUFFICIENT_BUFFER => {
                    unsafe { SetLastError(ERROR_SUCCESS) };
                    // Try again with twice the size
                    buffer.resize(buffer.len() * 2, 0);
                }
                err => {
                    print_last_error_and_exit(&format!(
                        "Failed to get executable name (code: {})",
                        err.0
                    ));
                }
            }
        } else {
            buffer.truncate(len as usize + b"\0".len());
            break;
        }
    }

    CString::from_vec_with_nul(buffer).unwrap_or_else(|_| {
        eprintln!("Executable name is not correctly null terminated");
        exit_with_status(1);
    })
}

/// Reads the executable binary from the back to find the path to the Python executable that is written
/// after the ZIP file content.
///
/// The executable is expected to have the following format:
/// * The file must end with the magic number 'UVUV'.
/// * The last 4 bytes (little endian) are the length of the path to the Python executable.
/// * The path encoded as UTF-8 comes right before the length
///
/// # Panics
/// If there's any IO error, or the file does not conform to the specified format.
fn find_python_exe(executable_name: &CStr) -> CString {
    let file_handle = unsafe {
        CreateFileA(
            PCSTR::from_raw(executable_name.as_ptr() as *const _),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .unwrap_or_else(|_| {
        print_last_error_and_exit(&format!(
            "Failed to open executable '{}'",
            &*executable_name.to_string_lossy(),
        ))
    });

    let mut file_size: i64 = 0;
    // `SetFilePointerEx` supports setting the file pointer from the back, but pointing it past the file's start
    // results in an error. That's why we need to know the file size to avoid ever seeking past the start of the file.
    if unsafe { GetFileSizeEx(file_handle, &mut file_size) }.is_err() {
        print_last_error_and_exit(&format!(
            "Failed to get the size of the executable '{}'",
            &*executable_name.to_string_lossy(),
        ));
    }

    // Start with a size of 1024 bytes which should be enough for most paths but avoids reading the
    // entire file.
    let mut buffer: Vec<u8> = Vec::new();
    let mut bytes_to_read = 1024.min(u32::try_from(file_size).unwrap_or(u32::MAX));

    let path: CString = loop {
        // SAFETY: Casting to usize is safe because we only support 64bit systems where usize is guaranteed to be larger than u32.
        buffer.resize(bytes_to_read as usize, 0);

        if unsafe {
            SetFilePointerEx(
                file_handle,
                file_size - i64::from(bytes_to_read),
                None,
                FILE_BEGIN,
            )
        }
        .is_err()
        {
            print_last_error_and_exit("Failed to set the file pointer to the end of the file");
        }

        let mut read_bytes = bytes_to_read;
        if unsafe { ReadFile(file_handle, Some(&mut buffer), Some(&mut read_bytes), None) }.is_err()
        {
            print_last_error_and_exit("Failed to read the executable file");
        }

        // Truncate the buffer to the actual number of bytes read.
        buffer.truncate(read_bytes as usize);

        if !buffer.ends_with(&MAGIC_NUMBER) {
            eprintln!("Magic number 'UVUV' not found at the end of the file. Did you append the magic number, the length and the path to the python executable at the end of the file?");
            exit_with_status(1);
        }

        // Remove the magic number
        buffer.truncate(buffer.len() - MAGIC_NUMBER.len());

        let path_len = match buffer.get(buffer.len() - PATH_LEN_SIZE..) {
            Some(path_len) => {
                let path_len = u32::from_le_bytes(path_len.try_into().unwrap_or_else(|_| {
                    eprintln!("Slice length is not equal to 4 bytes");
                    exit_with_status(1);
                }));

                if path_len > MAX_PATH_LEN {
                    eprintln!("Only paths with a length up to 32KBs are supported but the python path has a length of {}", path_len);
                    exit_with_status(1);
                }

                // SAFETY: path len is guaranteed to be less than 32KBs
                path_len as usize
            }
            None => {
                eprintln!("Python executable length missing. Did you write the length of the path to the Python executable before the Magic number?");
                exit_with_status(1);
            }
        };

        // Remove the path length
        buffer.truncate(buffer.len() - PATH_LEN_SIZE);

        if let Some(path_offset) = buffer.len().checked_sub(path_len) {
            buffer.drain(..path_offset);
            buffer.push(b'\0');

            break CString::from_vec_with_nul(buffer).unwrap_or_else(|_| {
                eprintln!("Python executable path is not correctly null terminated");
                exit_with_status(1);
            });
        } else {
            // SAFETY: Casting to u32 is safe because `path_len` is guaranteed to be less than 32KBs,
            // MAGIC_NUMBER is 4 bytes and PATH_LEN_SIZE is 4 bytes.
            bytes_to_read = (path_len + MAGIC_NUMBER.len() + PATH_LEN_SIZE) as u32;

            if i64::from(bytes_to_read) > file_size {
                eprintln!("The length of the python executable path exceeds the file size. Verify that the path length is appended to the end of the launcher script as a u32 in little endian");
                exit_with_status(1);
            }
        }
    };

    if unsafe { CloseHandle(file_handle) }.is_err() {
        print_last_error_and_exit("Failed to close file handle");
    }

    if is_absolute(&path) {
        path
    } else {
        let parent_dir = match executable_name
            .to_bytes()
            .rsplitn(2, |c| *c == b'\\')
            .last()
        {
            Some(parent_dir) => parent_dir,
            None => {
                eprintln!("Script path has unknown separator characters");
                exit_with_status(1);
            }
        };
        let final_path = [parent_dir, b"\\", path.as_bytes()].concat();
        CString::new(final_path).unwrap_or_else(|_| {
            eprintln!("Could not construct the absolute path to the Python executable");
            exit_with_status(1);
        })
    }
}

/// Returns `true` if the path is absolute.
///
/// In this context, as in the Rust standard library, c:\windows is absolute, while c:temp and
/// \temp are not.
fn is_absolute(path: &CStr) -> bool {
    let path = path.to_bytes();
    path.len() >= 3 && path[0].is_ascii_alphabetic() && path[1] == b':' && path[2] == b'\\'
}

fn push_arguments(output: &mut Vec<u8>) {
    // SAFETY: We rely on `GetCommandLineA` to return a valid pointer to a null terminated string.
    let arguments_as_str = unsafe { GetCommandLineA() };
    let arguments_as_bytes = unsafe { arguments_as_str.as_bytes() };

    // Skip over the executable name and then push the rest of the arguments
    let after_executable = skip_one_argument(arguments_as_bytes);

    output.extend_from_slice(after_executable)
}

fn skip_one_argument(arguments: &[u8]) -> &[u8] {
    let mut quoted = false;
    let mut offset = 0;
    let mut bytes_iter = arguments.iter().peekable();

    // Implements https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments?view=msvc-170
    while let Some(byte) = bytes_iter.next().copied() {
        match byte {
            b'"' => {
                quoted = !quoted;
            }
            b'\\' => {
                // Skip over escaped quotes or even number of backslashes.
                if matches!(bytes_iter.peek().copied(), Some(&b'\"' | &b'\\')) {
                    offset += 1;
                    bytes_iter.next();
                }
            }
            byte => {
                if byte.is_ascii_whitespace() && !quoted {
                    break;
                }
            }
        }

        offset += 1;
    }

    &arguments[offset..]
}

fn make_job_object() -> HANDLE {
    let job = unsafe { CreateJobObjectA(None, None) }
        .unwrap_or_else(|_| print_last_error_and_exit("Job creation failed"));
    let mut job_info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    let mut retlen = 0u32;
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut job_info as *mut _ as *mut c_void,
            size_of_val(&job_info) as u32,
            Some(&mut retlen),
        )
    }
    .is_err()
    {
        print_last_error_and_exit("Job information querying failed");
    }
    job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &job_info as *const _ as *const c_void,
            size_of_val(&job_info) as u32,
        )
    }
    .is_err()
    {
        print_last_error_and_exit("Job information setting failed");
    }
    job
}

fn spawn_child(si: &STARTUPINFOA, child_cmdline: CString) -> HANDLE {
    // See distlib/PC/launcher.c::run_child
    if (si.dwFlags & STARTF_USESTDHANDLES).0 != 0 {
        // ignore errors, if the handles are not inheritable/valid, then nothing we can do
        unsafe { SetHandleInformation(si.hStdInput, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| eprintln!("Making stdin inheritable failed"));
        unsafe { SetHandleInformation(si.hStdOutput, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| eprintln!("Making stdout inheritable failed"));
        unsafe { SetHandleInformation(si.hStdError, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| eprintln!("Making stderr inheritable failed"));
    }
    let mut child_process_info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessA(
            None,
            // Why does this have to be mutable? Who knows. But it's not a mistake --
            // MS explicitly documents that this buffer might be mutated by CreateProcess.
            PSTR::from_raw(child_cmdline.as_ptr() as *mut _),
            None,
            None,
            true,
            PROCESS_CREATION_FLAGS(0),
            None,
            None,
            si,
            &mut child_process_info,
        )
    }
    .unwrap_or_else(|_| {
        print_last_error_and_exit("Failed to spawn the python child process");
    });
    unsafe { CloseHandle(child_process_info.hThread) }.unwrap_or_else(|_| {
        print_last_error_and_exit("Failed to close handle to python child process main thread");
    });
    // Return handle to child process.
    child_process_info.hProcess
}

// Apparently, the Windows C runtime has a secret way to pass file descriptors into child
// processes, by using the .lpReserved2 field. We want to close those file descriptors too.
// The UCRT source code has details on the memory layout (see also initialize_inherited_file_handles_nolock):
// https://github.com/huangqinjin/ucrt/blob/10.0.19041.0/lowio/ioinit.cpp#L190-L223
fn close_handles(si: &STARTUPINFOA) {
    // See distlib/PC/launcher.c::cleanup_standard_io()
    for std_handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        if let Ok(handle) = unsafe { GetStdHandle(std_handle) } {
            unsafe { CloseHandle(handle) }.unwrap_or_else(|_| {
                eprintln!("Failed to close standard device handle {}", handle.0 as u32);
            });
            unsafe { SetStdHandle(std_handle, INVALID_HANDLE_VALUE) }.unwrap_or_else(|_| {
                eprintln!("Failed to modify standard device handle {}", std_handle.0);
            });
        }
    }

    // See distlib/PC/launcher.c::cleanup_fds()
    if si.cbReserved2 == 0 || si.lpReserved2.is_null() {
        return;
    }
    let crt_magic = si.lpReserved2 as *const u32;
    let handle_count = unsafe { crt_magic.read_unaligned() } as isize;
    let handle_start = unsafe { crt_magic.offset(1 + handle_count) };
    for i in 0..handle_count {
        let handle_ptr = unsafe { handle_start.offset(i).read_unaligned() } as *const HANDLE;
        // Close all fds inherited from the parent, except for the standard I/O fds.
        unsafe { CloseHandle(*handle_ptr) }.unwrap_or_else(|_| {
            eprintln!("Failed to close child file descriptors at {}", i);
        });
    }
}

/*
    I don't really understand what this function does. It's a straight port from
    https://github.com/pypa/distlib/blob/master/PC/launcher.c, which has the following
    comment:

        End the launcher's "app starting" cursor state.
        When Explorer launches a Windows (GUI) application, it displays
        the "app starting" (the "pointer + hourglass") cursor for a number
        of seconds, or until the app does something UI-ish (eg, creating a
        window, or fetching a message).  As this launcher doesn't do this
        directly, that cursor remains even after the child process does these
        things.  We avoid that by doing the stuff in here.
        See http://bugs.python.org/issue17290 and
        https://github.com/pypa/pip/issues/10444#issuecomment-973408601

    Why do we call `PostMessage`/`GetMessage` at the start, before waiting for the
    child? (Looking at the bpo issue above, this was originally the *whole* fix.)
    Is creating a window and calling PeekMessage the best way to do this? idk.
*/
fn clear_app_starting_state(child_handle: HANDLE) {
    let mut msg = MSG::default();
    unsafe {
        // End the launcher's "app starting" cursor state.
        PostMessageA(None, 0, None, None).unwrap_or_else(|_| {
            eprintln!("Failed to post a message to specified window");
        });
        if GetMessageA(&mut msg, None, 0, 0) != TRUE {
            eprintln!("Failed to retrieve posted window message");
        }
        // Proxy the child's input idle event.
        if WaitForInputIdle(child_handle, INFINITE) != 0 {
            eprintln!("Failed to wait for input from window");
        }
        // Signal the process input idle event by creating a window and pumping
        // sent messages. The window class isn't important, so just use the
        // system "STATIC" class.
        if let Ok(hwnd) = CreateWindowExA(
            WINDOW_EX_STYLE(0),
            s!("STATIC"),
            s!("uv Python Trampoline"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            None,
            None,
        ) {
            // Process all sent messages and signal input idle.
            let _ = PeekMessageA(&mut msg, hwnd, 0, 0, PEEK_MESSAGE_REMOVE_TYPE(0));
            DestroyWindow(hwnd).unwrap_or_else(|_| {
                print_last_error_and_exit("Failed to destroy temporary window");
            });
        }
    }
}

pub fn bounce(is_gui: bool) -> ! {
    let child_cmdline = make_child_cmdline();

    let mut si = STARTUPINFOA::default();
    unsafe { GetStartupInfoA(&mut si) }

    let child_handle = spawn_child(&si, child_cmdline);
    let job = make_job_object();

    if unsafe { AssignProcessToJobObject(job, child_handle) }.is_err() {
        print_last_error_and_exit("Failed to assign child process to the job")
    }

    // (best effort) Close all the handles that we can
    close_handles(&si);

    // (best effort) Switch to some innocuous directory, so we don't hold the original cwd open.
    // See distlib/PC/launcher.c::switch_working_directory
    if std::env::set_current_dir(std::env::temp_dir()).is_err() {
        eprintln!("Failed to set cwd to temp dir");
    }

    // We want to ignore control-C/control-Break/logout/etc.; the same event will
    // be delivered to the child, so we let them decide whether to exit or not.
    unsafe extern "system" fn control_key_handler(_: u32) -> BOOL {
        TRUE
    }
    // See distlib/PC/launcher.c::control_key_handler
    unsafe { SetConsoleCtrlHandler(Some(control_key_handler), true) }.unwrap_or_else(|_| {
        print_last_error_and_exit("Control handler setting failed");
    });

    if is_gui {
        clear_app_starting_state(child_handle);
    }

    let _ = unsafe { WaitForSingleObject(child_handle, INFINITE) };
    let mut exit_code = 0u32;
    if unsafe { GetExitCodeProcess(child_handle, &mut exit_code) }.is_err() {
        print_last_error_and_exit("Failed to get exit code of child process");
    }
    exit_with_status(exit_code);
}

#[cold]
fn print_last_error_and_exit(message: &str) -> ! {
    let err = unsafe { GetLastError() };
    eprintln!("Received error code: {}", err.0);
    let mut msg_ptr = PSTR::null();
    let size = unsafe {
        FormatMessageA(
            FORMAT_MESSAGE_ALLOCATE_BUFFER
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            None,
            err.0,
            0,
            // Weird calling convention: this argument is typed as *mut u8,
            // but if you pass FORMAT_MESSAGE_ALLOCATE_BUFFER then you have to
            // *actually* pass in a *mut *mut u8 and just lie about the type.
            // Getting Rust to do this requires some convincing.
            PSTR(&mut msg_ptr.0 as *mut _ as *mut _),
            0,
            None,
        )
    };

    if size == 0 || msg_ptr.0.is_null() {
        eprintln!(
            "{}: with code {} (failed to get error message)",
            message, err.0
        );
    } else {
        let reason = unsafe {
            let reason = std::slice::from_raw_parts(msg_ptr.0, size as usize);
            std::str::from_utf8_unchecked(reason)
        };
        eprintln!("(uv internal error) {}: {}", message, reason);
    }

    // Note: We don't need to free the buffer here because we're going to exit anyway.
    exit_with_status(1);
}

#[cold]
fn exit_with_status(code: u32) -> ! {
    unsafe {
        ExitProcess(code);
    }
}
