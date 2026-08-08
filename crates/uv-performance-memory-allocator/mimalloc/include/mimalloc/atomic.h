/* ----------------------------------------------------------------------------
Copyright (c) 2018-2024 Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/
#pragma once
#ifndef MI_ATOMIC_H
#define MI_ATOMIC_H

// include windows.h or pthreads.h
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#elif !defined(__wasi__) && (!defined(__EMSCRIPTEN__) || defined(__EMSCRIPTEN_PTHREADS__))
#define  MI_USE_PTHREADS
#include <pthread.h>
#endif

// --------------------------------------------------------------------------------------------
// Atomics
// We need to be portable between C, C++, and MSVC.
// We base the primitives on the C/C++ atomics and create a minimal wrapper for MSVC in C compilation mode.
// This is why we try to use only `uintptr_t` and `<type>*` as atomic types.
// To gain better insight in the range of used atomics, we use explicitly named memory order operations
// instead of passing the memory order as a parameter.
// -----------------------------------------------------------------------------------------------

#if defined(__cplusplus)
// Use C++ atomics
#include <atomic>
#define  _Atomic(tp)              std::atomic<tp>
#define  mi_atomic(name)          std::atomic_##name
#define  mi_memory_order(name)    std::memory_order_##name
#if (__cplusplus >= 202002L)      // c++20, see issue #571
 #define MI_ATOMIC_VAR_INIT(x)    x
#elif !defined(ATOMIC_VAR_INIT)
 #define MI_ATOMIC_VAR_INIT(x)    x
#else
 #define MI_ATOMIC_VAR_INIT(x)    ATOMIC_VAR_INIT(x)
#endif
#elif defined(_MSC_VER)
// Use MSVC C wrapper for C11 atomics
#define  _Atomic(tp)              tp
#define  MI_ATOMIC_VAR_INIT(x)    x
#define  mi_atomic(name)          mi_atomic_##name
#define  mi_memory_order(name)    mi_memory_order_##name
#else
// Use C11 atomics
#include <stdatomic.h>
#define  mi_atomic(name)          atomic_##name
#define  mi_memory_order(name)    memory_order_##name
#if (__STDC_VERSION__ >= 201710L) // c17, see issue #735
 #define MI_ATOMIC_VAR_INIT(x)    x
#elif !defined(ATOMIC_VAR_INIT)
 #define MI_ATOMIC_VAR_INIT(x)    x
#else
 #define MI_ATOMIC_VAR_INIT(x)    ATOMIC_VAR_INIT(x)
#endif
#endif

// Various defines for all used memory orders in mimalloc
#define mi_atomic_cas_weak(p,expected,desired,mem_success,mem_fail)  \
  mi_atomic(compare_exchange_weak_explicit)(p,expected,desired,mem_success,mem_fail)

#define mi_atomic_cas_strong(p,expected,desired,mem_success,mem_fail)  \
  mi_atomic(compare_exchange_strong_explicit)(p,expected,desired,mem_success,mem_fail)

#define mi_atomic_load_acquire(p)                mi_atomic(load_explicit)(p,mi_memory_order(acquire))
#define mi_atomic_load_relaxed(p)                mi_atomic(load_explicit)(p,mi_memory_order(relaxed))
#define mi_atomic_store_release(p,x)             mi_atomic(store_explicit)(p,x,mi_memory_order(release))
#define mi_atomic_store_relaxed(p,x)             mi_atomic(store_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_exchange_relaxed(p,x)          mi_atomic(exchange_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_exchange_release(p,x)          mi_atomic(exchange_explicit)(p,x,mi_memory_order(release))
#define mi_atomic_exchange_acq_rel(p,x)          mi_atomic(exchange_explicit)(p,x,mi_memory_order(acq_rel))

#define mi_atomic_cas_weak_relaxed(p,exp,des)    mi_atomic_cas_weak(p,exp,des,mi_memory_order(relaxed),mi_memory_order(relaxed))
#define mi_atomic_cas_weak_release(p,exp,des)    mi_atomic_cas_weak(p,exp,des,mi_memory_order(release),mi_memory_order(relaxed))
#define mi_atomic_cas_weak_acq_rel(p,exp,des)    mi_atomic_cas_weak(p,exp,des,mi_memory_order(acq_rel),mi_memory_order(acquire))
#define mi_atomic_cas_strong_relaxed(p,exp,des)  mi_atomic_cas_strong(p,exp,des,mi_memory_order(relaxed),mi_memory_order(relaxed))
#define mi_atomic_cas_strong_release(p,exp,des)  mi_atomic_cas_strong(p,exp,des,mi_memory_order(release),mi_memory_order(relaxed))
#define mi_atomic_cas_strong_acq_rel(p,exp,des)  mi_atomic_cas_strong(p,exp,des,mi_memory_order(acq_rel),mi_memory_order(acquire))

#define mi_atomic_add_relaxed(p,x)               mi_atomic(fetch_add_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_add_acq_rel(p,x)               mi_atomic(fetch_add_explicit)(p,x,mi_memory_order(acq_rel))
#define mi_atomic_sub_relaxed(p,x)               mi_atomic(fetch_sub_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_sub_acq_rel(p,x)               mi_atomic(fetch_sub_explicit)(p,x,mi_memory_order(acq_rel))
#define mi_atomic_and_relaxed(p,x)               mi_atomic(fetch_and_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_and_acq_rel(p,x)               mi_atomic(fetch_and_explicit)(p,x,mi_memory_order(acq_rel))
#define mi_atomic_or_relaxed(p,x)                mi_atomic(fetch_or_explicit)(p,x,mi_memory_order(relaxed))
#define mi_atomic_or_acq_rel(p,x)                mi_atomic(fetch_or_explicit)(p,x,mi_memory_order(acq_rel))

#define mi_atomic_increment_relaxed(p)           mi_atomic_add_relaxed(p,(uintptr_t)1)
#define mi_atomic_decrement_relaxed(p)           mi_atomic_sub_relaxed(p,(uintptr_t)1)
#define mi_atomic_increment_acq_rel(p)           mi_atomic_add_acq_rel(p,(uintptr_t)1)
#define mi_atomic_decrement_acq_rel(p)           mi_atomic_sub_acq_rel(p,(uintptr_t)1)

static inline intptr_t mi_atomic_addi(_Atomic(intptr_t)*p, intptr_t add);
static inline intptr_t mi_atomic_subi(_Atomic(intptr_t)*p, intptr_t sub);


#if defined(__cplusplus) || !defined(_MSC_VER)

// In C++/C11 atomics we have polymorphic atomics so can use the typed `ptr` variants (where `tp` is the type of atomic value)
// We use these macros so we can provide a typed wrapper in MSVC in C compilation mode as well
#define mi_atomic_load_ptr_acquire(tp,p)                mi_atomic_load_acquire(p)
#define mi_atomic_load_ptr_relaxed(tp,p)                mi_atomic_load_relaxed(p)

// In C++ we need to add casts to help resolve templates if NULL is passed
#if defined(__cplusplus)
#define mi_atomic_store_ptr_release(tp,p,x)             mi_atomic_store_release(p,(tp*)x)
#define mi_atomic_store_ptr_relaxed(tp,p,x)             mi_atomic_store_relaxed(p,(tp*)x)
#define mi_atomic_cas_ptr_weak_release(tp,p,exp,des)    mi_atomic_cas_weak_release(p,exp,(tp*)des)
#define mi_atomic_cas_ptr_weak_acq_rel(tp,p,exp,des)    mi_atomic_cas_weak_acq_rel(p,exp,(tp*)des)
#define mi_atomic_cas_ptr_strong_release(tp,p,exp,des)  mi_atomic_cas_strong_release(p,exp,(tp*)des)
#define mi_atomic_cas_ptr_strong_acq_rel(tp,p,exp,des)  mi_atomic_cas_strong_acq_rel(p,exp,(tp*)des)
#define mi_atomic_exchange_ptr_relaxed(tp,p,x)          mi_atomic_exchange_relaxed(p,(tp*)x)
#define mi_atomic_exchange_ptr_release(tp,p,x)          mi_atomic_exchange_release(p,(tp*)x)
#define mi_atomic_exchange_ptr_acq_rel(tp,p,x)          mi_atomic_exchange_acq_rel(p,(tp*)x)
#else
#define mi_atomic_store_ptr_release(tp,p,x)             mi_atomic_store_release(p,x)
#define mi_atomic_store_ptr_relaxed(tp,p,x)             mi_atomic_store_relaxed(p,x)
#define mi_atomic_cas_ptr_weak_release(tp,p,exp,des)    mi_atomic_cas_weak_release(p,exp,des)
#define mi_atomic_cas_ptr_weak_acq_rel(tp,p,exp,des)    mi_atomic_cas_weak_acq_rel(p,exp,des)
#define mi_atomic_cas_ptr_strong_release(tp,p,exp,des)  mi_atomic_cas_strong_release(p,exp,des)
#define mi_atomic_cas_ptr_strong_acq_rel(tp,p,exp,des)  mi_atomic_cas_strong_acq_rel(p,exp,des)
#define mi_atomic_exchange_ptr_relaxed(tp,p,x)          mi_atomic_exchange_relaxed(p,x)
#define mi_atomic_exchange_ptr_release(tp,p,x)          mi_atomic_exchange_release(p,x)
#define mi_atomic_exchange_ptr_acq_rel(tp,p,x)          mi_atomic_exchange_acq_rel(p,x)
#endif

// These are used by the statistics
static inline int64_t mi_atomic_addi64_relaxed(volatile int64_t* p, int64_t add) {
  return mi_atomic(fetch_add_explicit)((_Atomic(int64_t)*)p, add, mi_memory_order(relaxed));
}
static inline void mi_atomic_void_addi64_relaxed(volatile int64_t* p, const volatile int64_t* padd) {
  const int64_t add = mi_atomic_load_relaxed((_Atomic(int64_t)*)padd);
  if (add != 0) {
    mi_atomic(fetch_add_explicit)((_Atomic(int64_t)*)p, add, mi_memory_order(relaxed));
  }
}
static inline void mi_atomic_maxi64_relaxed(volatile int64_t* p, int64_t x) {
  int64_t current = mi_atomic_load_relaxed((_Atomic(int64_t)*)p);
  while (current < x && !mi_atomic_cas_weak_release((_Atomic(int64_t)*)p, &current, x)) { /* nothing */ };
}

