/* ----------------------------------------------------------------------------
Copyright (c) 2018-2021, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/

// ------------------------------------------------------------------------
// mi prefixed publi definitions of various Posix, Unix, and C++ functions
// for convenience and used when overriding these functions.
// ------------------------------------------------------------------------
#include "mimalloc.h"
#include "mimalloc/internal.h"

// ------------------------------------------------------
// Posix & Unix functions definitions
// ------------------------------------------------------

#include <errno.h>
#include <string.h>  // memset
#include <stdlib.h>  // getenv

#ifdef _MSC_VER
#pragma warning(disable:4996)  // getenv _wgetenv
#endif

#ifndef EINVAL
#define EINVAL 22
#endif
#ifndef ENOMEM
#define ENOMEM 12
#endif


mi_decl_nodiscard size_t mi_malloc_size(const void* p) mi_attr_noexcept {
  // if (!mi_is_in_heap_region(p)) return 0;
  return mi_usable_size(p);
}

mi_decl_nodiscard size_t mi_malloc_usable_size(const void *p) mi_attr_noexcept {
  // if (!mi_is_in_heap_region(p)) return 0;
  return mi_usable_size(p);
}

mi_decl_nodiscard size_t mi_malloc_good_size(size_t size) mi_attr_noexcept {
  return mi_good_size(size);
}

void mi_cfree(void* p) mi_attr_noexcept {
  if (mi_is_in_heap_region(p)) {
    mi_free(p);
  }
}

int mi_posix_memalign(void** p, size_t alignment, size_t size) mi_attr_noexcept {
  // Note: The spec dictates we should not modify `*p` on an error. (issue#27)
  // <http://man7.org/linux/man-pages/man3/posix_memalign.3.html>
  if (p == NULL) return EINVAL;
  if ((alignment % sizeof(void*)) != 0) return EINVAL;                 // natural alignment
  // it is also required that alignment is a power of 2 and > 0; this is checked in `mi_malloc_aligned`
  if (alignment==0 || !_mi_is_power_of_two(alignment)) return EINVAL;  // not a power of 2
  void* q = mi_malloc_aligned(size, alignment);
  if (q==NULL && size != 0) return ENOMEM;
  mi_assert_internal(_mi_is_aligned(q,alignment));
  *p = q;
  return 0;
}

mi_decl_nodiscard mi_decl_restrict void* mi_memalign(size_t alignment, size_t size) mi_attr_noexcept {
  void* p = mi_malloc_aligned(size, alignment);
  mi_assert_internal(_mi_is_aligned(p,alignment));
  return p;
}

mi_decl_nodiscard mi_decl_restrict void* mi_valloc(size_t size) mi_attr_noexcept {
  return mi_memalign( _mi_os_page_size(), size );
}

mi_decl_nodiscard mi_decl_restrict void* mi_pvalloc(size_t size) mi_attr_noexcept {
  size_t psize = _mi_os_page_size();
  if (size >= SIZE_MAX - psize) return NULL; // overflow
  size_t asize = _mi_align_up(size, psize);
  return mi_malloc_aligned(asize, psize);
}

mi_decl_nodiscard mi_decl_restrict void* mi_aligned_alloc(size_t alignment, size_t size) mi_attr_noexcept {
  // C11 requires the size to be an integral multiple of the alignment, see <https://en.cppreference.com/w/c/memory/aligned_alloc>.
  // unfortunately, it turns out quite some programs pass a size that is not an integral multiple so skip this check..
  /* if mi_unlikely((size & (alignment - 1)) != 0) { // C11 requires alignment>0 && integral multiple, see <https://en.cppreference.com/w/c/memory/aligned_alloc>
      #if MI_DEBUG > 0
      _mi_error_message(EOVERFLOW, "(mi_)aligned_alloc requires the size to be an integral multiple of the alignment (size %zu, alignment %zu)\n", size, alignment);
      #endif
      return NULL;
    }
  */
  // C11 also requires alignment to be a power-of-two (and > 0) which is checked in mi_malloc_aligned
  void* p = mi_malloc_aligned(size, alignment);
  mi_assert_internal(_mi_is_aligned(p,alignment));
  return p;
}

