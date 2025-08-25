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

/// A write target for standard error that can be safely used in an exception handler.
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
    let CONTEXT {
        Eax,
        Ebx,
        Ecx,
        Edx,
        Esi,
        Edi,
        Eip,
        Ebp,
        Esp,
        EFlags,
        ..
    } = c;
    writeln!(
        e,
        "eax={Eax:08x} ebx={Ebx:08x} ecx={Ecx:08x} edx={Edx:08x} esi={Esi:08x} edi={Edi:08x}"
    )?;
    writeln!(
        e,
        "eip={Eip:08x} ebp={Ebp:08x} esp={Esp:08x} eflags={EFlags:08x}"
    )?;
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn dump_regs(e: &mut ExceptionSafeStderr, c: &CONTEXT) -> std::fmt::Result {
    let CONTEXT {
        Rax,
        Rbx,
        Rcx,
        Rdx,
        Rsi,
        Rdi,
        Rsp,
        Rbp,
        R8,
        R9,
        R10,
        R11,
        R12,
        R13,
        R14,
        R15,
        Rip,
        EFlags,
        ..
    } = c;
    writeln!(e, "rax={Rax:016x} rbx={Rbx:016x} rcx={Rcx:016x}")?;
    writeln!(e, "rdx={Rdx:016x} rsi={Rsi:016x} rdi={Rdi:016x}")?;
    writeln!(e, "rsp={Rsp:016x} rbp={Rbp:016x}  r8={R8 :016x}")?;
    writeln!(e, " r9={R9 :016x} r10={R10:016x} r11={R11:016x}")?;
    writeln!(e, "r12={R12:016x} r13={R13:016x} r14={R14:016x}")?;
    writeln!(e, "r15={R15:016x} rip={Rip:016x} eflags={EFlags:016x}")?;
    Ok(())
}

#[cfg(target_arch = "aarch64")]
fn dump_regs(e: &mut ExceptionSafeStderr, c: &CONTEXT) -> std::fmt::Result {
    let CONTEXT { Cpsr, Sp, Pc, .. } = c;
    // SAFETY: The two variants of this anonymous union are equivalent,
    // one's an array and one has named registers.
    let regs = unsafe { c.Anonymous.Anonymous };
    let windows::Win32::System::Diagnostics::Debug::CONTEXT_0_0 {
        X0,
        X1,
        X2,
        X3,
        X4,
        X5,
        X6,
        X7,
        X8,
        X9,
        X10,
        X11,
        X12,
        X13,
        X14,
        X15,
        X16,
        X17,
        X18,
        X19,
        X20,
        X21,
        X22,
        X23,
        X24,
        X25,
        X26,
        X27,
        X28,
        Fp,
        Lr,
    } = regs;
    writeln!(e, "cpsr={Cpsr:016x}  sp={Sp :016x}  pc={Pc :016x}")?;
    writeln!(e, "  x0={X0  :016x}  x1={X1 :016x}  x2={X2 :016x}")?;
    writeln!(e, "  x3={X3  :016x}  x4={X4 :016x}  x5={X5 :016x}")?;
    writeln!(e, "  x6={X6  :016x}  x7={X7 :016x}  x8={X8 :016x}")?;
    writeln!(e, "  x9={X9  :016x} x10={X10:016x} x11={X11:016x}")?;
    writeln!(e, " x12={X12 :016x} x13={X13:016x} x14={X14:016x}")?;
    writeln!(e, " x15={X15 :016x} x16={X16:016x} x17={X17:016x}")?;
    writeln!(e, " x18={X18 :016x} x19={X19:016x} x20={X20:016x}")?;
    writeln!(e, " x21={X21 :016x} x22={X22:016x} x23={X23:016x}")?;
    writeln!(e, " x24={X24 :016x} x25={X25:016x} x26={X26:016x}")?;
    writeln!(e, " x27={X27 :016x} x28={X28:016x}")?;
    writeln!(e, "  fp={Fp  :016x}  lr={Lr :016x}")?;
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