// Used by timers
#define mi_atomic_loadi64_acquire(p)            mi_atomic(load_explicit)(p,mi_memory_order(acquire))
#define mi_atomic_loadi64_relaxed(p)            mi_atomic(load_explicit)(p,mi_memory_order(relaxed))
#define mi_atomic_storei64_release(p,x)         mi_atomic(store_explicit)(p,x,mi_memory_order(release))
#define mi_atomic_storei64_relaxed(p,x)         mi_atomic(store_explicit)(p,x,mi_memory_order(relaxed))

#define mi_atomic_casi64_strong_acq_rel(p,e,d)  mi_atomic_cas_strong_acq_rel(p,e,d)
#define mi_atomic_addi64_acq_rel(p,i)           mi_atomic_add_acq_rel(p,i)


#elif defined(_MSC_VER)

// Deprecated: MSVC plain C compilation wrapper that uses Interlocked operations to model C11 atomics.
// It is recommended to always compile as C++ when using MSVC.

#include <intrin.h>
#ifdef _WIN64
typedef LONG64        msc_intptr_t;
#define MI_MSC_64(f)  f##64
#define MI_MSC_XX(f)  f##64
#else
typedef LONG          msc_intptr_t;
#define MI_MSC_64(f)  f
#define MI_MSC_XX(f)  f##32
#endif

