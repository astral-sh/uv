/* ----------------------------------------------------------------------------
Copyright (c) 2018-2025, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/
#pragma once
#ifndef MIMALLOC_PRIM_H
#define MIMALLOC_PRIM_H
#include "internal.h"             // mi_decl_hidden

// --------------------------------------------------------------------------
// This file specifies the primitive portability API.
// Each OS/host needs to implement these primitives, see `src/prim`
// for implementations on Window, macOS, WASI, and Linux/Unix.
//
// note: on all primitive functions, we always have result parameters != NULL, and:
//  addr != NULL and page aligned
//  size > 0     and page aligned
//  the return value is an error code as an `int` where 0 is success
// --------------------------------------------------------------------------

// OS memory configuration
typedef struct mi_os_mem_config_s {
  size_t  page_size;              // default to 4KiB
  size_t  large_page_size;        // 0 if not supported, usually 2MiB (4MiB on Windows)
  size_t  alloc_granularity;      // smallest allocation size (usually 4KiB, on Windows 64KiB)
  size_t  physical_memory_in_kib; // physical memory size in KiB
  size_t  virtual_address_bits;   // usually 48 or 56 bits on 64-bit systems. (used to determine secure randomization)
  bool    has_overcommit;         // can we reserve more memory than can be actually committed?
  bool    has_partial_free;       // can allocated blocks be freed partially? (true for mmap, false for VirtualAlloc)
  bool    has_virtual_reserve;    // supports virtual address space reservation? (if true we can reserve virtual address space without using commit or physical memory)
  bool    has_transparent_huge_pages;  // true if transparent huge pages are enabled (on Linux)
} mi_os_mem_config_t;

// Initialize
void _mi_prim_mem_init( mi_os_mem_config_t* config );

// Free OS memory
int _mi_prim_free(void* addr, size_t size );

// Allocate OS memory. Return NULL on error.
// The `try_alignment` is just a hint and the returned pointer does not have to be aligned.
// If `commit` is false, the virtual memory range only needs to be reserved (with no access)
// which will later be committed explicitly using `_mi_prim_commit`.
// `is_zero` is set to true if the memory was zero initialized (as on most OS's)
// The `hint_addr` address is either `NULL` or a preferred allocation address but can be ignored.
// pre: !commit => !allow_large
//      try_alignment >= _mi_os_page_size() and a power of 2
int _mi_prim_alloc(void* hint_addr, size_t size, size_t try_alignment, bool commit, bool allow_large, bool* is_large, bool* is_zero, void** addr);

// Commit memory. Returns error code or 0 on success.
// For example, on Linux this would make the memory PROT_READ|PROT_WRITE.
// `is_zero` is set to true if the memory was zero initialized (e.g. on Windows)
int _mi_prim_commit(void* addr, size_t size, bool* is_zero);

// Decommit memory. Returns error code or 0 on success. The `needs_recommit` result is true
// if the memory would need to be re-committed. For example, on Windows this is always true,
// but on Linux we could use MADV_DONTNEED to decommit which does not need a recommit.
// pre: needs_recommit != NULL
int _mi_prim_decommit(void* addr, size_t size, bool* needs_recommit);

// Reset memory. The range keeps being accessible but the content might be reset to zero at any moment.
// Returns error code or 0 on success.
int _mi_prim_reset(void* addr, size_t size);

// Reuse memory. This is called for memory that is already committed but
// may have been reset (`_mi_prim_reset`) or decommitted (`_mi_prim_decommit`) where `needs_recommit` was false.
// Returns error code or 0 on success. On most platforms this is a no-op.
int _mi_prim_reuse(void* addr, size_t size);

// Protect memory. Returns error code or 0 on success.
int _mi_prim_protect(void* addr, size_t size, bool protect);

// Allocate huge (1GiB) pages possibly associated with a NUMA node.
// `is_zero` is set to true if the memory was zero initialized (as on most OS's)
// pre: size > 0  and a multiple of 1GiB.
//      numa_node is either negative (don't care), or a numa node number.
int _mi_prim_alloc_huge_os_pages(void* hint_addr, size_t size, int numa_node, bool* is_zero, void** addr);

// Return the current NUMA node
size_t _mi_prim_numa_node(void);

// Return the number of logical NUMA nodes
size_t _mi_prim_numa_node_count(void);

// Clock ticks
mi_msecs_t _mi_prim_clock_now(void);

// Return process information (only for statistics)
typedef struct mi_process_info_s {
  mi_msecs_t  elapsed;
  mi_msecs_t  utime;
  mi_msecs_t  stime;
  size_t      current_rss;
  size_t      peak_rss;
  size_t      current_commit;
  size_t      peak_commit;
  size_t      page_faults;
} mi_process_info_t;

void _mi_prim_process_info(mi_process_info_t* pinfo);

// Default stderr output. (only for warnings etc. with verbose enabled)
// msg != NULL && _mi_strlen(msg) > 0
void _mi_prim_out_stderr( const char* msg );

// Get an environment variable. (only for options)
// name != NULL, result != NULL, result_size >= 64
bool _mi_prim_getenv(const char* name, char* result, size_t result_size);


// Fill a buffer with strong randomness; return `false` on error or if
// there is no strong randomization available.
bool _mi_prim_random_buf(void* buf, size_t buf_len);

// Called on the first thread start, and should ensure `_mi_thread_done` is called on thread termination.
void _mi_prim_thread_init_auto_done(void);

// Called on process exit and may take action to clean up resources associated with the thread auto done.
void _mi_prim_thread_done_auto_done(void);

// Called when the default theap for a thread changes
void _mi_prim_thread_associate_default_theap(mi_theap_t* theap);

// Is this thread part of a thread pool?
bool _mi_prim_thread_is_in_threadpool(void);

// Yield to other threads. Should be similar to `sleep(0)`. 
// Is called only in rare situations and does not have to be lightning fast.
void _mi_prim_thread_yield(void);

//-------------------------------------------------------------------
// Access to TLS (thread local storage) slots.
// We need fast access to both a unique thread id (in `free.c:mi_free`) and
// to a thread-local theap pointer (in `alloc.c:mi_malloc`).
// To achieve this we use specialized code for various platforms.
//-------------------------------------------------------------------

// On some libc + platform combinations we can directly access a thread-local storage (TLS) slot.
// The TLS layout depends on both the OS and libc implementation so we use specific tests for each main platform.
// If you test on another platform and it works please send a PR :-)
// see also https://akkadia.org/drepper/tls.pdf for more info on the TLS register.
//
// Note: we would like to prefer `__builtin_thread_pointer()` nowadays instead of using assembly,
// but unfortunately we can not detect support reliably (see issue #883)
// We also use it on Apple OS as we use a TLS slot for the default theap there.
#if (defined(_WIN32)) || \
    (defined(__GNUC__) && ( \
           (defined(__GLIBC__)   && (defined(__x86_64__) || defined(__i386__) || (defined(__arm__) && __ARM_ARCH >= 7) || defined(__aarch64__))) \
        || (defined(__APPLE__)   && (defined(__x86_64__) || defined(__aarch64__) || defined(__POWERPC__))) \
        || (defined(__BIONIC__)  && (defined(__x86_64__) || defined(__i386__) || (defined(__arm__) && __ARM_ARCH >= 7) || defined(__aarch64__))) \
        || (defined(__FreeBSD__) && (defined(__x86_64__) || defined(__i386__) || defined(__aarch64__))) \
        || (defined(__OpenBSD__) && (defined(__x86_64__) || defined(__i386__) || defined(__aarch64__))) \
      ))

static inline void* mi_prim_tls_slot(size_t slot) mi_attr_noexcept {
  void* res;
  const size_t ofs = (slot*sizeof(void*));
  #if defined(_WIN32)
    #if (_M_X64 || _M_AMD64) && !defined(_M_ARM64EC)
      res = (void*)__readgsqword((unsigned long)ofs);   // direct load at offset from gs
    #elif _M_IX86 && !defined(_M_ARM64EC)
      res = (void*)__readfsdword((unsigned long)ofs);   // direct load at offset from fs
    #else
      res = ((void**)NtCurrentTeb())[slot]; MI_UNUSED(ofs);
    #endif
  #elif defined(__i386__)
    __asm__("movl %%gs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x86 32-bit always uses GS
  #elif defined(__APPLE__) && defined(__x86_64__)
    __asm__("movq %%gs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x86_64 macOSX uses GS
  #elif defined(__x86_64__) && (MI_INTPTR_SIZE==4)
    __asm__("movl %%fs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x32 ABI
  #elif defined(__x86_64__)
    __asm__("movq %%fs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x86_64 Linux, BSD uses FS
  #elif defined(__arm__)
    void** tcb; MI_UNUSED(ofs);
    __asm__ volatile ("mrc p15, 0, %0, c13, c0, 3\nbic %0, %0, #3" : "=r" (tcb));
    res = tcb[slot];
  #elif defined(__aarch64__)
    void** tcb; MI_UNUSED(ofs);
    #if defined(__APPLE__) // M1, issue #343
    __asm__ volatile ("mrs %0, tpidrro_el0\nbic %0, %0, #7" : "=r" (tcb));
    #else
    __asm__ volatile ("mrs %0, tpidr_el0" : "=r" (tcb));
    #endif
    res = tcb[slot];
  #elif defined(__APPLE__) && defined(__POWERPC__) // ppc, issue #781
    MI_UNUSED(ofs);
    res = pthread_getspecific(slot);
  #else
    #define MI_HAS_TLS_SLOT 0
    MI_UNUSED(ofs);
    res = NULL;
  #endif
  return res;
}

#ifndef MI_HAS_TLS_SLOT
#define MI_HAS_TLS_SLOT 1
#endif

// setting a tls slot is only used on macOS for now
static inline void mi_prim_tls_slot_set(size_t slot, void* value) mi_attr_noexcept {
  const size_t ofs = (slot*sizeof(void*));
  #if defined(_WIN32)
    ((void**)NtCurrentTeb())[slot] = value; MI_UNUSED(ofs);
  #elif defined(__i386__)
    __asm__("movl %1,%%gs:%0" : "=m" (*((void**)ofs)) : "rn" (value) : );  // 32-bit always uses GS
  #elif defined(__APPLE__) && defined(__x86_64__)
    __asm__("movq %1,%%gs:%0" : "=m" (*((void**)ofs)) : "rn" (value) : );  // x86_64 macOS uses GS
  #elif defined(__x86_64__) && (MI_INTPTR_SIZE==4)
    __asm__("movl %1,%%fs:%0" : "=m" (*((void**)ofs)) : "rn" (value) : );  // x32 ABI
  #elif defined(__x86_64__)
    __asm__("movq %1,%%fs:%0" : "=m" (*((void**)ofs)) : "rn" (value) : );  // x86_64 Linux, BSD uses FS
  #elif defined(__arm__)
    void** tcb; MI_UNUSED(ofs);
    __asm__ volatile ("mrc p15, 0, %0, c13, c0, 3\nbic %0, %0, #3" : "=r" (tcb));
    tcb[slot] = value;
  #elif defined(__aarch64__)
    void** tcb; MI_UNUSED(ofs);
    #if defined(__APPLE__) // M1, issue #343
    __asm__ volatile ("mrs %0, tpidrro_el0\nbic %0, %0, #7" : "=r" (tcb));
    #else
    __asm__ volatile ("mrs %0, tpidr_el0" : "=r" (tcb));
    #endif
    tcb[slot] = value;
  #elif defined(__APPLE__) && defined(__POWERPC__) // ppc, issue #781
    MI_UNUSED(ofs);
    pthread_setspecific(slot, value);
  #else
    MI_UNUSED(ofs); MI_UNUSED(value);
  #endif
}

#endif


// defined in `init.c`; do not use these directly
extern mi_decl_hidden mi_decl_thread mi_theap_t* __mi_theap_main;     // theap belonging to the main heap
extern mi_decl_hidden bool _mi_process_is_initialized;                // has mi_process_init been called?


//-------------------------------------------------------------------
// Get a fast unique thread id.
//
// Getting the thread id should be performant as it is called in the
// fast path of `_mi_free` and we specialize for various platforms as
// inlined definitions. Regular code should call `init.c:_mi_thread_id()`.
// We only require _mi_prim_thread_id() to return a unique id
// for each thread (unequal to zero).
//-------------------------------------------------------------------


// Do we have __builtin_thread_pointer? This would be the preferred way to get a unique thread id
// but unfortunately, it seems we cannot test for this reliably at this time (see issue #883)
// Nevertheless, it seems needed on older graviton platforms (see issue #851).
// For now, we only enable this for specific platforms.
#if !defined(MI_USE_BUILTIN_THREAD_POINTER)   /* allow user override */
  #if !defined(__APPLE__)  /* on apple (M1) the wrong register is read (tpidr_el0 instead of tpidrro_el0) so fall back to TLS slot assembly (<https://github.com/microsoft/mimalloc/issues/343#issuecomment-763272369>)*/ \
      && !defined(__CYGWIN__) \
      && !defined(MI_LIBC_MUSL) \
      && (!defined(__clang_major__) || __clang_major__ >= 14)  /* older clang versions emit bad code; fall back to using the TLS slot (<https://lore.kernel.org/linux-arm-kernel/202110280952.352F66D8@keescook/T/>) */
    #if    (defined(__GNUC__) && (__GNUC__ >= 7)  && defined(__aarch64__)) /* aarch64 for older gcc versions (issue #851) */ \
        || (defined(__GNUC__) && (__GNUC__ >= 11) && defined(__x86_64__)) \
        || (defined(__clang_major__) && (__clang_major__ >= 14) && (defined(__aarch64__) || defined(__x86_64__)))
      #define MI_USE_BUILTIN_THREAD_POINTER  1
    #endif
  #endif
