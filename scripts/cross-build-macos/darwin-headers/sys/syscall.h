/* Stub sys/syscall.h for macOS cross-compilation.
 *
 * The real header is part of the macOS SDK (Xcode) and is not available when
 * cross-compiling from Linux.  jemalloc unconditionally includes it, but only
 * uses the Linux-specific SYS_* / __NR_* constants that do not exist on macOS
 * anyway, so an empty stub is sufficient. */
#ifndef _SYS_SYSCALL_H_
#define _SYS_SYSCALL_H_
#endif