typedef enum mi_memory_order_e {
  mi_memory_order_relaxed,
  mi_memory_order_consume,
  mi_memory_order_acquire,
  mi_memory_order_release,
  mi_memory_order_acq_rel,
  mi_memory_order_seq_cst
} mi_memory_order;

static inline uintptr_t mi_atomic_fetch_add_explicit(_Atomic(uintptr_t)*p, uintptr_t add, mi_memory_order mo) {
  (void)(mo);
  return (uintptr_t)MI_MSC_64(_InterlockedExchangeAdd)((volatile msc_intptr_t*)p, (msc_intptr_t)add);
}
static inline uintptr_t mi_atomic_fetch_sub_explicit(_Atomic(uintptr_t)*p, uintptr_t sub, mi_memory_order mo) {
  (void)(mo);
  return (uintptr_t)MI_MSC_64(_InterlockedExchangeAdd)((volatile msc_intptr_t*)p, -((msc_intptr_t)sub));
}
static inline uintptr_t mi_atomic_fetch_and_explicit(_Atomic(uintptr_t)*p, uintptr_t x, mi_memory_order mo) {
  (void)(mo);
  return (uintptr_t)MI_MSC_64(_InterlockedAnd)((volatile msc_intptr_t*)p, (msc_intptr_t)x);
}
static inline uintptr_t mi_atomic_fetch_or_explicit(_Atomic(uintptr_t)*p, uintptr_t x, mi_memory_order mo) {
  (void)(mo);
  return (uintptr_t)MI_MSC_64(_InterlockedOr)((volatile msc_intptr_t*)p, (msc_intptr_t)x);
}
static inline bool mi_atomic_compare_exchange_strong_explicit(_Atomic(uintptr_t)*p, uintptr_t* expected, uintptr_t desired, mi_memory_order mo1, mi_memory_order mo2) {
  (void)(mo1); (void)(mo2);
  const uintptr_t read = (uintptr_t)MI_MSC_64(_InterlockedCompareExchange)((volatile msc_intptr_t*)p, (msc_intptr_t)desired, (msc_intptr_t)(*expected));
  if (read == *expected) {
    return true;
  }
  else {
    *expected = read;
    return false;
  }
}
static inline bool mi_atomic_compare_exchange_weak_explicit(_Atomic(uintptr_t)*p, uintptr_t* expected, uintptr_t desired, mi_memory_order mo1, mi_memory_order mo2) {
  return mi_atomic_compare_exchange_strong_explicit(p, expected, desired, mo1, mo2);
}
static inline uintptr_t mi_atomic_exchange_explicit(_Atomic(uintptr_t)*p, uintptr_t exchange, mi_memory_order mo) {
  (void)(mo);
  return (uintptr_t)MI_MSC_64(_InterlockedExchange)((volatile msc_intptr_t*)p, (msc_intptr_t)exchange);
}
static inline void mi_atomic_thread_fence(mi_memory_order mo) {
  (void)(mo);
  _Atomic(uintptr_t) x = 0;
  mi_atomic_exchange_explicit(&x, 1, mo);
}