#endif

static inline mi_threadid_t __mi_prim_thread_id(void) mi_attr_noexcept;

static inline mi_threadid_t _mi_prim_thread_id(void) mi_attr_noexcept {
  const mi_threadid_t tid = __mi_prim_thread_id();
  mi_assert_internal(tid > 1);
  mi_assert_internal((tid & MI_PAGE_FLAG_MASK) == 0);  // bottom 2 bits are clear?
  return tid;
}

// Get a unique id for the current thread.
#if defined(MI_PRIM_THREAD_ID)

static inline mi_threadid_t _mi_prim_thread_id(void) mi_attr_noexcept {
  const mi_threadid_t tid = MI_PRIM_THREAD_ID();  // used for example by CPython for a free threaded build (see python/cpython#115488)
  mi_assert_internal( (tid & 0x03) == 0 );        // mimalloc reserves the bottom 2 bits
  return tid;
}

#elif defined(_WIN32)

static inline mi_threadid_t __mi_prim_thread_id(void) mi_attr_noexcept {
  // Windows: works on Intel and ARM in both 32- and 64-bit
  return (uintptr_t)NtCurrentTeb();
}

#elif MI_USE_BUILTIN_THREAD_POINTER

static inline mi_threadid_t __mi_prim_thread_id(void) mi_attr_noexcept {
  // Works on most Unix based platforms with recent compilers
  return (uintptr_t)__builtin_thread_pointer();
}

