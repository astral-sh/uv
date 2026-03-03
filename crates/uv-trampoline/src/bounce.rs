#![allow(clippy::disallowed_types)]
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::vec::Vec;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::{
    Foundation::{
        CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, TRUE,
    },
    Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType},
    System::Console::{GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle},
    System::Environment::GetCommandLineA,
    System::LibraryLoader::{FindResourceW, LoadResource, LockResource, SizeofResource},
    System::Threading::{
        CreateProcessA, GetExitCodeProcess, GetStartupInfoA, INFINITE, PROCESS_CREATION_FLAGS,
        PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOA, WaitForInputIdle,
        WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{
        CreateWindowExA, DestroyWindow, GetMessageA, HWND_MESSAGE, MSG, PEEK_MESSAGE_REMOVE_TYPE,
        PeekMessageA, PostMessageA, WINDOW_EX_STYLE, WINDOW_STYLE,
    },
};
use windows::core::{PSTR, s};

use uv_windows::{Job, install_ctrl_handler};

use uv_static::EnvVars;

use crate::{error, format, warn};

// https://learn.microsoft.com/en-us/windows/win32/menurc/resource-types
const RT_RCDATA: u16 = 10;

/// Resource IDs for the trampoline metadata
const RESOURCE_TRAMPOLINE_KIND: windows::core::PCWSTR = windows::core::w!("UV_TRAMPOLINE_KIND");
const RESOURCE_PYTHON_PATH: windows::core::PCWSTR = windows::core::w!("UV_PYTHON_PATH");

/// The kind of trampoline.
enum TrampolineKind {
    /// The trampoline should execute itself, it's a zipped Python script.
    Script,
    /// The trampoline should just execute Python, it's a proxy Python executable.
    Python,
}

impl TrampolineKind {
    fn from_resource(data: &[u8]) -> Option<Self> {
        match data.first() {
            Some(1) => Some(Self::Script),
            Some(2) => Some(Self::Python),
            _ => None,
        }
    }
}

/// Safely loads a resource from the current module
fn load_resource(resource_id: windows::core::PCWSTR) -> Option<Vec<u8>> {
    // SAFETY: winapi calls; null-terminated strings; all pointers are checked.
    unsafe {
        // Find the resource
        let resource = FindResourceW(
            None,
            resource_id,
            windows::core::PCWSTR(RT_RCDATA as *const _),
        );
        if resource.is_invalid() {
            return None;
        }

        // Get resource size and data
        let size = SizeofResource(None, resource);
        if size == 0 {
            return None;
        }
        let data = LoadResource(None, resource).ok();
        let ptr = LockResource(data?) as *const u8;
        if ptr.is_null() {
            return None;
        }

        // Copy the resource data into a Vec
        Some(std::slice::from_raw_parts(ptr, size as usize).to_vec())
    }
}

