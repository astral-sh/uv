
/* ----------------------------------------------------------------------------
Copyright (c) 2018-2025, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/
#ifndef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE   // for realpath() on Linux
#endif

#include "mimalloc.h"
#include "mimalloc/internal.h"
#include "mimalloc/atomic.h"
#include "mimalloc/prim.h"   // _mi_prim_thread_id()

#include <string.h>      // memset, strlen (for mi_strdup)
#include <stdlib.h>      // malloc, abort

#define MI_IN_ALLOC_C
#include "alloc-override.c"
#include "free.c"
#undef MI_IN_ALLOC_C

// ------------------------------------------------------
// Allocation
// ------------------------------------------------------

// Fast allocation in a page: just pop from the free list.
// Fall back to generic allocation only if the list is empty.
// Note: in release mode the (inlined) routine is about 7 instructions with a single test.
static mi_decl_forceinline void* mi_page_malloc_zero(mi_theap_t* theap, mi_page_t* page, size_t size, bool zero, size_t* usable) mi_attr_noexcept
{
  if (page->block_size != 0) { // not the empty theap
    mi_assert_internal(mi_page_block_size(page) >= size);
    mi_assert_internal(_mi_is_aligned(mi_page_slice_start(page), MI_PAGE_ALIGN));
    mi_assert_internal(_mi_ptr_page(mi_page_start(page))==page);
  }

  // check the free list
  mi_block_t* const block = page->free;
  if mi_unlikely(block == NULL) {
    return _mi_malloc_generic(theap, size, (zero ? 1 : 0), usable);
  }
  mi_assert_internal(block != NULL && _mi_ptr_page(block) == page);
  if (usable != NULL) { *usable = mi_page_usable_block_size(page); };

  // pop from the free list
  page->free = mi_block_next(page, block);
  page->used++;
  mi_assert_internal(page->free == NULL || _mi_ptr_page(page->free) == page);
  mi_assert_internal(page->block_size < MI_MAX_ALIGN_SIZE || _mi_is_aligned(block, MI_MAX_ALIGN_SIZE));

  #if MI_DEBUG>3
  if (page->free_is_zero && size > sizeof(*block)) {
    mi_assert_expensive(mi_mem_is_zero(block+1,size - sizeof(*block)));
  }
  #endif

  // allow use of the block internally
  // note: when tracking we need to avoid ever touching the MI_PADDING since
  // that is tracked by valgrind etc. as non-accessible (through the red-zone, see `mimalloc/track.h`)
  const size_t bsize = mi_page_usable_block_size(page);
  mi_track_mem_undefined(block, bsize);

  #if (MI_STAT>0)
  if (bsize <= MI_LARGE_MAX_OBJ_SIZE) {
    mi_theap_stat_increase(theap, malloc_normal, bsize);
    #if (MI_STAT>1)
    mi_theap_stat_counter_increase(theap, malloc_normal_count, 1);
    const size_t bin = _mi_bin(bsize);
    mi_theap_stat_increase(theap, malloc_bins[bin], 1);
    mi_theap_stat_increase(theap, malloc_requested, size - MI_PADDING_SIZE);
    #endif
  }
  #endif

  // zero the block? note: we need to zero the full block size (issue #63)
  if mi_likely(!zero) {
    // #if MI_SECURE
    block->next = 0;  // don't leak internal data
    // #endif
    #if (MI_DEBUG>0) && !MI_TRACK_ENABLED && !MI_TSAN
      if (!mi_page_is_huge(page)) { memset(block, MI_DEBUG_UNINIT, bsize); }
    #endif    
  }
  else {
    if (!page->free_is_zero) {
      _mi_memzero_aligned(block, bsize);
    }
    else {
      block->next = 0;
      mi_track_mem_defined(block, bsize);
    }    
  }

  #if MI_PADDING // && !MI_TRACK_ENABLED
    mi_padding_t* const padding = (mi_padding_t*)((uint8_t*)block + bsize);
    ptrdiff_t delta = ((uint8_t*)padding - (uint8_t*)block - (size - MI_PADDING_SIZE));
    #if (MI_DEBUG>=2)
    mi_assert_internal(delta >= 0 && bsize >= (size - MI_PADDING_SIZE + delta));
    #endif
    mi_track_mem_defined(padding,sizeof(mi_padding_t));  // note: re-enable since mi_page_usable_block_size may set noaccess
    padding->canary = mi_ptr_encode_canary(page,block,page->keys);
    padding->delta  = (uint32_t)(delta);
    #if MI_PADDING_CHECK
    if (!mi_page_is_huge(page)) {
      uint8_t* fill = (uint8_t*)padding - delta;
      const size_t maxpad = (delta > MI_MAX_ALIGN_SIZE ? MI_MAX_ALIGN_SIZE : delta); // set at most N initial padding bytes
      for (size_t i = 0; i < maxpad; i++) { fill[i] = MI_DEBUG_PADDING; }
    }
    #endif
  #endif

  return block;
}

// extra entries for improved efficiency in `alloc-aligned.c` (and in `page.c:mi_malloc_generic`.
extern void* _mi_page_malloc_zero(mi_theap_t* theap, mi_page_t* page, size_t size, bool zero) mi_attr_noexcept {
  return mi_page_malloc_zero(theap, page, size, zero, NULL);
}

#if MI_GUARDED
mi_decl_restrict void* _mi_theap_malloc_guarded(mi_theap_t* theap, size_t size, bool zero) mi_attr_noexcept;
#endif

// main allocation primitives for small and generic allocation

// internal small size allocation
static mi_decl_forceinline mi_decl_restrict void* mi_theap_malloc_small_zero_nonnull(mi_theap_t* theap, size_t size, bool zero, size_t* usable) mi_attr_noexcept
{
  mi_assert(theap != NULL);
  mi_assert(size <= MI_SMALL_SIZE_MAX);
  #if MI_DEBUG
  const uintptr_t tid = _mi_thread_id();
  mi_assert(theap->tld->thread_id == 0 || theap->tld->thread_id == tid); // theaps are thread local
  #endif
  #if (MI_PADDING || MI_GUARDED)
  if mi_unlikely(size == 0) { size = sizeof(void*); }
  #endif
  #if MI_GUARDED
  if mi_unlikely(mi_theap_malloc_use_guarded(theap,size)) {
    return _mi_theap_malloc_guarded(theap, size, zero);
  }
  #endif

  // get page in constant time, and allocate from it
  mi_page_t* page = _mi_theap_get_free_small_page(theap, size + MI_PADDING_SIZE);
  void* const p = mi_page_malloc_zero(theap, page, size + MI_PADDING_SIZE, zero, usable);
  mi_track_malloc(p,size,zero);

  #if MI_DEBUG>3
  if (p != NULL && zero) {
    mi_assert_expensive(mi_mem_is_zero(p, size));
  }
  #endif
  return p;
}

// internal generic allocation
static mi_decl_forceinline void* mi_theap_malloc_generic(mi_theap_t* theap, size_t size, bool zero, size_t huge_alignment, size_t* usable) mi_attr_noexcept
{
  #if MI_GUARDED
  #if MI_THEAP_INITASNULL
  if (theap!=NULL)
  #endif
  if (huge_alignment==0 && mi_theap_malloc_use_guarded(theap, size)) {
    return _mi_theap_malloc_guarded(theap, size, zero);
  }
  #endif
  #if !MI_THEAP_INITASNULL
  mi_assert(theap!=NULL);
  #endif
  mi_assert(theap==NULL || theap->tld->thread_id == 0 || theap->tld->thread_id == _mi_thread_id());   // theaps are thread local
  mi_assert((huge_alignment & 1)==0);
  void* const p = _mi_malloc_generic(theap, size + MI_PADDING_SIZE, (zero ? 1 : 0) | huge_alignment, usable);  // note: size can overflow but it is detected in malloc_generic
  mi_track_malloc(p, size, zero);

  #if MI_DEBUG>3
  if (p != NULL && zero) {
    mi_assert_expensive(mi_mem_is_zero(p, size));
  }
  #endif
  return p;
}

// internal small allocation
static mi_decl_forceinline mi_decl_restrict void* mi_theap_malloc_small_zero(mi_theap_t* theap, size_t size, bool zero, size_t* usable) mi_attr_noexcept {
  #if MI_THEAP_INITASNULL
  if (theap!=NULL) {
    return mi_theap_malloc_small_zero_nonnull(theap, size, zero, usable);
  }
  else {
    return mi_theap_malloc_generic(theap, size, zero, 0, usable); // tailcall
  }
  #else
  return mi_theap_malloc_small_zero_nonnull(theap, size, zero, usable);
  #endif
}


// allocate a small block
mi_decl_nodiscard extern inline mi_decl_restrict void* mi_theap_malloc_small(mi_theap_t* theap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small_zero(theap, size, false, NULL);
}

mi_decl_nodiscard mi_decl_restrict void* mi_malloc_small(size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small(_mi_theap_default(), size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_malloc_small(mi_heap_t* heap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small_zero_nonnull(_mi_heap_theap(heap), size, false, NULL);
}

// The main internal allocation functions
static mi_decl_forceinline void* mi_theap_malloc_zero_nonnull(mi_theap_t* theap, size_t size, bool zero, size_t huge_alignment, size_t* usable) mi_attr_noexcept {
  // fast path for small objects
  if mi_likely(size <= MI_SMALL_SIZE_MAX) {
    mi_assert_internal(huge_alignment == 0);
    return mi_theap_malloc_small_zero_nonnull(theap, size, zero, usable);
  }
  else {
    return mi_theap_malloc_generic(theap, size, zero, huge_alignment, usable);
  }
}

extern mi_decl_forceinline void* _mi_theap_malloc_zero_ex(mi_theap_t* theap, size_t size, bool zero, size_t huge_alignment, size_t* usable) mi_attr_noexcept {
  // fast path for small objects
  #if MI_THEAP_INITASNULL
  if mi_likely(theap!=NULL && size <= MI_SMALL_SIZE_MAX)
  #else
  if mi_likely(size <= MI_SMALL_SIZE_MAX)
  #endif
  {
    mi_assert_internal(huge_alignment == 0);
    return mi_theap_malloc_small_zero_nonnull(theap, size, zero, usable);
  }
  else {
    return mi_theap_malloc_generic(theap, size, zero, huge_alignment, usable);
  }
}

void* _mi_theap_malloc_zero(mi_theap_t* theap, size_t size, bool zero, size_t* usable) mi_attr_noexcept {
  return _mi_theap_malloc_zero_ex(theap, size, zero, 0, usable);
}


// Main allocation functions

mi_decl_nodiscard extern inline mi_decl_restrict void* mi_theap_malloc(mi_theap_t* theap, size_t size) mi_attr_noexcept {
  return _mi_theap_malloc_zero(theap, size, false, NULL);
}

mi_decl_nodiscard mi_decl_restrict void* mi_malloc(size_t size) mi_attr_noexcept {
   return mi_theap_malloc(_mi_theap_default(), size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_malloc(mi_heap_t* heap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_zero_nonnull(_mi_heap_theap(heap), size, false, 0, NULL);
}


// zero initialized small block
mi_decl_nodiscard mi_decl_restrict void* mi_zalloc_small(size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small_zero(_mi_theap_default(), size, true, NULL);
}

mi_decl_nodiscard extern inline mi_decl_restrict void* mi_theap_zalloc_small(mi_theap_t* theap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small_zero(theap, size, true, NULL);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_zalloc_small(mi_heap_t* heap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_small_zero_nonnull(_mi_heap_theap(heap), size, true, NULL);
}


mi_decl_nodiscard extern inline mi_decl_restrict void* mi_theap_zalloc(mi_theap_t* theap, size_t size) mi_attr_noexcept {
  return _mi_theap_malloc_zero(theap, size, true, NULL);
}

mi_decl_nodiscard mi_decl_restrict void* mi_zalloc(size_t size) mi_attr_noexcept {
  return _mi_theap_malloc_zero(_mi_theap_default(), size, true, NULL);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_zalloc(mi_heap_t* heap, size_t size) mi_attr_noexcept {
  return mi_theap_malloc_zero_nonnull(_mi_heap_theap(heap), size, true, 0, NULL);
}

mi_decl_nodiscard extern inline mi_decl_restrict void* mi_theap_calloc(mi_theap_t* theap, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count,size,&total)) return NULL;
  return mi_theap_zalloc(theap,total);
}

mi_decl_nodiscard mi_decl_restrict void* mi_calloc(size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_calloc(_mi_theap_default(),count,size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_calloc(mi_heap_t* heap, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count, size, &total)) return NULL;
  return mi_heap_zalloc(heap, total);
}

// Return usable size
mi_decl_nodiscard mi_decl_restrict void* mi_umalloc_small(size_t size, size_t* usable) mi_attr_noexcept {
  return mi_theap_malloc_small_zero(_mi_theap_default(), size, false, usable);
}

mi_decl_nodiscard mi_decl_restrict void* mi_theap_umalloc(mi_theap_t* theap, size_t size, size_t* usable) mi_attr_noexcept {
  return _mi_theap_malloc_zero_ex(theap, size, false, 0, usable);
}

mi_decl_nodiscard mi_decl_restrict void* mi_umalloc(size_t size, size_t* usable) mi_attr_noexcept {
  return mi_theap_umalloc(_mi_theap_default(), size, usable);
}

mi_decl_nodiscard mi_decl_restrict void* mi_uzalloc(size_t size, size_t* usable) mi_attr_noexcept {
  return _mi_theap_malloc_zero_ex(_mi_theap_default(), size, true, 0, usable);
}

mi_decl_nodiscard mi_decl_restrict void* mi_ucalloc(size_t count, size_t size, size_t* usable) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count,size,&total)) return NULL;
  return mi_uzalloc(total, usable);
}

// Uninitialized `calloc`
static mi_decl_restrict void* mi_theap_mallocn(mi_theap_t* theap, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count, size, &total)) return NULL;
  return mi_theap_malloc(theap, total);
}

mi_decl_nodiscard mi_decl_restrict void* mi_mallocn(size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_mallocn(_mi_theap_default(),count,size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_mallocn(mi_heap_t* heap, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count, size, &total)) return NULL;
  return mi_heap_malloc(heap, total);
}


// Expand (or shrink) in place (or fail)
void* mi_expand(void* p, size_t newsize) mi_attr_noexcept {
  #if MI_PADDING
  // we do not shrink/expand with padding enabled
  MI_UNUSED(p); MI_UNUSED(newsize);
  return NULL;
  #else
  if (p == NULL) return NULL;
  const mi_page_t* const page = mi_validate_ptr_page(p,"mi_expand");
  const size_t size = _mi_usable_size(p,page);
  if (newsize > size) return NULL;
  return p; // it fits
  #endif
}

void* _mi_theap_realloc_zero(mi_theap_t* theap, void* p, size_t newsize, bool zero, size_t* usable_pre, size_t* usable_post) mi_attr_noexcept {
  // if p == NULL then behave as malloc.
  // else if size == 0 then reallocate to a zero-sized block (and don't return NULL, just as mi_malloc(0)).
  // (this means that returning NULL always indicates an error, and `p` will not have been freed in that case.)
  const mi_page_t* page;
  size_t size;
  if (p==NULL) {
    page = NULL;
    size = 0;
    if (usable_pre!=NULL) { *usable_pre = 0; }
  }
  else {
    page = mi_validate_ptr_page(p,"mi_realloc");
    size = _mi_usable_size(p,page);
    if (usable_pre!=NULL) { *usable_pre = mi_page_usable_block_size(page); }
  }
  if mi_unlikely(newsize<=size && newsize>=(size/2) && newsize>0  // note: newsize must be > 0 or otherwise we return NULL for realloc(NULL,0)
                  && mi_page_heap(page)==_mi_theap_heap(theap))             // and within the same heap
  {
    mi_assert_internal(p!=NULL);
    // todo: do not track as the usable size is still the same in the free; adjust potential padding?
    // mi_track_resize(p,size,newsize)
    // if (newsize < size) { mi_track_mem_noaccess((uint8_t*)p + newsize, size - newsize); }
    if (usable_post!=NULL) { *usable_post = mi_page_usable_block_size(page); }
    return p;  // reallocation still fits and not more than 50% waste
  }
  void* newp = mi_theap_umalloc(theap,newsize,usable_post);
  if mi_likely(newp != NULL) {
    if (zero && newsize > size) {
      // also set last word in the previous allocation to zero to ensure any padding is zero-initialized
      const size_t start = (size >= sizeof(intptr_t) ? size - sizeof(intptr_t) : 0);
      _mi_memzero((uint8_t*)newp + start, newsize - start);
    }
    else if (newsize == 0) {
      ((uint8_t*)newp)[0] = 0; // work around for applications that expect zero-reallocation to be zero initialized (issue #725)
    }
    if mi_likely(p != NULL) {
      const size_t copysize = (newsize > size ? size : newsize);
      mi_track_mem_defined(p,copysize);  // _mi_useable_size may be too large for byte precise memory tracking..
      _mi_memcpy(newp, p, copysize);
      mi_free(p); // only free the original pointer if successful  // todo: optimize since page is known?
    }
  }
  return newp;
}

mi_decl_nodiscard void* mi_theap_realloc(mi_theap_t* theap, void* p, size_t newsize) mi_attr_noexcept {
  return _mi_theap_realloc_zero(theap, p, newsize, false, NULL, NULL);
}

static void* mi_theap_reallocn(mi_theap_t* theap, void* p, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count, size, &total)) return NULL;
  return mi_theap_realloc(theap, p, total);
}


// Reallocate but free `p` on errors
static void* mi_theap_reallocf(mi_theap_t* theap, void* p, size_t newsize) mi_attr_noexcept {
  void* newp = mi_theap_realloc(theap, p, newsize);
  if (newp==NULL && p!=NULL) mi_free(p);
  return newp;
}

static void* mi_theap_rezalloc(mi_theap_t* theap, void* p, size_t newsize) mi_attr_noexcept {
  return _mi_theap_realloc_zero(theap, p, newsize, true, NULL, NULL);
}

static void* mi_theap_recalloc(mi_theap_t* theap, void* p, size_t count, size_t size) mi_attr_noexcept {
  size_t total;
  if (mi_count_size_overflow(count, size, &total)) return NULL;
  return mi_theap_rezalloc(theap, p, total);
}


mi_decl_nodiscard void* mi_realloc(void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_realloc(_mi_theap_default(),p,newsize);
}

mi_decl_nodiscard void* mi_reallocn(void* p, size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_reallocn(_mi_theap_default(),p,count,size);
}

mi_decl_nodiscard void* mi_urealloc(void* p, size_t newsize, size_t* usable_pre, size_t* usable_post) mi_attr_noexcept {
  return _mi_theap_realloc_zero(_mi_theap_default(),p,newsize, false, usable_pre, usable_post);
}

// Reallocate but free `p` on errors
mi_decl_nodiscard void* mi_reallocf(void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_reallocf(_mi_theap_default(),p,newsize);
}

mi_decl_nodiscard void* mi_rezalloc(void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_rezalloc(_mi_theap_default(), p, newsize);
}

mi_decl_nodiscard void* mi_recalloc(void* p, size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_recalloc(_mi_theap_default(), p, count, size);
}


mi_decl_nodiscard void* mi_heap_realloc(mi_heap_t* heap, void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_realloc(_mi_heap_theap(heap), p, newsize);
}

mi_decl_nodiscard void* mi_heap_reallocn(mi_heap_t* heap, void* p, size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_reallocn(_mi_heap_theap(heap), p, count, size);
}

// Reallocate but free `p` on errors
mi_decl_nodiscard void* mi_heap_reallocf(mi_heap_t* heap, void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_reallocf(_mi_heap_theap(heap), p, newsize);
}

mi_decl_nodiscard void* mi_heap_rezalloc(mi_heap_t* heap, void* p, size_t newsize) mi_attr_noexcept {
  return mi_theap_rezalloc(_mi_heap_theap(heap), p, newsize);
}

mi_decl_nodiscard void* mi_heap_recalloc(mi_heap_t* heap, void* p, size_t count, size_t size) mi_attr_noexcept {
  return mi_theap_recalloc(_mi_heap_theap(heap), p, count, size);
}



// ------------------------------------------------------
// strdup, strndup, and realpath
// ------------------------------------------------------

// `strdup` using mi_malloc
mi_decl_nodiscard static mi_decl_restrict char* mi_theap_strdup(mi_theap_t* theap, const char* s) mi_attr_noexcept {
  if (s == NULL) return NULL;
  size_t len = _mi_strlen(s);
  if (len > MI_MAX_ALLOC_SIZE - 1) return NULL;  // prevent overflow on len+1
  char* t = (char*)mi_theap_malloc(theap,len+1);
  if (t == NULL) return NULL;
  _mi_memcpy(t, s, len);
  t[len] = 0;
  return t;
}

mi_decl_nodiscard mi_decl_restrict char* mi_strdup(const char* s) mi_attr_noexcept {
  return mi_theap_strdup(_mi_theap_default(), s);
}

mi_decl_nodiscard mi_decl_restrict char* mi_heap_strdup(mi_heap_t* heap, const char* s) mi_attr_noexcept {
  return mi_theap_strdup(_mi_heap_theap(heap), s);
}

// `strndup` using mi_malloc
mi_decl_nodiscard static mi_decl_restrict char* mi_theap_strndup(mi_theap_t* theap, const char* s, size_t n) mi_attr_noexcept {
  if (s == NULL) return NULL;
  const size_t len = _mi_strnlen(s,n);  // len <= n
  if (len > MI_MAX_ALLOC_SIZE - 1) return NULL;  // prevent overflow on len+1
  char* t = (char*)mi_theap_malloc(theap, len+1);
  if (t == NULL) return NULL;
  _mi_memcpy(t, s, len);
  t[len] = 0;
  return t;
}

mi_decl_nodiscard mi_decl_restrict char* mi_strndup(const char* s, size_t n) mi_attr_noexcept {
  return mi_theap_strndup(_mi_theap_default(),s,n);
}

mi_decl_nodiscard mi_decl_restrict char* mi_heap_strndup(mi_heap_t* heap, const char* s, size_t n) mi_attr_noexcept {
  return mi_theap_strndup(_mi_heap_theap(heap), s, n);
}

#ifndef __wasi__
// `realpath` using mi_malloc
#ifdef _WIN32
#ifndef PATH_MAX
#define PATH_MAX MAX_PATH
#endif

mi_decl_nodiscard static mi_decl_restrict char* mi_theap_realpath(mi_theap_t* theap, const char* fname, char* resolved_name) mi_attr_noexcept {
  // todo: use GetFullPathNameW to allow longer file names
  char buf[PATH_MAX];
  DWORD res = GetFullPathNameA(fname, PATH_MAX, (resolved_name == NULL ? buf : resolved_name), NULL);
  if (res == 0) {
    errno = GetLastError(); return NULL;
  }
  else if (res > PATH_MAX) {
    errno = EINVAL; return NULL;
  }
  else if (resolved_name != NULL) {
    return resolved_name;
  }
  else {
    return mi_theap_strndup(theap, buf, PATH_MAX);
  }
}

#else

#include <unistd.h>  // pathconf

static size_t mi_path_max(void) {
  static _Atomic(size_t) path_max = 0;
  size_t pmax = mi_atomic_load_acquire(&path_max);
  if (pmax == 0) {
    long m = 0;
    #ifdef _PC_PATH_MAX
    m = pathconf("/",_PC_PATH_MAX);
    #endif
    if (m <= 0) pmax = 4096;      // guess
    else if (m < 256) pmax = 256; // at least 256
    else if (m > 64*1024) pmax = 64*1024;  // at most 64 KiB
    else pmax = m;
    size_t expected = 0;
    mi_atomic_cas_strong_acq_rel(&path_max, &expected, pmax);
  }
  return pmax;
}

char* mi_theap_realpath(mi_theap_t* theap, const char* fname, char* resolved_name) mi_attr_noexcept {
  if (resolved_name != NULL) {
    return realpath(fname,resolved_name);
  }
  else {
  /*
    char* rname = realpath(fname, NULL);
    if (rname == NULL) return NULL;
    char* result = mi_heap_strdup(heap, rname);
    mi_cfree(rname);  // note: may leak the original pointer if allocated internally with the system allocator 
    // note: with ASAN realpath is intercepted and mi_cfree may leak the returned pointer :-(
    return result;  
  */
    const size_t n  = mi_path_max();
    char* const buf = (char*)mi_zalloc(n+1);
    if (buf == NULL) {
      errno = ENOMEM;
      return NULL;
    }
    char* rname  = realpath(fname,buf);
    char* result = mi_theap_strndup(theap,rname,n); // ok if `rname==NULL`
    mi_free(buf);
    return result;
  }
}
#endif