#elif MI_HAS_TLS_SLOT

static inline mi_threadid_t __mi_prim_thread_id(void) mi_attr_noexcept {
  #if defined(__BIONIC__)
    // issue #384, #495: on the Bionic libc (Android), slot 1 is the thread id
    // see: https://github.com/aosp-mirror/platform_bionic/blob/c44b1d0676ded732df4b3b21c5f798eacae93228/libc/platform/bionic/tls_defines.h#L86
    return (uintptr_t)mi_prim_tls_slot(1);
  #else
    // in all our other targets, slot 0 is the thread id
    // glibc: https://sourceware.org/git/?p=glibc.git;a=blob_plain;f=sysdeps/x86_64/nptl/tls.h
    // apple: https://github.com/apple/darwin-xnu/blob/main/libsyscall/os/tsd.h#L36
    return (uintptr_t)mi_prim_tls_slot(0);
  #endif
}

#else

// otherwise use portable C, taking the address of a thread local variable (this is still very fast on most platforms).
static inline mi_threadid_t __mi_prim_thread_id(void) mi_attr_noexcept {
  return (uintptr_t)&__mi_theap_main;
}

#endif



/* ----------------------------------------------------------------------------------------
Get the thread local default theap: `_mi_theap_default()` (and the cached heap `_mi_theap_cached`).

This is inlined here as it is on the fast path for allocation functions.
We have 4 models:

- MI_TLS_MODEL_THREAD_LOCAL: use regular thread local (default on Linux, FreeBSD, etc)
    On most platforms (Linux, FreeBSD, NetBSD, etc), this just returns a
    thread local variable (`__mi_theap_default`). With the initial-exec TLS model this ensures
    that the storage will always be available and properly initialized (with an empty theap).

    On some platforms the underlying TLS implementation (or the loader) will call itself `malloc`
    on a first access to a thread local and recurse in the MI_TLS_MODEL_THREAD_LOCAL.
    A way around this is to define MI_TLS_RECURSE_GUARD which adds an extra check if the process
    is initialized before accessing the thread-local. This is a check in the fast path though
    so this should be avoided.

- MI_TLS_MODEL_FIXED_SLOT: use a fixed slot in the TLS block (default on macOS)
    This reserves an unused and fixed TLS slot. This is fast and avoids the problem
    where the underlying TLS implementation (or the loader) will call itself `malloc`
    on a first access to a thread local (and recurse in the MI_TLS_MODEL_THREAD_LOCAL).
    This goes wrong though if the OS or a library uses the same fixed slot.

- MI_TLS_MODEL_DYNAMIC_WIN32: use a dynamically allocated slot with TlsAlloc. (default on Windows)
    Windows has somewhat slow thread locals so by default we use TlsAlloc'd slots which
    can be more efficient. First tries to use one of the "direct" first 64 slots which 
    are the fastest, but falls back to using "expansion" slots when needed (up to 1088 slots).
    (If the allocated slot happens to always be under 64 for a particular program,
    one might use cmake with `-DMI_WIN_DIRECT_TLS=ON` to skip the expansion slot test in the fast path.)

- MI_TLS_MODEL_DYNAMIC_PTHREADS: use `pthread_getspecific`. (default on OpenBSD, maybe good for Android as well?)
    Use pthread local storage. Somewhat slow but can work well depending on the platform.

Each model should define `MI_THEAP_INITASNULL` to signify that the initial value
returned from `_mi_theap_default()` can be `NULL` (instead of the address of the empty heap).
This incurs an extra check in the fast path (but can often be combined in an existing check).
------------------------------------------------------------------------------------------- */

