use core::mem::MaybeUninit;
use core::{
    ffi::CStr,
    ptr::{addr_of, addr_of_mut, null, null_mut},
};

use alloc::{ffi::CString, vec, vec::Vec};
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
use crate::{c, check, eprintln};

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
fn make_child_cmdline(is_gui: bool) -> Vec<u8> {
    let executable_name: CString = executable_filename();
    let python_exe = find_python_exe(is_gui, &executable_name);
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

    eprintln!(
        "executable_name: '{}'\nnew_cmdline: {}",
        unsafe { core::str::from_utf8_unchecked(executable_name.to_bytes()) },
        unsafe { core::str::from_utf8_unchecked(child_cmdline.as_slice()) }
    );

    child_cmdline
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
    unsafe {
        // MAX_PATH is a lie, Windows paths can be longer.
        // https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#maximum-path-length-limitation
        // But it's a good first guess, usually paths are short and we should only need a single attempt.
        let mut buffer: Vec<u8> = vec![0; MAX_PATH as usize];
        loop {
            // Call the Windows API function to get the module file name
            let len = GetModuleFileNameA(0, buffer.as_mut_ptr(), buffer.len() as u32);

            // That's the error condition because len doesn't include the trailing null byte
            if len as usize == buffer.len() {
                let last_error = GetLastError();
                match last_error {
                    ERROR_INSUFFICIENT_BUFFER => {
                        SetLastError(ERROR_SUCCESS);
                        // Try again with twice the size
                        buffer.resize(buffer.len() * 2, 0);
                    }
                    err => {
                        eprintln!("Failed to get executable name: code {}", err);
                        ExitProcess(1);
                    }
                }
            } else {
                buffer.truncate(len as usize + b"\0".len());
                break;
            }
        }
        CString::from_vec_with_nul_unchecked(buffer)
    }
}

/// The scripts are in the same directory as the Python interpreter, so we can find Python by getting the locations of
/// the current .exe and replacing the filename with `python[w].exe`.
fn find_python_exe(is_gui: bool, executable_name: &CStr) -> CString {
    // Replace the filename (the last segment of the path) with "python.exe"
    // Assumption: We are not in an encoding where a backslash byte can be part of a larger character.
    let bytes = executable_name.to_bytes();
    let Some(last_backslash) = bytes.iter().rposition(|byte| *byte == b'\\') else {
        eprintln!(
            "Invalid current exe path (missing backslash): `{}`",
            &*executable_name.to_string_lossy()
        );
        unsafe {
            ExitProcess(1);
        }
    };

    let mut buffer = bytes[..last_backslash + 1].to_vec();
    buffer.extend_from_slice(if is_gui {
        b"pythonw.exe"
    } else {
        b"python.exe"
    });
    buffer.push(b'\0');

    unsafe { CString::from_vec_with_nul_unchecked(buffer) }
}

fn push_arguments(output: &mut Vec<u8>) {
    let arguments_as_str = unsafe { CStr::from_ptr(GetCommandLineA() as _) };

    // Skip over the executable name and then push the rest of the arguments
    let after_executable = skip_one_argument(arguments_as_str.to_bytes());

    output.extend_from_slice(after_executable)
}

// TODO copy tests from MSDN for parsing
fn skip_one_argument(arguments: &[u8]) -> &[u8] {
    let mut quoted = false;
    let mut offset = 0;
    let mut bytes_iter = arguments.iter().peekable();

    // Implements https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments?view=msvc-170
    while let Some(byte) = bytes_iter.next().copied() {
        match byte {
            b'"' => {
                if quoted {
                    offset += 1;
                    break;
                } else {
                    quoted = true;
                }
            }
            b'\\' => {
                if bytes_iter.peek().copied() == Some(&b'\"') {
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
        check!(QueryInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            job_info.as_mut_ptr() as *mut _,
            job_info.size_of(),
            &mut retlen as *mut _,
        ));
        let mut job_info = job_info.assume_init();
        job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        check!(SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            addr_of!(job_info) as *const _,
            job_info.size_of(),
        ));
        job
    }
}

fn spawn_child(si: &STARTUPINFOA, child_cmdline: &mut [u8]) -> HANDLE {
    unsafe {
        if si.dwFlags & STARTF_USESTDHANDLES != 0 {
            // ignore errors from these -- if the handle's not inheritable/not valid, then nothing
            // we can do
            SetHandleInformation(si.hStdInput, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            SetHandleInformation(si.hStdOutput, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
            SetHandleInformation(si.hStdError, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
        }
        let mut child_process_info = MaybeUninit::<PROCESS_INFORMATION>::uninit();
        check!(CreateProcessA(
            null(),
            // Why does this have to be mutable? Who knows. But it's not a mistake --
            // MS explicitly documents that this buffer might be mutated by CreateProcess.
            child_cmdline.as_mut_ptr(),
            null(),
            null(),
            1,
            0,
            null(),
            null(),
            addr_of!(*si),
            child_process_info.as_mut_ptr(),
        ));
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
        let mut child_cmdline = make_child_cmdline(is_gui);
        let job = make_job_object();

        let mut si = MaybeUninit::<STARTUPINFOA>::uninit();
        GetStartupInfoA(si.as_mut_ptr());
        let si = si.assume_init();

        let child_handle = spawn_child(&si, child_cmdline.as_mut_slice());
        check!(AssignProcessToJobObject(job, child_handle));

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
        check!(GetExitCodeProcess(child_handle, addr_of_mut!(exit_code)));
        ExitProcess(exit_code);
    }
}
