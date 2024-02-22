use alloc::string::String;
use alloc::{ffi::CString, vec, vec::Vec};
use core::mem::MaybeUninit;
use core::{
    ffi::CStr,
    mem,
    ptr::{addr_of, addr_of_mut, null, null_mut},
};

use windows_sys::Win32::Storage::FileSystem::{
    CreateFileA, GetFileSizeEx, ReadFile, SetFilePointerEx, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN,
    FILE_SHARE_READ, OPEN_EXISTING,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        Console::*,
        Environment::{GetCommandLineA, GetEnvironmentVariableA, SetCurrentDirectoryA},
        JobObjects::*,
        LibraryLoader::GetModuleFileNameA,
        Threading::*,
    },
    UI::WindowsAndMessaging::*,
};

use crate::helpers::SizeOf;
use crate::{c, eprintln, format};

const MAGIC_NUMBER: [u8; 4] = [b'U', b'V', b'U', b'V'];
const PATH_LEN_SIZE: usize = mem::size_of::<u32>();
const MAX_PATH_LEN: u32 = 2 * 1024 * 1024;

fn getenv(name: &CStr) -> Option<CString> {
    unsafe {
        let count = GetEnvironmentVariableA(name.as_ptr() as _, null_mut(), 0);
        if count == 0 {
            return None;
        }
        let mut value = Vec::<u8>::with_capacity(count as usize);
        GetEnvironmentVariableA(
            name.as_ptr() as _,
            value.as_mut_ptr(),
            value.capacity() as u32,
        );
        value.set_len(count as usize);
        Some(CString::from_vec_with_nul_unchecked(value))
    }
}

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

    // Helpful when debugging trampline issues
    // eprintln!(
    //     "executable_name: '{}'\nnew_cmdline: {}",
    //     core::str::from_utf8(executable_name.to_bytes()).unwrap(),
    //     core::str::from_utf8(child_cmdline.as_slice()).unwrap()
    // );

    CString::from_vec_with_nul(child_cmdline).unwrap_or_else(|_| {
        eprintln!("Child command line is not correctly null terminated.");
        exit_with_status(1)
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
    // But it's a good first guess, usually paths are short and we should only need a single attempt.
    let mut buffer: Vec<u8> = vec![0; MAX_PATH as usize];
    loop {
        // Call the Windows API function to get the module file name
        let len = unsafe { GetModuleFileNameA(0, buffer.as_mut_ptr(), buffer.len() as u32) };

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
                        err
                    ));
                }
            }
        } else {
            buffer.truncate(len as usize + b"\0".len());
            break;
        }
    }

    CString::from_vec_with_nul(buffer).unwrap_or_else(|_| {
        eprintln!("Executable name is not correctly null terminated.");
        exit_with_status(1)
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
    let file_handle = expect_result(
        unsafe {
            CreateFileA(
                executable_name.as_ptr() as _,
                GENERIC_READ,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                0,
            )
        },
        INVALID_HANDLE_VALUE,
        || {
            format!(
                "Failed to open executable '{}'",
                &*executable_name.to_string_lossy(),
            )
        },
    );

    let mut file_size: i64 = 0;
    // `SetFilePointerEx` supports setting the file pointer from the back, but pointing it past the file's start
    // results in an error. That's why we need to know the file size to avoid ever seeking past the start of the file.
    expect_result(
        unsafe { GetFileSizeEx(file_handle, &mut file_size) },
        0,
        || {
            format!(
                "Failed to get the size of the executable '{}'",
                &*executable_name.to_string_lossy(),
            )
        },
    );

    // Start with a size of 1024 bytes which should be enough for most paths but avoids reading the
    // entire file.
    let mut buffer: Vec<u8> = Vec::new();
    let mut bytes_to_read = 1024.min(u32::try_from(file_size).unwrap_or(u32::MAX));

    let path = loop {
        // SAFETY: Casting to usize is safe because we only support 64bit systems where usize is guaranteed to be larger than u32.
        buffer.resize(bytes_to_read as usize, 0);

        expect_result(
            unsafe {
                SetFilePointerEx(
                    file_handle,
                    file_size - i64::from(bytes_to_read),
                    null_mut(),
                    FILE_BEGIN,
                )
            },
            0,
            || String::from("Failed to set the file pointer to the end of the file."),
        );

        let mut read_bytes = 0u32;

        expect_result(
            unsafe {
                ReadFile(
                    file_handle,
                    buffer.as_mut_ptr() as *mut _,
                    bytes_to_read,
                    &mut read_bytes,
                    null_mut(),
                )
            },
            0,
            || String::from("Failed to read the executable file"),
        );

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
                    exit_with_status(1)
                }));

                if path_len > MAX_PATH_LEN {
                    eprintln!("Only paths with a length up to 2MBs are supported but the python path has a length of {}.", path_len);
                    exit_with_status(1);
                }

                // SAFETY: path len is guaranteed to be less than 2MB
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
                eprintln!("Python executable path is not correctly null terminated.");
                exit_with_status(1)
            });
        } else {
            // SAFETY: Casting to u32 is safe because `path_len` is guaranteed to be less than 2MB,
            // MAGIC_NUMBER is 4 bytes and PATH_LEN_SIZE is 4 bytes.
            bytes_to_read = (path_len + MAGIC_NUMBER.len() + PATH_LEN_SIZE) as u32;

            if i64::from(bytes_to_read) > file_size {
                eprintln!("The length of the python executable path exceeds the file size. Verify that the path length is appended to the end of the launcher script as a u32 in little endian.");
                exit_with_status(1);
            }
        }
    };

    expect_result(unsafe { CloseHandle(file_handle) }, 0, || {
        String::from("Failed to close file handle")
    });

    path
}