mi_decl_nodiscard mi_decl_restrict char* mi_realpath(const char* fname, char* resolved_name) mi_attr_noexcept {
  return mi_theap_realpath(_mi_theap_default(),fname,resolved_name);
}

mi_decl_nodiscard mi_decl_restrict char* mi_heap_realpath(mi_heap_t* heap, const char* fname, char* resolved_name) mi_attr_noexcept {
  return mi_theap_realpath(_mi_heap_theap(heap), fname, resolved_name);
}
#endif

/*-------------------------------------------------------
C++ new and new_aligned
The standard requires calling into `get_new_handler` and
throwing the bad_alloc exception on failure. If we compile
with a C++ compiler we can implement this precisely. If we
use a C compiler we cannot throw a `bad_alloc` exception
but we call `exit` instead (i.e. not returning).
-------------------------------------------------------*/

#ifdef __cplusplus
#include <new>
static bool mi_try_new_handler(bool nothrow) {
  #if defined(_MSC_VER) || (__cplusplus >= 201103L)
    std::new_handler h = std::get_new_handler();
  #else
    std::new_handler h = std::set_new_handler();
    std::set_new_handler(h);
  #endif
  if (h==NULL) {
    _mi_error_message(ENOMEM, "out of memory in 'new'");
    #if defined(_CPPUNWIND) || defined(__cpp_exceptions)  // exceptions are not always enabled
    if (!nothrow) {
      throw std::bad_alloc();
    }
    #else
    MI_UNUSED(nothrow);
    #endif
    return false;
  }
  else {
    h();
    return true;
  }
}
#else
typedef void (*std_new_handler_t)(void);