static inline mi_theap_t* _mi_theap_default(void);
static inline mi_theap_t* _mi_theap_cached(void);

#if defined(_WIN32)
  #define MI_TLS_MODEL_DYNAMIC_WIN32        1    
#elif defined(__APPLE__) && MI_HAS_TLS_SLOT && !defined(__POWERPC__)  // macOS on arm64 or x64
  // #define MI_TLS_MODEL_DYNAMIC_PTHREADS  1    // also works but a bit slower
  #define MI_TLS_MODEL_FIXED_SLOT           1
  #define MI_TLS_MODEL_FIXED_SLOT_DEFAULT   108  // seems unused. @apple: it would be great to get 2 official slots for custom allocators :-)
  #define MI_TLS_MODEL_FIXED_SLOT_CACHED    109
  // we used before __PTK_FRAMEWORK_OLDGC_KEY9 (89) but that seems used now.
  // see <https://github.com/rweichler/substrate/blob/master/include/pthread_machdep.h>
#elif defined(__APPLE__) || defined(__OpenBSD__) || defined(__ANDROID__)
  #define MI_TLS_MODEL_DYNAMIC_PTHREADS     1
  // #define MI_TLS_MODEL_DYNAMIC_PTHREADS_DEFAULT_ENTRY_IS_NULL  1