mi_decl_nodiscard void* mi_reallocarray( void* p, size_t count, size_t size ) mi_attr_noexcept {  // BSD <https://man.freebsd.org/cgi/man.cgi?query=reallocarray>
  size_t total;
  if mi_unlikely(mi_count_size_overflow(count, size, &total)) {
    errno = EOVERFLOW;
    return NULL;
  }
  void* newp = mi_realloc(p,total);
  if (newp==NULL) { errno = ENOMEM; }
  return newp;
}

mi_decl_nodiscard int mi_reallocarr( void* ptrp, size_t count, size_t size ) mi_attr_noexcept { // NetBSD <https://man.netbsd.org/reallocarr.3>
  mi_assert(size != 0);
  mi_assert(ptrp != NULL);
  if (ptrp == NULL || size == 0) {
    return (errno = EINVAL);
  }
  size_t total;
  if mi_unlikely(mi_count_size_overflow(count, size, &total)) {
    return (errno = EOVERFLOW);
  }
  void** op = (void**)ptrp;
  if (total == 0) {
    free(*op);
    *op = NULL;
    return 0;
  }
  else {
    void* newp = mi_realloc(*op,total);
    if (newp == NULL) { return (errno = ENOMEM); }
    *op = newp;
    return 0;
  }
}

void* mi__expand(void* p, size_t newsize) mi_attr_noexcept {  // Microsoft
  void* res = mi_expand(p, newsize);
  if (res == NULL) { errno = ENOMEM; }
  return res;
}

mi_decl_nodiscard mi_decl_restrict wchar_t* mi_wcsdup(const wchar_t* s) mi_attr_noexcept {
  if (s==NULL) return NULL;
  size_t wlen;
  for(wlen = 0; s[wlen] != 0 && wlen < PTRDIFF_MAX; wlen++) { }  // prevent overflow on wlen+1
  size_t size; 
  if (mi_mul_overflow(wlen+1, sizeof(wchar_t), &size) || size > PTRDIFF_MAX) return NULL;
  wchar_t* p = (wchar_t*)mi_malloc(size);
  if (p != NULL) {
    _mi_memcpy(p,s,size);
  }
  return p;
}

mi_decl_nodiscard mi_decl_restrict unsigned char* mi_mbsdup(const unsigned char* s)  mi_attr_noexcept {
  return (unsigned char*)mi_strdup((const char*)s);
}

int mi_dupenv_s(char** buf, size_t* size, const char* name) mi_attr_noexcept {
  if (size != NULL) *size = 0;
  if (buf==NULL || name==NULL) return EINVAL;
  char* p = getenv(name);
  if (p==NULL) {
    *buf = NULL;
  }
  else {
    *buf = mi_strdup(p);
    if (*buf==NULL) return ENOMEM;
    if (size != NULL) { *size = _mi_strlen(p) + 1; }    // cannot overflow as mi_strdup is limited to PTRDIFF_MAX
  }
  return 0;
}

int mi_wdupenv_s(wchar_t** buf, size_t* size, const wchar_t* name) mi_attr_noexcept {
  if (size != NULL) *size = 0;
  if (buf==NULL || name==NULL) return EINVAL;  
#if !defined(_WIN32) || (defined(WINAPI_FAMILY) && (WINAPI_FAMILY != WINAPI_FAMILY_DESKTOP_APP))
  // not supported
  *buf = NULL;
  return EINVAL;
#else
  wchar_t* p = (wchar_t*)_wgetenv(name);
  if (p==NULL) {
    *buf = NULL;
  }
  else {
    *buf = mi_wcsdup(p);
    if (*buf==NULL) return ENOMEM;
    if (size != NULL) { *size = wcslen(p) + 1; }  // cannot overflow as wcsdup is limited to PTRDIFF_MAX
  }
  return 0;
#endif
}

mi_decl_nodiscard void* mi_aligned_offset_recalloc(void* p, size_t newcount, size_t size, size_t alignment, size_t offset) mi_attr_noexcept { // Microsoft
  return mi_recalloc_aligned_at(p, newcount, size, alignment, offset);
}

mi_decl_nodiscard void* mi_aligned_recalloc(void* p, size_t newcount, size_t size, size_t alignment) mi_attr_noexcept { // Microsoft
  return mi_recalloc_aligned(p, newcount, size, alignment);
}