static inline uintptr_t mi_atomic_load_explicit(_Atomic(uintptr_t) const* p, mi_memory_order mo) {
  (void)(mo);
  // assert(mo<=mi_memory_order_acquire); // others are not used by mimalloc
  #if defined(_M_IX86) || defined(_M_X64)
    return (uintptr_t)MI_MSC_XX(__iso_volatile_load)((volatile const intptr_t*)p);
  #elif defined(_M_ARM) || defined(_M_ARM64)
    if (mo == mi_memory_order_relaxed) {
      return (uintptr_t)MI_MSC_XX(__iso_volatile_load)((volatile const intptr_t*)p);
    }
    else if (mo <= mi_memory_order_acquire) {
      return MI_MSC_XX(__ldar)((volatile const uintptr_t*)p);
    }
    else {
      const uintptr_t u = (uintptr_t)MI_MSC_XX(__iso_volatile_load)((volatile const intptr_t*)p);
      __dmb(15);  // _ARM(64)_BARRIER_SY
      return u;
    }
  #else
    #warning "define mi_atomic_load_explicit for MSVC C compilation on this platform (which should be readonly, see issue #1277)"
    return MI_MSC_XX(__iso_volatile_load)((volatile const intptr_t*)p);
  #endif
}
static inline void mi_atomic_store_explicit(_Atomic(uintptr_t)*p, uintptr_t x, mi_memory_order mo) {
  (void)(mo);
  // assert(mo<=mi_memory_order_release); // others are not used by mimalloc
  #if defined(_M_IX86) || defined(_M_X64)
    MI_MSC_XX(__iso_volatile_store)((volatile intptr_t*)p, x);
  #elif defined(_M_ARM) || defined(_M_ARM64)
    if (mo == mi_memory_order_relaxed) {
      MI_MSC_XX(__iso_volatile_store)((volatile intptr_t*)p, x);
    }
    else if (mo <= mi_memory_order_release) {
      MI_MSC_XX(__stlr)((volatile uintptr_t*)p,x);
    }
    else {
      mi_atomic_exchange_explicit(p, x, mo);
    }
  #else
    mi_atomic_exchange_explicit(p, x, mo);
  #endif
}