#else
  #define MI_TLS_MODEL_THREAD_LOCAL         1
#endif

// Declared this way to optimize register spills and branches
mi_decl_cold mi_decl_noinline mi_theap_t* _mi_theap_empty_get(void);

static inline mi_theap_t* __mi_theap_empty(void) {
  #if __GNUC__
  __asm("");  // prevent conditional load
  return (mi_theap_t*)&_mi_theap_empty;
  #else
  return _mi_theap_empty_get();
  #endif
}

#if MI_TLS_MODEL_THREAD_LOCAL
// Thread local with an initial value (default on Linux). Very efficient.

extern mi_decl_hidden mi_decl_thread mi_theap_t* __mi_theap_default;  // default theap to allocate from
extern mi_decl_hidden mi_decl_thread mi_theap_t* __mi_theap_cached;   // theap from the last used heap

static inline mi_theap_t* _mi_theap_default(void) {
  #if defined(MI_TLS_RECURSE_GUARD)
  if (mi_unlikely(!_mi_process_is_initialized)) return _mi_theap_empty_get();
  #endif
  return __mi_theap_default;
}

static inline mi_theap_t* _mi_theap_cached(void) {
  return __mi_theap_cached;
}

#elif MI_TLS_MODEL_FIXED_SLOT
// Fixed TLS slot (default on macOS).
#define MI_THEAP_INITASNULL  1