#if (defined(__GNUC__) || (defined(__clang__) && !defined(_MSC_VER)))  // exclude clang-cl, see issue #631
std_new_handler_t __attribute__((weak)) _ZSt15get_new_handlerv(void) {
  return NULL;
}
static std_new_handler_t mi_get_new_handler(void) {
  return _ZSt15get_new_handlerv();
}
#else
// note: on windows we could dynamically link to `?get_new_handler@std@@YAP6AXXZXZ`.
static std_new_handler_t mi_get_new_handler(void) {
  return NULL;
}
#endif

static bool mi_try_new_handler(bool nothrow) {
  std_new_handler_t h = mi_get_new_handler();
  if (h==NULL) {
    _mi_error_message(ENOMEM, "out of memory in 'new'");
    if (!nothrow) {
      abort();  // cannot throw in plain C, use abort
    }
    return false;
  }
  else {
    h();
    return true;
  }
}
#endif

static mi_decl_noinline void* mi_theap_try_new(mi_theap_t* theap, size_t size, bool nothrow ) {
  void* p = NULL;
  while(p == NULL && mi_try_new_handler(nothrow)) {
    p = mi_theap_malloc(theap,size);
  }
  return p;
}

static mi_decl_noinline void* mi_try_new(size_t size, bool nothrow) {
  return mi_theap_try_new(_mi_theap_default(), size, nothrow);
}