static inline int64_t mi_atomic_loadi64_explicit(_Atomic(int64_t)*p, mi_memory_order mo) {
  (void)(mo);
  // assert(mo<=mi_memory_order_acquire); // others are not used by mimalloc
  #if defined(_M_IX86) || defined(_M_X64)
    return __iso_volatile_load64((volatile const int64_t*)p);
  #elif defined(_M_ARM) || defined(_M_ARM64)
    if (mo == mi_memory_order_relaxed) {
      return __iso_volatile_load64((volatile const int64_t*)p);
    }
    #if defined(_M_ARM64)
    else if (mo <= mi_memory_order_acquire) {
      return __ldar64((volatile const uintptr_t*)p);
    }
    #endif
    else {
      const int64_t i = __iso_volatile_load64((volatile const int64_t*)p);
      __dmb(15);  // _ARM(64)_BARRIER_SY
      return i;
    }
  #else
    #warning "define mi_atomic_loadi64_explicit for MSVC C compilation on this platform (which should be readonly, see issue #1277)"
    return __iso_volatile_load64((volatile const int64_t*)p);
  #endif
}

static inline void mi_atomic_storei64_explicit(_Atomic(int64_t)*p, int64_t x, mi_memory_order mo) {
  (void)(mo);
  // assert(mo<=mi_memory_order_release); // others are not used by mimalloc
  #if defined(_M_IX86) || defined(_M_X64)
    __iso_volatile_store64((volatile int64_t*)p,x);
  #elif defined(_M_ARM) || defined(_M_ARM64)
    if (mo == mi_memory_order_relaxed) {
      __iso_volatile_store64((volatile int64_t*)p,x);
    }
    #if defined(_M_ARM64)
    else if (mo == mi_memory_order_release) {
      __stlr64((volatile uint64_t*)p, (uint64_t)x);
    }
    #endif
    else {
      InterlockedExchange64(p, x);
    }
  #else
    InterlockedExchange64(p, x);
  #endif
}

// These are used by the statistics
static inline int64_t mi_atomic_addi64_relaxed(volatile _Atomic(int64_t)*p, int64_t add) {
  #ifdef _WIN64
    return (int64_t)mi_atomic_addi((int64_t*)p, add);
  #elif defined(_M_ARM)
    return _InterlockedExchangeAdd64(p, add);
  #else
    // x86
    int64_t current;
    int64_t sum;
    do {
      current = __iso_volatile_load64((volatile const int64_t*)p);
      sum = current + add;
    } while (_InterlockedCompareExchange64(p, sum, current) != current);
    return current;
  #endif
}

static inline void mi_atomic_void_addi64_relaxed(volatile int64_t* p, const volatile int64_t* padd) {
  const int64_t add = *padd;
  if (add != 0) {
    mi_atomic_addi64_relaxed((volatile _Atomic(int64_t)*)p, add);
  }
}

static inline void mi_atomic_maxi64_relaxed(volatile _Atomic(int64_t)*p, int64_t x) {
  int64_t current;
  do {
    current = *p;
  } while (current < x && _InterlockedCompareExchange64(p, x, current) != current);
}

static inline void mi_atomic_addi64_acq_rel(volatile _Atomic(int64_t*)p, int64_t i) {
  mi_atomic_addi64_relaxed(p, i);
}

static inline bool mi_atomic_casi64_strong_acq_rel(volatile _Atomic(int64_t*)p, int64_t* exp, int64_t des) {
  const int64_t read = _InterlockedCompareExchange64(p, des, *exp);
  if (read == *exp) {
    return true;
  }
  else {
    *exp = read;
    return false;
  }
}

// The pointer macros cast to `uintptr_t`.
#define mi_atomic_load_ptr_acquire(tp,p)                (tp*)mi_atomic_load_acquire((_Atomic(uintptr_t)*)(p))
#define mi_atomic_load_ptr_relaxed(tp,p)                (tp*)mi_atomic_load_relaxed((_Atomic(uintptr_t)*)(p))
#define mi_atomic_store_ptr_release(tp,p,x)             mi_atomic_store_release((_Atomic(uintptr_t)*)(p),(uintptr_t)(x))
#define mi_atomic_store_ptr_relaxed(tp,p,x)             mi_atomic_store_relaxed((_Atomic(uintptr_t)*)(p),(uintptr_t)(x))
#define mi_atomic_cas_ptr_weak_release(tp,p,exp,des)    mi_atomic_cas_weak_release((_Atomic(uintptr_t)*)(p),(uintptr_t*)exp,(uintptr_t)des)
#define mi_atomic_cas_ptr_weak_acq_rel(tp,p,exp,des)    mi_atomic_cas_weak_acq_rel((_Atomic(uintptr_t)*)(p),(uintptr_t*)exp,(uintptr_t)des)
#define mi_atomic_cas_ptr_strong_release(tp,p,exp,des)  mi_atomic_cas_strong_release((_Atomic(uintptr_t)*)(p),(uintptr_t*)exp,(uintptr_t)des)
#define mi_atomic_cas_ptr_strong_acq_rel(tp,p,exp,des)  mi_atomic_cas_strong_acq_rel((_Atomic(uintptr_t)*)(p),(uintptr_t*)exp,(uintptr_t)des)
#define mi_atomic_exchange_ptr_relaxed(tp,p,x)          (tp*)mi_atomic_exchange_relaxed((_Atomic(uintptr_t)*)(p),(uintptr_t)x)
#define mi_atomic_exchange_ptr_release(tp,p,x)          (tp*)mi_atomic_exchange_release((_Atomic(uintptr_t)*)(p),(uintptr_t)x)
#define mi_atomic_exchange_ptr_acq_rel(tp,p,x)          (tp*)mi_atomic_exchange_acq_rel((_Atomic(uintptr_t)*)(p),(uintptr_t)x)