/// Transform `<command> <arguments>` to `python <command> <arguments>` or `python <arguments>`
/// depending on the [`TrampolineKind`].
fn make_child_cmdline() -> CString {
    let executable_name = std::env::current_exe().unwrap_or_else(|_| {
        error_and_exit("uv trampoline failed to determine executable path");
    });

    // Load trampoline kind
    let trampoline_kind = load_resource(RESOURCE_TRAMPOLINE_KIND)
        .and_then(|data| TrampolineKind::from_resource(&data))
        .unwrap_or_else(|| {
            error_and_exit("uv trampoline failed to load trampoline kind from resources")
        });

    // Load Python path
    let python_path = load_resource(RESOURCE_PYTHON_PATH)
        .and_then(|data| String::from_utf8(data).ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            error_and_exit("uv trampoline failed to load Python path from resources")
        });

    let python_exe = if python_path.is_absolute() {
        python_path
    } else {
        let parent_dir = match executable_name.parent() {
            Some(parent) => parent,
            None => {
                error_and_exit("uv trampoline executable path has no parent directory");
            }
        };
        parent_dir.join(python_path)
    };

    let python_exe =
        if !python_exe.is_absolute() || matches!(trampoline_kind, TrampolineKind::Script) {
            // NOTICE: dunce adds 5kb~
            // TODO(john): In order to avoid resolving junctions and symlinks for relative paths and
            // scripts, we can consider reverting https://github.com/astral-sh/uv/pull/5750/files#diff-969979506be03e89476feade2edebb4689a9c261f325988d3c7efc5e51de26d1L273-L277.
            dunce::canonicalize(python_exe.as_path()).unwrap_or_else(|_| {
                error_and_exit("uv trampoline failed to canonicalize script path");
            })
        } else {
            // For Python trampolines with absolute paths, we skip `dunce::canonicalize` to
            // avoid resolving junctions.
            python_exe
        };

    let mut child_cmdline = Vec::<u8>::new();
    push_quoted_path(python_exe.as_ref(), &mut child_cmdline);
    child_cmdline.push(b' ');

    // Only execute the trampoline again if it's a script, otherwise, just invoke Python.
    match trampoline_kind {
        TrampolineKind::Python => {
            // SAFETY: `std::env::set_var` is safe to call on Windows, and
            // this code only ever runs on Windows.
            unsafe {
                // Setting this env var will cause `getpath.py` to set
                // `executable` to the path to this trampoline. This is
                // the approach taken by CPython for Python Launchers
                // (in `launcher.c`). This allows virtual environments to
                // be correctly detected when using trampolines.
                std::env::set_var(EnvVars::PYVENV_LAUNCHER, &executable_name);

                // If this is not a virtual environment, set `PYTHONHOME` to
                // the parent directory of the executable. This ensures that
                // the correct installation directories are added to `sys.path`
                // when running with a junction trampoline.
                //
                // We use a marker variable (`UV_INTERNAL__PYTHONHOME`) to track
                // whether `PYTHONHOME` was set by uv. This allows us to:
                // - Override inherited `PYTHONHOME` from parent Python processes
                // - Preserve user-defined `PYTHONHOME` values
                if !is_virtualenv(python_exe.as_path()) {
                    let python_home = std::env::var(EnvVars::PYTHONHOME).ok();
                    let marker = std::env::var(EnvVars::UV_INTERNAL__PYTHONHOME).ok();

                    // Only set `PYTHONHOME` if:
                    // - It's not set, OR
                    // - It was set by uv (marker matches current `PYTHONHOME`)
                    let should_override = match (&python_home, &marker) {
                        (None, _) => true,
                        (Some(home), Some(m)) if home == m => true,
                        _ => false,
                    };

                    if should_override {
                        let home = python_exe
                            .parent()
                            .expect("Python executable should have a parent directory");
                        std::env::set_var(EnvVars::PYTHONHOME, home);
                        std::env::set_var(EnvVars::UV_INTERNAL__PYTHONHOME, home);
                    }
                }
            }
        }
        TrampolineKind::Script => {
            // Use the full executable name because CMD only passes the name of the executable (but not the path)
            // when e.g. invoking `black` instead of `<PATH_TO_VENV>/Scripts/black` and Python then fails
            // to find the file. Unfortunately, this complicates things because we now need to split the executable
            // from the arguments string...
            push_quoted_path(executable_name.as_ref(), &mut child_cmdline);
        }
    }

    push_arguments(&mut child_cmdline);

    child_cmdline.push(b'\0');

    // Helpful when debugging trampoline issues
    // warn!(
    //     "executable_name: '{}'\nnew_cmdline: {}",
    //     &*executable_name.to_string_lossy(),
    //     std::str::from_utf8(child_cmdline.as_slice()).unwrap()
    // );

    CString::from_vec_with_nul(child_cmdline).unwrap_or_else(|_| {
        error_and_exit("uv trampoline child command line is not correctly null terminated");
    })
}