static mi_decl_noinline void* mi_heap_try_new(mi_heap_t* heap, size_t size, bool nothrow) {
  return mi_theap_try_new(_mi_heap_theap(heap), size, nothrow);
}


mi_decl_nodiscard static mi_decl_restrict void* mi_theap_alloc_new(mi_theap_t* theap, size_t size) {
  void* p = mi_theap_malloc(theap,size);
  if mi_unlikely(p == NULL) return mi_theap_try_new(theap, size, false);
  return p;
}

mi_decl_nodiscard mi_decl_restrict void* mi_new(size_t size) {
  return mi_theap_alloc_new(_mi_theap_default(), size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_alloc_new(mi_heap_t* heap, size_t size) {
  void* p = mi_heap_malloc(heap, size);
  if mi_unlikely(p == NULL) return mi_heap_try_new(heap, size, false);
  return p;
}


mi_decl_nodiscard static mi_decl_restrict void* mi_theap_alloc_new_n(mi_theap_t* theap, size_t count, size_t size) {
  size_t total;
  if mi_unlikely(mi_count_size_overflow(count, size, &total)) {
    mi_try_new_handler(false);  // on overflow we invoke the try_new_handler once to potentially throw std::bad_alloc
    return NULL;
  }
  else {
    return mi_theap_alloc_new(theap,total);
  }
}

mi_decl_nodiscard mi_decl_restrict void* mi_new_n(size_t count, size_t size) {
  return mi_theap_alloc_new_n(_mi_theap_default(), count, size);
}

mi_decl_nodiscard mi_decl_restrict void* mi_heap_alloc_new_n(mi_heap_t* heap, size_t count, size_t size) {
  return mi_theap_alloc_new_n(_mi_heap_theap(heap), count, size);
}


mi_decl_nodiscard mi_decl_restrict void* mi_new_nothrow(size_t size) mi_attr_noexcept {
  void* p = mi_malloc(size);
  if mi_unlikely(p == NULL) return mi_try_new(size, true);
  return p;
}

mi_decl_nodiscard mi_decl_restrict void* mi_new_aligned(size_t size, size_t alignment) {
  void* p;
  do {
    p = mi_malloc_aligned(size, alignment);
  }
  while(p == NULL && mi_try_new_handler(false));
  return p;
}

mi_decl_nodiscard mi_decl_restrict void* mi_new_aligned_nothrow(size_t size, size_t alignment) mi_attr_noexcept {
  void* p;
  do {
    p = mi_malloc_aligned(size, alignment);
  }
  while(p == NULL && mi_try_new_handler(true));
  return p;
}

mi_decl_nodiscard void* mi_new_realloc(void* p, size_t newsize) {
  void* q;
  do {
    q = mi_realloc(p, newsize);
  } while (q == NULL && mi_try_new_handler(false));
  return q;
}

mi_decl_nodiscard void* mi_new_reallocn(void* p, size_t newcount, size_t size) {
  size_t total;
  if mi_unlikely(mi_count_size_overflow(newcount, size, &total)) {
    mi_try_new_handler(false);  // on overflow we invoke the try_new_handler once to potentially throw std::bad_alloc
    return NULL;
  }
  else {
    return mi_new_realloc(p, total);
  }
}

#if MI_GUARDED
// We always allocate a guarded allocation at an offset (`mi_page_has_interior_pointers` will be true).
// We then set the first word of the block to `0` for regular offset aligned allocations (in `alloc-aligned.c`)
// and the first word to `~0` for guarded allocations to have a correct `mi_usable_size`

static void* mi_block_ptr_set_guarded(mi_block_t* block, size_t obj_size) {
  // TODO: we can still make padding work by moving it out of the guard page area
  mi_page_t* const page = _mi_ptr_page(block);
  mi_page_set_has_interior_pointers(page, true);
  block->next = MI_BLOCK_TAG_GUARDED;

  // set guard page at the end of the block
  const size_t block_size = mi_page_block_size(page);  // must use `block_size` to match `mi_free_local`
  const size_t os_page_size = _mi_os_page_size();
  mi_assert_internal(block_size >= obj_size + os_page_size + sizeof(mi_block_t));
  if (block_size < obj_size + os_page_size + sizeof(mi_block_t)) {
    // should never happen
    mi_free(block);
    return NULL;
  }
  uint8_t* guard_page = (uint8_t*)block + block_size - os_page_size;
  // note: the alignment of the guard page relies on blocks being os_page_size aligned which
  // is ensured in `mi_arena_page_alloc_fresh`.  
  mi_assert_internal(_mi_is_aligned(block, os_page_size));
  mi_assert_internal(_mi_is_aligned(guard_page, os_page_size));
  if (!page->memid.is_pinned && _mi_is_aligned(guard_page, os_page_size)) {
    const bool ok = _mi_os_protect(guard_page, os_page_size);
    if mi_unlikely(!ok) {
      _mi_warning_message("failed to set a guard page behind an object (object %p of size %zu)\n", block, block_size);
    }
  }
  else {
    _mi_warning_message("unable to set a guard page behind an object due to pinned memory (large OS pages?) (object %p of size %zu)\n", block, block_size);
  }

  // align pointer just in front of the guard page
  size_t offset = block_size - os_page_size - obj_size;
  mi_assert_internal(offset > sizeof(mi_block_t));
  if (offset > MI_PAGE_MAX_OVERALLOC_ALIGN) {
    // give up to place it right in front of the guard page if the offset is too large for unalignment
    offset = MI_PAGE_MAX_OVERALLOC_ALIGN;
  }
  uint8_t* const p = (uint8_t*)block + offset;
  mi_assert_internal(p == guard_page - obj_size);
  mi_track_align(block, p, offset, obj_size);
  mi_track_mem_defined(block, sizeof(mi_block_t));
  return p;
}

mi_decl_restrict void* _mi_theap_malloc_guarded(mi_theap_t* theap, size_t size, bool zero) mi_attr_noexcept
{
  // allocate multiple of page size ending in a guard page
  // ensure minimal alignment requirement?
  if mi_unlikely(size >= MI_MAX_ALLOC_SIZE - MI_PADDING_SIZE) {  // check up front so the `req_size` won't overflow    
    _mi_error_message(EOVERFLOW, "(guarded) allocation request is too large (%zu bytes)\n", size);
    return NULL;
  }
  const size_t os_page_size = _mi_os_page_size();
  const size_t obj_size = (mi_option_is_enabled(mi_option_guarded_precise) ? size : _mi_align_up(size, MI_MAX_ALIGN_SIZE));
  const size_t bsize    = _mi_align_up(_mi_align_up(obj_size, MI_MAX_ALIGN_SIZE) + sizeof(mi_block_t), MI_MAX_ALIGN_SIZE);
  const size_t req_size = _mi_align_up(bsize + os_page_size, os_page_size);  
  mi_block_t* const block = (mi_block_t*)_mi_malloc_generic(theap, req_size, 0 /* don't zero */, NULL);
  if (block==NULL) return NULL;
  void* const p = mi_block_ptr_set_guarded(block, obj_size);
  if (p == NULL) return p;
  if (zero) { 
    _mi_memzero_aligned(p,obj_size);  // we have to zero here as padding might have written here (if the blocksize > reqsize + os_page_size)
  }

  // stats
  mi_track_malloc(p, obj_size, zero);    
  if (!mi_theap_is_initialized(theap)) { theap = _mi_theap_default(); }
  mi_theap_stat_counter_increase(theap, malloc_guarded_count, 1);
  #if MI_STAT>1
  // adjust stats to only count the allocated size of the block (and not the guard page)
  mi_theap_stat_adjust_decrease(theap, malloc_requested, req_size);
  mi_theap_stat_increase(theap, malloc_requested, size);
  #endif
  #if MI_DEBUG>3
  if (zero) {
    mi_assert_expensive(mi_mem_is_zero(p, size));
  }
  #endif
  return p;
}
#endif

// ------------------------------------------------------
// ensure explicit external inline definitions are emitted!
// ------------------------------------------------------

#ifdef __cplusplus
void* _mi_externs[] = {
  (void*)&_mi_page_malloc_zero,
  (void*)&_mi_theap_malloc_zero,
  (void*)&_mi_theap_malloc_zero_ex,
  (void*)&mi_theap_malloc,
  (void*)&mi_theap_zalloc,
  (void*)&mi_theap_malloc_small,
  (void*)&mi_malloc,
  (void*)&mi_malloc_small,
  (void*)&mi_zalloc,
  (void*)&mi_zalloc_small,
  (void*)&mi_heap_malloc,
  (void*)&mi_heap_malloc_small,
  (void*)&mi_malloc_aligned
  // (void*)&mi_theap_alloc_new,
  // (void*)&mi_theap_alloc_new_n
};
#endif