#define mi_atomic_loadi64_acquire(p)    mi_atomic(loadi64_explicit)(p,mi_memory_order(acquire))
#define mi_atomic_loadi64_relaxed(p)    mi_atomic(loadi64_explicit)(p,mi_memory_order(relaxed))
#define mi_atomic_storei64_release(p,x) mi_atomic(storei64_explicit)(p,x,mi_memory_order(release))
#define mi_atomic_storei64_relaxed(p,x) mi_atomic(storei64_explicit)(p,x,mi_memory_order(relaxed))


#endif


// Atomically add a signed value; returns the previous value.
static inline intptr_t mi_atomic_addi(_Atomic(intptr_t)*p, intptr_t add) {
  return (intptr_t)mi_atomic_add_acq_rel((_Atomic(uintptr_t)*)p, (uintptr_t)add);
}

// Atomically subtract a signed value; returns the previous value.
static inline intptr_t mi_atomic_subi(_Atomic(intptr_t)*p, intptr_t sub) {
  return (intptr_t)mi_atomic_addi(p, -sub);
}


// ----------------------------------------------------------------------
// Guard
// ----------------------------------------------------------------------

typedef _Atomic(uintptr_t) mi_atomic_guard_t;

// Allows only one thread to execute at a time (without blocking anyone)
#define mi_atomic_guard(guard) \
  uintptr_t _mi_guard_expected = 0; \
  for(bool _mi_guard_once = true; \
      _mi_guard_once && mi_atomic_cas_strong_acq_rel(guard,&_mi_guard_expected,(uintptr_t)1); \
      (mi_atomic_store_release(guard,(uintptr_t)0), _mi_guard_once = false) )


// ----------------------------------------------------------------------
// Locks
// These should be light-weight in-process only locks.
// Only used for reserving arena's and to maintain the abandoned list.
// ----------------------------------------------------------------------
#if _MSC_VER
#pragma warning(disable:26110)  // unlock with holding lock
#endif

#define mi_lock(lock)                  for(bool _mi_go = (mi_lock_acquire(lock),true); _mi_go; (mi_lock_release(lock), _mi_go=false) )
#define mi_lock_maybe(lock,acquire)    for(bool _mi_go = (acquire ? (mi_lock_acquire(lock),true) : true); _mi_go; _mi_go = (acquire ? (mi_lock_release(lock),false) : false) )


#if defined(_WIN32)

typedef struct mi_lock_s {
  SRWLOCK mutex;    // slim reader-writer lock
} mi_lock_t;

#define MI_LOCK_INITIALIZER   { SRWLOCK_INIT }

static inline bool mi_lock_try_acquire(mi_lock_t* lock) {
  return TryAcquireSRWLockExclusive(&lock->mutex);
}
static inline void mi_lock_acquire(mi_lock_t* lock) {
  AcquireSRWLockExclusive(&lock->mutex);
}
static inline void mi_lock_release(mi_lock_t* lock) {
  ReleaseSRWLockExclusive(&lock->mutex);
}
static inline void mi_lock_init(mi_lock_t* lock) {
  InitializeSRWLock(&lock->mutex);
}
static inline void mi_lock_done(mi_lock_t* lock) {
  (void)(lock);
}

#elif defined(MI_USE_PTHREADS)

#include <string.h> // memcpy
void _mi_error_message(int err, const char* fmt, ...);