fn push_arguments(output: &mut Vec<u8>) {
    let arguments_as_str = unsafe {
        // SAFETY: We rely on `GetCommandLineA` to return a valid pointer to a null terminated string.
        CStr::from_ptr(GetCommandLineA() as _)
    };

    // Skip over the executable name and then push the rest of the arguments
    let after_executable = skip_one_argument(arguments_as_str.to_bytes());

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
    unsafe {
        let job = CreateJobObjectW(null(), null());
        let mut job_info = MaybeUninit::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>::uninit();
        let mut retlen = 0u32;
        expect_result(
            QueryInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                job_info.as_mut_ptr() as *mut _,
                job_info.size_of(),
                &mut retlen as *mut _,
            ),
            0,
            || String::from("Error from QueryInformationJobObject"),
        );
        let mut job_info = job_info.assume_init();
        job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        expect_result(
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                addr_of!(job_info) as *const _,
                job_info.size_of(),
            ),
            0,
            || String::from("Error from SetInformationJobObject"),
        );
        job
    }
}

fn spawn_child(si: &STARTUPINFOA, child_cmdline: CString) -> HANDLE {
    unsafe {
        if si.dwFlags & STARTF_USESTDHANDLES != 0 {
            // ignore errors from these -- if the handle's not inheritable/not valid, then nothing
            // we can do
            SetHandleInformation(si.hStdInput, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            SetHandleInformation(si.hStdOutput, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            SetHandleInformation(si.hStdError, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
        }
        let mut child_process_info = MaybeUninit::<PROCESS_INFORMATION>::uninit();
        expect_result(
            CreateProcessA(
                null(),
                // Why does this have to be mutable? Who knows. But it's not a mistake --
                // MS explicitly documents that this buffer might be mutated by CreateProcess.
                child_cmdline.as_ptr().cast_mut() as _,
                null(),
                null(),
                1,
                0,
                null(),
                null(),
                addr_of!(*si),
                child_process_info.as_mut_ptr(),
            ),
            0,
            || String::from("Failed to spawn the python child process"),
        );
        let child_process_info = child_process_info.assume_init();
        CloseHandle(child_process_info.hThread);
        child_process_info.hProcess
    }
}

// Apparently, the Windows C runtime has a secret way to pass file descriptors into child
// processes, by using the .lpReserved2 field. We want to close those file descriptors too.
// The UCRT source code has details on the memory layout (see also initialize_inherited_file_handles_nolock):
//   https://github.com/huangqinjin/ucrt/blob/10.0.19041.0/lowio/ioinit.cpp#L190-L223
fn close_handles(si: &STARTUPINFOA) {
    unsafe {
        for handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE] {
            CloseHandle(GetStdHandle(handle));
            SetStdHandle(handle, INVALID_HANDLE_VALUE);
        }

        if si.cbReserved2 == 0 || si.lpReserved2.is_null() {
            return;
        }
        let crt_magic = si.lpReserved2 as *const u32;
        let handle_count = crt_magic.read_unaligned() as isize;
        let handle_start = crt_magic.offset(1 + handle_count);
        for i in 0..handle_count {
            CloseHandle(handle_start.offset(i).read_unaligned() as HANDLE);
        }
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
    unsafe {
        PostMessageA(0, 0, 0, 0);
        let mut msg = MaybeUninit::<MSG>::uninit();
        GetMessageA(msg.as_mut_ptr(), 0, 0, 0);
        WaitForInputIdle(child_handle, INFINITE);
        let hwnd = CreateWindowExA(
            0,
            c!("STATIC").as_ptr() as *const _,
            c!("uv Python Trampoline").as_ptr() as *const _,
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            0,
            0,
            null(),
        );
        PeekMessageA(msg.as_mut_ptr(), hwnd, 0, 0, 0);
        DestroyWindow(hwnd);
    }
}

pub fn bounce(is_gui: bool) -> ! {
    unsafe {
        let child_cmdline = make_child_cmdline();

        let mut si = MaybeUninit::<STARTUPINFOA>::uninit();
        GetStartupInfoA(si.as_mut_ptr());
        let si = si.assume_init();

        let child_handle = spawn_child(&si, child_cmdline);
        let job = make_job_object();
        expect_result(AssignProcessToJobObject(job, child_handle), 0, || {
            String::from("Error from AssignProcessToJobObject")
        });

        // (best effort) Close all the handles that we can
        close_handles(&si);

        // (best effort) Switch to some innocuous directory so we don't hold the original
        // cwd open.
        if let Some(tmp) = getenv(c!("TEMP")) {
            SetCurrentDirectoryA(tmp.as_ptr() as *const _);
        } else {
            SetCurrentDirectoryA(c!("c:\\").as_ptr() as *const _);
        }

        // We want to ignore control-C/control-Break/logout/etc.; the same event will
        // be delivered to the child, so we let them decide whether to exit or not.
        unsafe extern "system" fn control_key_handler(_: u32) -> BOOL {
            1
        }
        SetConsoleCtrlHandler(Some(control_key_handler), 1);

        if is_gui {
            clear_app_starting_state(child_handle);
        }

        WaitForSingleObject(child_handle, INFINITE);
        let mut exit_code = 0u32;
        expect_result(
            GetExitCodeProcess(child_handle, addr_of_mut!(exit_code)),
            0,
            || String::from("Error from GetExitCodeProcess"),
        );
        exit_with_status(exit_code);
    }
}

/// Unwraps the result of the C call by asserting that it doesn't match the `error_code`.
///
/// Prints the passed error message if the `actual_result` is equal to `error_code` and exits the process with status 1.
#[inline]
fn expect_result<T, F>(actual_result: T, error_code: T, error_message: F) -> T
where
    T: Eq,
    F: FnOnce() -> String,
{
    if actual_result == error_code {
        print_last_error_and_exit(&error_message());
    }

    actual_result
}

#[cold]
fn print_last_error_and_exit(message: &str) -> ! {
    use windows_sys::Win32::{
        Foundation::*,
        System::Diagnostics::Debug::{
            FormatMessageA, FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM,
            FORMAT_MESSAGE_IGNORE_INSERTS,
        },
    };

    let err = unsafe { GetLastError() };
    eprintln!("Received error code: {}", err);
    let mut msg_ptr: *mut u8 = core::ptr::null_mut();
    let size = unsafe {
        FormatMessageA(
            FORMAT_MESSAGE_ALLOCATE_BUFFER
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            null(),
            err,
            0,
            // Weird calling convention: this argument is typed as *mut u16,
            // but if you pass FORMAT_MESSAGE_ALLOCATE_BUFFER then you have to
            // *actually* pass in a *mut *mut u16 and just lie about the type.
            // Getting Rust to do this requires some convincing.
            core::ptr::addr_of_mut!(msg_ptr) as *mut _ as _,
            0,
            core::ptr::null(),
        )
    };

    if size == 0 {
        eprintln!(
            "{}: with code {} (failed to get error message)",
            message, err
        );
    } else {
        let reason = unsafe {
            let reason = core::slice::from_raw_parts(msg_ptr, size as usize + 1);
            CStr::from_bytes_with_nul_unchecked(reason)
        };
        eprintln!(
            "(uv internal error) {}: {}",
            message,
            &*reason.to_string_lossy()
        );
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