static inline mi_theap_t* _mi_theap_default(void) {
  return (mi_theap_t*)mi_prim_tls_slot(MI_TLS_MODEL_FIXED_SLOT_DEFAULT);
}

static inline mi_theap_t* _mi_theap_cached(void) {
  return (mi_theap_t*)mi_prim_tls_slot(MI_TLS_MODEL_FIXED_SLOT_CACHED);
}

#elif MI_TLS_MODEL_DYNAMIC_WIN32
// Dynamic TLS slot (default on Windows)
#define MI_THEAP_INITASNULL  1

// We try to use direct slots (64), but can also use the expansion slots (upto 1024 extra available)
// See <https://www.geoffchappell.com/studies/windows/km/ntoskrnl/inc/api/pebteb/teb/index.htm> for the offsets.
#if MI_SIZE_SIZE==4
#define MI_TLS_EXPANSION_SLOT    (0x0F94 / MI_SIZE_SIZE)
#else
#define MI_TLS_EXPANSION_SLOT    (0x1780 / MI_SIZE_SIZE)
#endif

extern mi_decl_hidden size_t _mi_theap_default_slot;
extern mi_decl_hidden size_t _mi_theap_cached_slot;
extern mi_decl_hidden size_t _mi_theap_default_expansion_slot;
extern mi_decl_hidden size_t _mi_theap_cached_expansion_slot;

static inline mi_theap_t* _mi_theap_default(void) {
  const size_t slot = _mi_theap_default_slot;
  mi_theap_t* theap  = (mi_theap_t*)mi_prim_tls_slot(slot);
  #if !MI_WIN_DIRECT_TLS
  if mi_unlikely(slot==MI_TLS_EXPANSION_SLOT) { // in TlsExpansionSlots ?
    if mi_likely(theap!=NULL) {                 // initialized (on this thread)?
      theap = ((mi_theap_t**)theap)[_mi_theap_default_expansion_slot];
    }
  }
  #endif
  return theap;
}