typedef struct mi_lock_s {
  pthread_mutex_t mutex;
} mi_lock_t;

#define MI_LOCK_INITIALIZER { PTHREAD_MUTEX_INITIALIZER }

static inline bool mi_lock_try_acquire(mi_lock_t* lock) {
  return (pthread_mutex_trylock(&lock->mutex) == 0);
}
static inline void mi_lock_acquire(mi_lock_t* lock) {
  const int err = pthread_mutex_lock(&lock->mutex);
  if (err != 0) {
    _mi_error_message(err, "internal error: lock cannot be acquired (err %i)\n", err);
  }
}
static inline void mi_lock_release(mi_lock_t* lock) {
  pthread_mutex_unlock(&lock->mutex);
}
static inline void mi_lock_init(mi_lock_t* lock) {
  if(lock==NULL) return;
  // use this instead of pthread_mutex_init since that can cause allocation on some platforms (and recursively initialize)
  const pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
  memcpy(&lock->mutex,&mutex,sizeof(mutex));
}
static inline void mi_lock_done(mi_lock_t* lock) {
  pthread_mutex_destroy(&lock->mutex);
}

#elif defined(__cplusplus)

#include <thread>
#include <mutex>
#include <new>

typedef struct mi_lock_s {
  std::mutex mutex;
} mi_lock_t;

#define MI_LOCK_INITIALIZER   { }

static inline bool mi_lock_try_acquire(mi_lock_t* lock) {
  return lock->mutex.try_lock();
}
static inline void mi_lock_acquire(mi_lock_t* lock) {
  lock->mutex.lock();
}
static inline void mi_lock_release(mi_lock_t* lock) {
  lock->mutex.unlock();
}
static inline void mi_lock_init(mi_lock_t* lock) {
  new(&lock->mutex) std::mutex();
}
static inline void mi_lock_done(mi_lock_t* lock) {
  (void)(lock);
}

#else

// fall back to poor man's locks.
// this should only be the case in a single-threaded environment (like __wasi__)
#include <errno.h>
#ifndef EFAULT
#define EFAULT (14)
#endif
void _mi_error_message(int err, const char* fmt, ...);
void _mi_prim_thread_yield(void);

typedef struct mi_lock_s {
  _Atomic(uintptr_t) mutex;
} mi_lock_t;

#define MI_LOCK_INITIALIZER  { MI_ATOMIC_VAR_INIT(0) }

static inline bool mi_lock_try_acquire(mi_lock_t* lock) {
  uintptr_t expected = 0;
  return mi_atomic_cas_strong_acq_rel(&lock->mutex, &expected, (uintptr_t)1);
}
static inline void mi_lock_acquire(mi_lock_t* lock) {
  for (int i = 0; i < 10000; i++) {  // for at most 10000 tries?
    if (mi_lock_try_acquire(lock)) return;
    _mi_prim_thread_yield();
  }
  _mi_error_message(EFAULT, "internal error: lock cannot be acquired (due to lack of native lock primitives)\n");
}
static inline void mi_lock_release(mi_lock_t* lock) {
  mi_atomic_store_release(&lock->mutex, (uintptr_t)0);
}
static inline void mi_lock_init(mi_lock_t* lock) {
  mi_lock_release(lock);
}
static inline void mi_lock_done(mi_lock_t* lock) {
  (void)(lock);
}

#endif


typedef struct mi_atomic_once_s {
  _Atomic(uintptr_t) tid;
  mi_lock_t          lock;
} mi_atomic_once_t;

// Returns `true` only on the first invocation, signifying we can execute an action once.
// If it returns `true`, the caller should call `_mi_atomic_once_release` after performing the action.
// Other threads (than the initial thread that entered) will block until `_mi_atomic_once_release` has been called.
bool _mi_atomic_once_enter(mi_atomic_once_t* once);        // defined in `libc.c`
void _mi_atomic_once_release(mi_atomic_once_t* once);      // defined in `libc.c`

#define mi_atomic_do_once  \
  static mi_atomic_once_t _mi_once = { MI_ATOMIC_VAR_INIT(0), MI_LOCK_INITIALIZER }; \
  for(bool _mi_exec = _mi_atomic_once_enter(&_mi_once); _mi_exec; (_mi_atomic_once_release(&_mi_once),_mi_exec=false))


#endif // __MIMALLOC_ATOMIC_H