fn push_quoted_path(path: &Path, command: &mut Vec<u8>) {
    command.push(b'"');
    for byte in path.as_os_str().as_encoded_bytes() {
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

/// Checks if the given executable is part of a virtual environment
///
/// Checks if a `pyvenv.cfg` file exists in grandparent directory of the given executable.
/// PEP 405 specifies a more robust procedure (checking both the parent and grandparent
/// directory and then scanning for a `home` key), but in practice we have found this to
/// be unnecessary.
fn is_virtualenv(executable: &Path) -> bool {
    executable
        .parent()
        .and_then(Path::parent)
        .map(|path| path.join("pyvenv.cfg").is_file())
        .unwrap_or(false)
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

#[cold]
fn print_job_error_and_exit(message: &str, err: uv_windows::JobError) -> ! {
    error!(
        "{}\n  Caused by: {} (os error {})",
        message,
        err.message(),
        err.code()
    );
    exit_with_status(1);
}

#[cold]
fn print_ctrl_handler_error_and_exit(err: uv_windows::CtrlHandlerError) -> ! {
    error!(
        "uv trampoline failed to set control handler\n  Caused by: os error {}",
        err.code()
    );
    exit_with_status(1);
}

fn spawn_child(si: &STARTUPINFOA, child_cmdline: CString) -> HANDLE {
    // See distlib/PC/launcher.c::run_child
    if (si.dwFlags & STARTF_USESTDHANDLES).0 != 0 {
        // ignore errors, if the handles are not inheritable/valid, then nothing we can do
        unsafe { SetHandleInformation(si.hStdInput, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| warn!("Making stdin inheritable failed"));
        unsafe { SetHandleInformation(si.hStdOutput, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| warn!("Making stdout inheritable failed"));
        unsafe { SetHandleInformation(si.hStdError, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
            .unwrap_or_else(|_| warn!("Making stderr inheritable failed"));
    }
    let mut child_process_info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessA(
            None,
            // Why does this have to be mutable? Who knows. But it's not a mistake --
            // MS explicitly documents that this buffer might be mutated by CreateProcess.
            Some(PSTR::from_raw(child_cmdline.as_ptr() as *mut _)),
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
        print_last_error_and_exit("uv trampoline failed to spawn Python child process");
    });
    unsafe { CloseHandle(child_process_info.hThread) }.unwrap_or_else(|_| {
        print_last_error_and_exit(
            "uv trampoline failed to close Python child process thread handle",
        );
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
    // Unlike cleanup_standard_io(), we don't close STD_ERROR_HANDLE to retain warn!
    for std_handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE] {
        if let Ok(handle) = unsafe { GetStdHandle(std_handle) } {
            if handle.is_invalid() || unsafe { GetFileType(handle) } != FILE_TYPE_PIPE {
                continue;
            }
            unsafe { CloseHandle(handle) }.unwrap_or_else(|_| {
                warn!("Failed to close standard device handle {}", handle.0 as u32);
            });
            unsafe { SetStdHandle(std_handle, INVALID_HANDLE_VALUE) }.unwrap_or_else(|_| {
                warn!("Failed to modify standard device handle {}", std_handle.0);
            });
        }
    }

    // See distlib/PC/launcher.c::cleanup_fds()
    if si.cbReserved2 == 0 || si.lpReserved2.is_null() {
        return;
    }

    let crt_magic = si.lpReserved2 as *const u32;
    let handle_count = unsafe { crt_magic.read_unaligned() } as isize;
    let handle_start =
        unsafe { (crt_magic.offset(1) as *const u8).offset(handle_count) as *const HANDLE };

    // Close all fds inherited from the parent, except for the standard I/O fds (skip first 3).
    for i in 3..handle_count {
        let handle = unsafe { handle_start.offset(i).read_unaligned() };
        // Ignore invalid handles, as that means this fd was not inherited.
        // -2 is a special value (https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/get-osfhandle)
        if handle.is_invalid() || handle.0 == -2 as _ {
            continue;
        }
        unsafe { CloseHandle(handle) }.unwrap_or_else(|_| {
            warn!("Failed to close child file descriptors at {}", i);
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
        PostMessageA(None, 0, WPARAM(0), LPARAM(0)).unwrap_or_else(|_| {
            warn!("Failed to post a message to specified window");
        });
        if GetMessageA(&mut msg, None, 0, 0) != TRUE {
            warn!("Failed to retrieve posted window message");
        }
        // Proxy the child's input idle event.
        if WaitForInputIdle(child_handle, INFINITE) != 0 {
            warn!("Failed to wait for input from window");
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
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        ) {
            // Process all sent messages and signal input idle.
            let _ = PeekMessageA(&mut msg, Some(hwnd), 0, 0, PEEK_MESSAGE_REMOVE_TYPE(0));
            DestroyWindow(hwnd).unwrap_or_else(|_| {
                print_last_error_and_exit("uv trampoline failed to destroy temporary window");
            });
        }
    }
}

pub fn bounce(is_gui: bool) -> ! {
    let child_cmdline = make_child_cmdline();

    let mut si = STARTUPINFOA::default();
    unsafe { GetStartupInfoA(&mut si) }

    let child_handle = spawn_child(&si, child_cmdline);
    let job = Job::new().unwrap_or_else(|e| {
        print_job_error_and_exit("uv trampoline failed to create job object", e);
    });

    // SAFETY: child_handle is a valid process handle returned by spawn_child.
    if let Err(e) = unsafe { job.assign_process(child_handle) } {
        print_job_error_and_exit(
            "uv trampoline failed to assign child process to job object",
            e,
        );
    }

    // (best effort) Close all the handles that we can
    close_handles(&si);

    // (best effort) Switch to some innocuous directory, so we don't hold the original cwd open.
    // See distlib/PC/launcher.c::switch_working_directory
    if std::env::set_current_dir(std::env::temp_dir()).is_err() {
        warn!("Failed to set cwd to temp dir");
    }

    // We want to ignore control-C/control-Break/logout/etc.; the same event will
    // be delivered to the child, so we let them decide whether to exit or not.
    if let Err(e) = install_ctrl_handler() {
        print_ctrl_handler_error_and_exit(e);
    }

    if is_gui {
        clear_app_starting_state(child_handle);
    }

    let _ = unsafe { WaitForSingleObject(child_handle, INFINITE) };
    let mut exit_code = 0u32;
    if unsafe { GetExitCodeProcess(child_handle, &mut exit_code) }.is_err() {
        print_last_error_and_exit("uv trampoline failed to get exit code of child process");
    }
    exit_with_status(exit_code);
}

#[cold]
fn error_and_exit(message: &str) -> ! {
    error!("{}", message);
    exit_with_status(1);
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
    error!(
        "{}\n  Caused by: {}{}",
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
