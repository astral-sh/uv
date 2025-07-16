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
#![allow(clippy::print_stderr)]

use windows::Win32::{
    Foundation,
    System::Diagnostics::Debug::{
        CONTEXT, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, SetUnhandledExceptionFilter,
    },
};

fn display_exception_info(name: &str, info: &[usize; 15]) {
    match info[0] {
        0 => eprintln!("{name} reading {:#x}", info[1]),
        1 => eprintln!("{name} writing {:#x}", info[1]),
        8 => eprintln!("{name} executing {:#x}", info[1]),
        _ => eprintln!("{name} from operation {} at {:#x}", info[0], info[1]),
    }
}

#[cfg(target_arch = "x86")]
fn dump_regs(c: &CONTEXT) {
    eprintln!(
        "eax={:08x} ebx={:08x} ecx={:08x} edx={:08x} esi={:08x} edi={:08x}",
        c.Eax, c.Ebx, c.Ecx, c.Edx, c.Esi, c.Edi
    );
    eprintln!(
        "eip={:08x} ebp={:08x} esp={:08x} eflags={:08x}",
        c.Eip, c.Ebp, c.Esp, c.EFlags
    );
}

#[cfg(target_arch = "x86_64")]
fn dump_regs(c: &CONTEXT) {
    eprintln!("rax={:016x} rbx={:016x} rcx={:016x}", c.Rax, c.Rbx, c.Rcx);
    eprintln!("rdx={:016x} rsx={:016x} rdi={:016x}", c.Rdx, c.Rsi, c.Rdi);
    eprintln!("rsp={:016x} rbp={:016x}  r8={:016x}", c.Rsp, c.Rbp, c.R8);
    eprintln!(" r9={:016x} r10={:016x} r11={:016x}", c.R9, c.R10, c.R11);
    eprintln!("r12={:016x} r13={:016x} r14={:016x}", c.R12, c.R13, c.R14);
    eprintln!(
        "r15={:016x} rip={:016x} eflags={:016x}",
        c.R15, c.Rip, c.EFlags
    );
}

#[cfg(target_arch = "aarch64")]
fn dump_regs(c: &CONTEXT) {
    // SAFETY: The two variants of this anonymous union are equivalent,
    // one's an array and one has named registers.
    let r = unsafe { c.Anonymous.Anonymous };
    eprintln!("cpsr={:016x}  sp={:016x}  pc={:016x}", c.Cpsr, c.Sp, c.Pc);
    eprintln!("  x0={:016x}  x1={:016x}  x2={:016x}", r.X0, r.X1, r.X2);
    eprintln!("  x3={:016x}  x4={:016x}  x5={:016x}", r.X3, r.X4, r.X5);
    eprintln!("  x6={:016x}  x7={:016x}  x8={:016x}", r.X6, r.X7, r.X8);
    eprintln!("  x9={:016x} x10={:016x} x11={:016x}", r.X9, r.X10, r.X11);
    eprintln!(" x12={:016x} x13={:016x} x14={:016x}", r.X12, r.X13, r.X14);
    eprintln!(" x15={:016x} x16={:016x} x17={:016x}", r.X15, r.X16, r.X17);
    eprintln!(" x18={:016x} x19={:016x} x20={:016x}", r.X18, r.X19, r.X20);
    eprintln!(" x21={:016x} x22={:016x} x23={:016x}", r.X21, r.X22, r.X23);
    eprintln!(" x24={:016x} x25={:016x} x26={:016x}", r.X24, r.X25, r.X26);
    eprintln!(" x27={:016x} x28={:016x}", r.X27, r.X28);
    eprintln!("  fp={:016x}  lr={:016x}", r.Fp, r.Lr);
}

unsafe extern "system" fn unhandled_exception_filter(
    exception_info: *const EXCEPTION_POINTERS,
) -> i32 {
    // TODO: Really we should not be using eprintln here because Stderr is not async-signal-safe.
    // Probably we should be calling the console APIs directly.
    eprintln!("error: unhandled exception in uv, please report a bug:");
    let mut context = None;
    // SAFETY: Pointer comes from the OS
    if let Some(info) = unsafe { exception_info.as_ref() } {
        // SAFETY: Pointer comes from the OS
        if let Some(exc) = unsafe { info.ExceptionRecord.as_ref() } {
            eprintln!(
                "code {:#X} at address {:?}",
                exc.ExceptionCode.0, exc.ExceptionAddress
            );
            match exc.ExceptionCode {
                Foundation::EXCEPTION_ACCESS_VIOLATION => {
                    display_exception_info("EXCEPTION_ACCESS_VIOLATION", &exc.ExceptionInformation);
                }
                Foundation::EXCEPTION_IN_PAGE_ERROR => {
                    display_exception_info("EXCEPTION_IN_PAGE_ERROR", &exc.ExceptionInformation);
                }
                Foundation::EXCEPTION_ILLEGAL_INSTRUCTION => {
                    eprintln!("EXCEPTION_ILLEGAL_INSTRUCTION");
                }
                Foundation::EXCEPTION_STACK_OVERFLOW => {
                    eprintln!("EXCEPTION_STACK_OVERFLOW");
                }
                _ => {}
            }
        } else {
            eprintln!("(ExceptionRecord is NULL)");
        }
        // SAFETY: Pointer comes from the OS
        context = unsafe { info.ContextRecord.as_ref() };
    } else {
        eprintln!("(ExceptionInfo is NULL)");
    }
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Disabled {
        eprintln!("note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace");
    } else {
        if let Some(context) = context {
            dump_regs(context);
        }
        eprintln!("stack backtrace:\n{backtrace:#}");
    }
    EXCEPTION_CONTINUE_SEARCH
}

/// Set up our handler for unhandled exceptions.
pub(crate) fn setup() {
    // SAFETY: winapi call
    unsafe {
        SetUnhandledExceptionFilter(Some(Some(unhandled_exception_filter)));
    }
}
