//! Helper for setting up Windows exception handling.
//!
//! Recent versions of Windows seem to no longer show dialog boxes on access violations
//! (segfaults) or similar errors. The user experience is that the command exits with
//! the exception code as its exit status and no visible output. In order to see these
//! errors both in the field and in CI, we need to install our own exception handler.
//!
//! This is a relatively simple exception handler that leans on Rust's own backtrace
//! implementation and also displays some minimal information from the exception itself.

#![allow(unsafe_code)]
// Usually we want fs_err over std::fs, but there's no advantage here, we don't
// report errors encountered while reporting an exception.
#![allow(clippy::disallowed_types)]

use std::fmt::Write;
use std::fs::File;
use std::mem::ManuallyDrop;
use std::os::windows::io::FromRawHandle;

use arrayvec::ArrayVec;
use windows::Win32::{
    Foundation,
    Globalization::CP_UTF8,
    System::Console::{
        CONSOLE_MODE, GetConsoleMode, GetConsoleOutputCP, GetStdHandle, STD_ERROR_HANDLE,
        WriteConsoleW,
    },
    System::Diagnostics::Debug::{
        CONTEXT, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, SetUnhandledExceptionFilter,
    },
};

/// Write to stderr in a way safe in an exception handler.
///
/// The exception handler can be called at any point in the execution of machine code, perhaps
/// halfway through a Rust operation. It needs to be robust to operating with unknown program
/// state, a concept that the UNIX world calls "async signal safety." In particular, we can't
/// write to `std::io::stderr()` because that takes a lock, and we could be called in the middle of
/// code that is holding that lock.
enum ExceptionSafeStderr {
    // This is a simplified version of the logic in Rust std::sys::stdio::windows, on the
    // assumption that we're only writing strs, not bytes (so we do not need to care about
    // incomplete or invalid UTF-8) and we don't care about Windows 7 or every drop of
    // performance.
    // - If stderr is a non-UTF-8 console, we need to write UTF-16 with WriteConsoleW, and we
    //   convert with encode_utf16().
    // - If stderr is not a console, we cannot use WriteConsole and must use NtWriteFile, which
    //   takes (UTF-8) bytes.
    // - If stderr is a UTF-8 console, we can do either. std uses NtWriteFile.
    // Note that we do not want to close stderr at any point, hence ManuallyDrop.
    WriteConsole(Foundation::HANDLE),
    NtWriteFile(ManuallyDrop<File>),
}

impl ExceptionSafeStderr {
    fn new() -> Result<Self, windows_result::Error> {
        // SAFETY: winapi call, no interesting parameters
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) }?;
        if handle.is_invalid() {
            return Err(windows_result::Error::empty());
        }
        let mut mode = CONSOLE_MODE::default();
        // SAFETY: winapi calls, no interesting parameters
        if unsafe {
            GetConsoleMode(handle, &raw mut mode).is_ok() && GetConsoleOutputCP() != CP_UTF8
        } {
            Ok(Self::WriteConsole(handle))
        } else {
            // SAFETY: winapi call, we just got this handle from the OS and checked it
            let file = unsafe { File::from_raw_handle(handle.0) };
            Ok(Self::NtWriteFile(ManuallyDrop::new(file)))
        }
    }

    fn write_winerror(&mut self, s: &str) -> Result<(), windows_result::Error> {
        match self {
            Self::WriteConsole(handle) => {
                // According to comments in the ReactOS source, NT's behavior is that writes of 80
                // bytes or fewer are passed in-line in the message to the console server and
                // longer writes allocate out of a shared heap with CSRSS. In an attempt to avoid
                // allocations, write in 80-byte chunks.
                let mut buf = ArrayVec::<u16, 40>::new();
                for c in s.encode_utf16() {
                    if buf.try_push(c).is_err() {
                        // SAFETY: winapi call, arrayvec guarantees the slice is valid
                        unsafe { WriteConsoleW(*handle, &buf, None, None) }?;
                        buf.clear();
                        buf.push(c);
                    }
                }
                if !buf.is_empty() {
                    // SAFETY: winapi call, arrayvec guarantees the slice is valid
                    unsafe { WriteConsoleW(*handle, &buf, None, None) }?;
                }
            }
            Self::NtWriteFile(file) => {
                use std::io::Write;
                file.write_all(s.as_bytes())?;
            }
        }
        Ok(())
    }
}

impl Write for ExceptionSafeStderr {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.write_winerror(s).map_err(|_| std::fmt::Error)
    }
}

fn display_exception_info(
    e: &mut ExceptionSafeStderr,
    name: &str,
    info: &[usize; 15],
) -> std::fmt::Result {
    match info[0] {
        0 => writeln!(e, "{name} reading {:#x}", info[1])?,
        1 => writeln!(e, "{name} writing {:#x}", info[1])?,
        8 => writeln!(e, "{name} executing {:#x}", info[1])?,
        _ => writeln!(e, "{name} from operation {} at {:#x}", info[0], info[1])?,
    }
    Ok(())
}