static inline mi_theap_t* _mi_theap_cached(void) {
  const size_t slot = _mi_theap_cached_slot;
  mi_theap_t* theap = (mi_theap_t*)mi_prim_tls_slot(slot);
  #if !MI_WIN_DIRECT_TLS
  if mi_unlikely(slot==MI_TLS_EXPANSION_SLOT) { // in TlsExpansionSlots ?
    if mi_likely(theap!=NULL) {                 // initialized (on this thread)?
      theap = ((mi_theap_t**)theap)[_mi_theap_cached_expansion_slot];
    }
  }
  #endif
  return theap;
}

#elif MI_TLS_MODEL_DYNAMIC_PTHREADS
// Dynamic pthread slot on less common platforms. This is not too bad. (default on OpenBSD)
#define MI_THEAP_INITASNULL  1

extern mi_decl_hidden pthread_key_t _mi_theap_default_key;
extern mi_decl_hidden pthread_key_t _mi_theap_cached_key;

static inline mi_theap_t* _mi_theap_default(void) {
  #if !MI_TLS_MODEL_DYNAMIC_PTHREADS_DEFAULT_ENTRY_IS_NULL
  // we can skip this check if using the initial key will return NULL from pthread_getspecific
  if mi_unlikely(_mi_theap_default_key==0) { return NULL; }
  #endif
  return (mi_theap_t*)pthread_getspecific(_mi_theap_default_key);
}

static inline mi_theap_t* _mi_theap_cached(void) {
  #if !MI_TLS_MODEL_DYNAMIC_PTHREADS_DEFAULT_ENTRY_IS_NULL
  // we can skip this check if using the initial key will return NULL from pthread_getspecific
  if mi_unlikely(_mi_theap_cached_key==0) { return NULL; }
  #endif
  return (mi_theap_t*)pthread_getspecific(_mi_theap_cached_key);
}

#else
#error "no TLS model is defined for this platform?"
#endif


// Check if a thread is initialized (without using a thread-local if using fixed slots)
static inline bool _mi_thread_is_initialized(void) {
  return (mi_theap_is_initialized(_mi_theap_default()));
}

// Get (and possible create) the theap belonging to a heap
// We cache the last accessed theap in `_mi_theap_cached` for better performance.
static inline mi_theap_t* _mi_heap_theap(const mi_heap_t* heap) {
  mi_theap_t* theap = _mi_theap_cached();  
  #if MI_THEAP_INITASNULL
  if mi_likely(theap!=NULL && _mi_theap_heap(theap)==heap) return theap;
  #else
  if mi_likely(_mi_theap_heap(theap)==heap) return theap;
  #endif
  return _mi_heap_theap_get_or_init(heap);
}

// Get the theap belonging to a heap without creating in if it is not yet initialized.
static inline mi_theap_t* _mi_heap_theap_peek(const mi_heap_t* heap) {
  mi_theap_t* theap = _mi_theap_cached();
  #if MI_THEAP_INITASNULL
  if mi_unlikely(theap==NULL || _mi_theap_heap(theap)!=heap)
  #else
  if mi_unlikely(_mi_theap_heap(theap)!=heap)
  #endif
  {
    theap = _mi_heap_theap_get_peek(heap);  // don't update the cache on a query (?)
  }
  mi_assert(theap==NULL || _mi_theap_heap(theap)==heap);
  return theap;
}

// Find the associated theap or NULL if it does not exist (during shutdown)
// Should be fast as it is called in `free.c:mi_free_try_collect`.
static inline mi_theap_t* _mi_page_associated_theap_peek(mi_page_t* page) {
  mi_heap_t* const heap = page->heap;
  mi_theap_t* theap;
  if mi_likely(heap==NULL) { theap = __mi_theap_main; }  // note: on macOS accessing the thread_local can cause allocation during thread shutdown (and reinitialize the thread)!
                      else { theap = _mi_heap_theap_peek(heap); }
  mi_assert_internal(theap==NULL || _mi_thread_id()==theap->tld->thread_id);
  return theap;
}

#endif  // MI_PRIM_H
