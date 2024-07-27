#![allow(clippy::disallowed_types)]
use std::ffi::{c_void, OsString};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::ChildExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::vec::Vec;

use windows::core::s;
use windows::Win32::{
    Foundation::{CloseHandle, BOOL, HANDLE, INVALID_HANDLE_VALUE, TRUE},
    System::Console::{
        GetStdHandle, SetConsoleCtrlHandler, SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    },
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectA, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
    },
    System::Threading::{
        GetExitCodeProcess, GetStartupInfoA, WaitForInputIdle, WaitForSingleObject, INFINITE,
        STARTUPINFOA,
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
fn find_python_exe(executable_name: &Path) -> PathBuf {
    let mut file_handle = File::open(executable_name).unwrap_or_else(|_| {
        print_last_error_and_exit(&format!(
            "Failed to open executable '{}'",
            &*executable_name.to_string_lossy(),
        ));
    });

    let metadata = executable_name.metadata().unwrap_or_else(|_| {
        print_last_error_and_exit(&format!(
            "Failed to get the size of the executable '{}'",
            &*executable_name.to_string_lossy(),
        ));
    });
    let file_size = metadata.len();

    // Start with a size of 1024 bytes which should be enough for most paths but avoids reading the
    // entire file.
    let mut buffer: Vec<u8> = Vec::new();
    let mut bytes_to_read = 1024.min(u32::try_from(file_size).unwrap_or(u32::MAX));

    let path: String = loop {
        // SAFETY: Casting to usize is safe because we only support 64bit systems where usize is guaranteed to be larger than u32.
        buffer.resize(bytes_to_read as usize, 0);

        file_handle
            .seek(SeekFrom::Start(file_size - u64::from(bytes_to_read)))
            .unwrap_or_else(|_| {
                print_last_error_and_exit("Failed to set the file pointer to the end of the file");
            });

        // Pulls in core::fmt::{write, Write, getcount}
        let read_bytes = file_handle.read(&mut buffer).unwrap_or_else(|_| {
            print_last_error_and_exit("Failed to read the executable file");
        });

        // Truncate the buffer to the actual number of bytes read.
        buffer.truncate(read_bytes);

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

            break String::from_utf8(buffer).unwrap_or_else(|_| {
                eprintln!("Python executable path is not a valid UTF-8 encoded path");
                exit_with_status(1);
            });
        } else {
            // SAFETY: Casting to u32 is safe because `path_len` is guaranteed to be less than 32KBs,
            // MAGIC_NUMBER is 4 bytes and PATH_LEN_SIZE is 4 bytes.
            bytes_to_read = (path_len + MAGIC_NUMBER.len() + PATH_LEN_SIZE) as u32;

            if u64::from(bytes_to_read) > file_size {
                eprintln!("The length of the python executable path exceeds the file size. Verify that the path length is appended to the end of the launcher script as a u32 in little endian");
                exit_with_status(1);
            }
        }
    };

    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        let parent_dir = match executable_name.parent() {
            Some(parent) => parent,
            None => {
                eprintln!("Executable path has no parent directory");
                exit_with_status(1);
            }
        };
        parent_dir.join(path)
    };

    // NOTICE: dunce adds 5kb~
    dunce::canonicalize(path.as_path()).unwrap_or_else(|_| {
        eprintln!("Failed to canonicalize script path");
        exit_with_status(1);
    })
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
    let launcher_exe = std::env::current_exe().unwrap_or_else(|_| {
        eprintln!("Failed to get executable name");
        exit_with_status(1);
    });
    let python_exe = find_python_exe(launcher_exe.as_ref());
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();

    let mut si = STARTUPINFOA::default();
    unsafe { GetStartupInfoA(&mut si) }

    let child_process_info = Command::new(python_exe.as_os_str())
        .arg(launcher_exe.as_os_str())
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|_| {
            print_last_error_and_exit("Failed to spawn the python child process");
        });
    // https://github.com/rust-lang/rust/issues/96723
    // We need to close the main_thread_handle to align with distlib implementation
    unsafe {
        CloseHandle(HANDLE(
            child_process_info.main_thread_handle().as_raw_handle() as _,
        ))
    }
    .unwrap_or_else(|_| {
        print_last_error_and_exit("Failed to close handle to python child process main thread");
    });

    let child_handle = HANDLE(child_process_info.as_raw_handle() as _);

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
    let err = std::io::Error::last_os_error();
    let err_no_str = err
        .raw_os_error()
        .map(|raw_error| format!(" (os error {})", raw_error))
        .unwrap_or_default();
    // we can't access sys::os::error_string directly so err.kind().to_string()
    // is the closest we can get to while avoiding bringing in a large chunk of core::fmt
    eprintln!(
        "(uv internal error) {}: {}.{}",
        message,
        err.kind().to_string(),
        err_no_str
    );
    exit_with_status(1);
}

#[cold]
fn exit_with_status(code: u32) -> ! {
    // ~5-10kb
    // Pulls in core::fmt::{write, Write, getcount}
    std::process::exit(code as _)
}