#[cfg(target_arch = "x86")]
fn dump_regs(e: &mut ExceptionSafeStderr, c: &CONTEXT) -> std::fmt::Result {
    writeln!(
        e,
        "eax={:08x} ebx={:08x} ecx={:08x} edx={:08x} esi={:08x} edi={:08x}",
        c.Eax, c.Ebx, c.Ecx, c.Edx, c.Esi, c.Edi
    )?;
    writeln!(
        e,
        "eip={:08x} ebp={:08x} esp={:08x} eflags={:08x}",
        c.Eip, c.Ebp, c.Esp, c.EFlags
    )?;
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn dump_regs(e: &mut ExceptionSafeStderr, c: &CONTEXT) -> std::fmt::Result {
    writeln!(
        e,
        "rax={:016x} rbx={:016x} rcx={:016x}",
        c.Rax, c.Rbx, c.Rcx
    )?;
    writeln!(
        e,
        "rdx={:016x} rsi={:016x} rdi={:016x}",
        c.Rdx, c.Rsi, c.Rdi
    )?;
    writeln!(e, "rsp={:016x} rbp={:016x}  r8={:016x}", c.Rsp, c.Rbp, c.R8)?;
    writeln!(e, " r9={:016x} r10={:016x} r11={:016x}", c.R9, c.R10, c.R11)?;
    writeln!(
        e,
        "r12={:016x} r13={:016x} r14={:016x}",
        c.R12, c.R13, c.R14
    )?;
    writeln!(
        e,
        "r15={:016x} rip={:016x} eflags={:016x}",
        c.R15, c.Rip, c.EFlags
    )?;
    Ok(())
}

#[cfg(target_arch = "aarch64")]
fn dump_regs(e: &mut ExceptionSafeStderr, c: &CONTEXT) -> std::fmt::Result {
    // SAFETY: The two variants of this anonymous union are equivalent,
    // one's an array and one has named registers.
    let r = unsafe { c.Anonymous.Anonymous };
    writeln!(
        e,
        "cpsr={:016x}  sp={:016x}  pc={:016x}",
        c.Cpsr, c.Sp, c.Pc
    )?;
    writeln!(e, "  x0={:016x}  x1={:016x}  x2={:016x}", r.X0, r.X1, r.X2)?;
    writeln!(e, "  x3={:016x}  x4={:016x}  x5={:016x}", r.X3, r.X4, r.X5)?;
    writeln!(e, "  x6={:016x}  x7={:016x}  x8={:016x}", r.X6, r.X7, r.X8)?;
    writeln!(
        e,
        "  x9={:016x} x10={:016x} x11={:016x}",
        r.X9, r.X10, r.X11
    )?;
    writeln!(
        e,
        " x12={:016x} x13={:016x} x14={:016x}",
        r.X12, r.X13, r.X14
    )?;
    writeln!(
        e,
        " x15={:016x} x16={:016x} x17={:016x}",
        r.X15, r.X16, r.X17
    )?;
    writeln!(
        e,
        " x18={:016x} x19={:016x} x20={:016x}",
        r.X18, r.X19, r.X20
    )?;
    writeln!(
        e,
        " x21={:016x} x22={:016x} x23={:016x}",
        r.X21, r.X22, r.X23
    )?;
    writeln!(
        e,
        " x24={:016x} x25={:016x} x26={:016x}",
        r.X24, r.X25, r.X26
    )?;
    writeln!(e, " x27={:016x} x28={:016x}", r.X27, r.X28)?;
    writeln!(e, "  fp={:016x}  lr={:016x}", r.Fp, r.Lr)?;
    Ok(())
}

fn dump_exception(exception_info: *const EXCEPTION_POINTERS) -> std::fmt::Result {
    let mut e = ExceptionSafeStderr::new().map_err(|_| std::fmt::Error)?;
    writeln!(e, "error: unhandled exception in uv, please report a bug:")?;
    let mut context = None;
    // SAFETY: Pointer comes from the OS
    if let Some(info) = unsafe { exception_info.as_ref() } {
        // SAFETY: Pointer comes from the OS
        if let Some(exc) = unsafe { info.ExceptionRecord.as_ref() } {
            writeln!(
                e,
                "code {:#X} at address {:?}",
                exc.ExceptionCode.0, exc.ExceptionAddress
            )?;
            match exc.ExceptionCode {
                Foundation::EXCEPTION_ACCESS_VIOLATION => {
                    display_exception_info(
                        &mut e,
                        "EXCEPTION_ACCESS_VIOLATION",
                        &exc.ExceptionInformation,
                    )?;
                }
                Foundation::EXCEPTION_IN_PAGE_ERROR => {
                    display_exception_info(
                        &mut e,
                        "EXCEPTION_IN_PAGE_ERROR",
                        &exc.ExceptionInformation,
                    )?;
                }
                Foundation::EXCEPTION_ILLEGAL_INSTRUCTION => {
                    writeln!(e, "EXCEPTION_ILLEGAL_INSTRUCTION")?;
                }
                Foundation::EXCEPTION_STACK_OVERFLOW => {
                    writeln!(e, "EXCEPTION_STACK_OVERFLOW")?;
                }
                _ => {}
            }
        } else {
            writeln!(e, "(ExceptionRecord is NULL)")?;
        }
        // SAFETY: Pointer comes from the OS
        context = unsafe { info.ContextRecord.as_ref() };
    } else {
        writeln!(e, "(ExceptionInfo is NULL)")?;
    }
    // TODO: std::backtrace does a lot of allocations, so we are no longer async-signal-safe at
    // this point, but hopefully we got a useful error message on screen already. We could do a
    // better job by using backtrace-rs directly + arrayvec.
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Disabled {
        writeln!(
            e,
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"
        )?;
    } else {
        if let Some(context) = context {
            dump_regs(&mut e, context)?;
        }
        writeln!(e, "stack backtrace:\n{backtrace:#}")?;
    }
    Ok(())
}

unsafe extern "system" fn unhandled_exception_filter(
    exception_info: *const EXCEPTION_POINTERS,
) -> i32 {
    let _ = dump_exception(exception_info);
    EXCEPTION_CONTINUE_SEARCH
}

/// Set up our handler for unhandled exceptions.
pub(crate) fn setup() {
    // SAFETY: winapi call, argument is a mostly async-signal-safe function
    unsafe {
        SetUnhandledExceptionFilter(Some(Some(unhandled_exception_filter)));
    }
}
