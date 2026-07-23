/*----------------------------------------------------------------------------
Copyright (c) 2018-2025, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/

#include "mimalloc.h"
#include "mimalloc/internal.h"
#include "mimalloc/prim.h"  // _mi_theap_default

#if defined(_MSC_VER) && (_MSC_VER < 1920)
#pragma warning(disable:4204)  // non-constant aggregate initializer
#endif

/* -----------------------------------------------------------
  Helpers
----------------------------------------------------------- */

// return `true` if ok, `false` to break
typedef bool (theap_page_visitor_fun)(mi_theap_t* theap, mi_page_queue_t* pq, mi_page_t* page, void* arg1, void* arg2);

// Visit all pages in a theap; returns `false` if break was called.
static bool mi_theap_visit_pages(mi_theap_t* theap, theap_page_visitor_fun* fn, bool include_full, void* arg1, void* arg2)
{
  if (theap==NULL || theap->page_count==0) return 0;

  // visit all pages
  #if MI_DEBUG>1
  size_t total = theap->page_count;
  size_t count = 0;
  #endif

  const size_t max_bin = (include_full ? MI_BIN_FULL : MI_BIN_FULL - 1);
  for (size_t i = 0; i <= max_bin; i++) {
    mi_page_queue_t* pq = &theap->pages[i];
    mi_page_t* page = pq->first;
    while(page != NULL) {
      mi_page_t* next = page->next; // save next in case the page gets removed from the queue
      mi_assert_internal(mi_page_theap(page) == theap);
      #if MI_DEBUG>1
      count++;
      #endif
      if (!fn(theap, pq, page, arg1, arg2)) return false;
      page = next; // and continue
    }
  }
  mi_assert_internal(!include_full || count == total);
  return true;
}


#if MI_DEBUG>=2
static bool mi_theap_page_is_valid(mi_theap_t* theap, mi_page_queue_t* pq, mi_page_t* page, void* arg1, void* arg2) {
  MI_UNUSED(arg1);
  MI_UNUSED(arg2);
  MI_UNUSED(pq);
  mi_assert_internal(mi_page_theap(page) == theap);
  mi_assert_expensive(_mi_page_is_valid(page));
  return true;
}
#endif
#if MI_DEBUG>=3
static bool mi_theap_is_valid(mi_theap_t* theap) {
  mi_assert_internal(theap!=NULL);
  mi_theap_visit_pages(theap, &mi_theap_page_is_valid, true, NULL, NULL);
  for (size_t bin = 0; bin < MI_BIN_COUNT; bin++) {
    mi_assert_internal(_mi_page_queue_is_valid(theap, &theap->pages[bin]));
  }
  return true;
}
#endif




/* -----------------------------------------------------------
  "Collect" pages by migrating `local_free` and `thread_free`
  lists and freeing empty pages. This is done when a thread
  stops (and in that case abandons pages if there are still
  blocks alive)
----------------------------------------------------------- */

typedef enum mi_collect_e {
  MI_NORMAL,
  MI_FORCE,
  MI_ABANDON
} mi_collect_t;


static bool mi_theap_page_collect(mi_theap_t* theap, mi_page_queue_t* pq, mi_page_t* page, void* arg_collect, void* arg2 ) {
  MI_UNUSED(arg2);
  MI_UNUSED(theap);
  mi_assert_internal(mi_theap_page_is_valid(theap, pq, page, NULL, NULL));
  mi_collect_t collect = *((mi_collect_t*)arg_collect);
  _mi_page_free_collect(page, collect >= MI_FORCE);
  if (mi_page_all_free(page)) {
    // no more used blocks, possibly free the page.
    if (collect >= MI_FORCE || page->retire_expire == 0) {  // either forced/abandon, or not already retired
      // note: this will potentially free retired pages as well.
      _mi_page_free(page, pq);
    }
  }
  else if (collect == MI_ABANDON) {
    // still used blocks but the thread is done; abandon the page
    _mi_page_abandon(page, pq);
  }
  return true; // don't break
}

static void mi_theap_merge_stats(mi_theap_t* theap) {
  mi_assert_internal(mi_theap_is_initialized(theap));
  _mi_stats_merge_into(&_mi_theap_heap(theap)->stats, &theap->stats);
}

static void mi_theap_collect_ex(mi_theap_t* theap, mi_collect_t collect)
{
  if (theap==NULL || !mi_theap_is_initialized(theap)) return;
  mi_assert_expensive(mi_theap_is_valid(theap));

  const bool force = (collect >= MI_FORCE);
  _mi_deferred_free(theap, force);

  // python/cpython#112532: we may be called from a thread that is not the owner of the theap
  // const bool is_main_thread = (_mi_is_main_thread() && theap->thread_id == _mi_thread_id());

  // collect retired pages
  _mi_theap_collect_retired(theap, force);

  // collect all pages owned by this thread
  mi_theap_visit_pages(theap, &mi_theap_page_collect, (collect!=MI_NORMAL), &collect, NULL);  // dont normally visit full pages, see issue #1220

  // collect arenas (this is program wide so don't force purges on abandonment of threads)
  //mi_atomic_storei64_release(&theap->tld->subproc->purge_expire, 1);
  _mi_arenas_collect(collect == MI_FORCE /* force purge? */, collect >= MI_FORCE /* visit all? */, theap->tld);

  // merge statistics
  mi_theap_merge_stats(theap);
}

void _mi_theap_collect_abandon(mi_theap_t* theap) {
  mi_theap_collect_ex(theap, MI_ABANDON);
}

void mi_theap_collect(mi_theap_t* theap, bool force) mi_attr_noexcept {
  mi_theap_collect_ex(theap, (force ? MI_FORCE : MI_NORMAL));
}

void mi_collect(bool force) mi_attr_noexcept {
  // cannot really collect process wide, just a theap..
  mi_theap_collect(_mi_theap_default(), force);
}

void mi_heap_collect(mi_heap_t* heap, bool force) {
  // cannot really collect a heap, just a theap..
  mi_theap_collect(mi_heap_theap(heap), force);
}

/* -----------------------------------------------------------
  Heap new
----------------------------------------------------------- */

mi_theap_t* mi_theap_get_default(void) {
  mi_theap_t* theap = _mi_theap_default();
  if mi_unlikely(!mi_theap_is_initialized(theap)) {
    mi_thread_init();
    theap = _mi_theap_default();
    mi_assert_internal(mi_theap_is_initialized(theap));
  }
  return theap;
}

// todo: make order of parameters consistent (but would that break compat with CPython?)
void _mi_theap_init(mi_theap_t* theap, mi_heap_t* heap, mi_tld_t* tld)
{
  mi_assert_internal(theap!=NULL);
  mi_assert_internal(heap!=NULL);
  mi_memid_t memid = theap->memid;
  _mi_memcpy_aligned(theap, &_mi_theap_empty, sizeof(mi_theap_t));
  theap->memid = memid;
  theap->refcount = 1;
  theap->tld   = tld;  // avoid reading the thread-local tld during initialization
  mi_atomic_store_ptr_relaxed(mi_heap_t,&theap->heap,heap);
  mi_assert_internal(theap->stats.size == sizeof(mi_stats_t));
  
  _mi_theap_options_init(theap);
  if (theap->tld->is_in_threadpool) {
    // if we run as part of a thread pool it is better to not arbitrarily reclaim abandoned pages into our theap.
    // this is checked in `free.c:mi_free_try_collect_mt`
    // .. but abandoning is good in this case: halve the full page retain (possibly to 0)
    // (so blocked threads do not hold on to too much memory)
    if (theap->page_full_retain > 0) {
      theap->page_full_retain = theap->page_full_retain / 4;
    }
  }

  // push on the thread local theaps list
  mi_theap_t* head = NULL;
  mi_lock(&theap->tld->theaps_lock) {
    head = theap->tld->theaps;
    theap->tprev = NULL;
    theap->tnext = head;
    if (head!=NULL) { head->tprev = theap; }
    theap->tld->theaps = theap;
  }

  // initialize random
  if (head == NULL) {  // first theap in this thread?
    #if defined(_WIN32) && !defined(MI_SHARED_LIB)
      _mi_random_init_weak(&theap->random);    // prevent allocation failure during bcrypt dll initialization with static linking (issue #1185)
    #else
      _mi_random_init(&theap->random);
    #endif
  }
  else {
    _mi_random_split(&head->random, &theap->random);
  }
  theap->cookie  = _mi_theap_random_next(theap) | 1;
  _mi_theap_guarded_init(theap);
  mi_subproc_stat_increase(_mi_subproc(),theaps,1);

  // push on the heap's theap list
  mi_lock(&heap->theaps_lock) {
    head = heap->theaps;
    theap->hprev = NULL;
    theap->hnext = head;
    if (head!=NULL) { head->hprev = theap; }
    heap->theaps = theap;
  }
}

mi_theap_t* _mi_theap_create(mi_heap_t* heap, mi_tld_t* tld) {
  mi_assert_internal(tld!=NULL);
  mi_assert_internal(heap!=NULL);
  // allocate and initialize a theap
  mi_memid_t memid;
  mi_theap_t* theap;
  //if (!_mi_is_heap_main(heap)) {
  //  theap = (mi_theap_t*)mi_heap_zalloc(mi_heap_main(),sizeof(mi_theap_t));
  //  memid = _mi_memid_create(MI_MEM_HEAP_MAIN);
  //  memid.initially_zero = memid.initially_committed = true;
  //}
  //else
  if (heap->exclusive_arena == NULL) {
    theap = (mi_theap_t*)_mi_meta_zalloc(sizeof(mi_theap_t), &memid);
  }
  else {
    // theaps associated with a specific arena are allocated in that arena
    // note: takes up at least one slice which is quite wasteful...
    const size_t size = _mi_align_up(sizeof(mi_theap_t),MI_ARENA_MIN_OBJ_SIZE);
    theap = (mi_theap_t*)_mi_arenas_alloc(heap, size, true, true, heap->exclusive_arena, tld->thread_seq, tld->numa_node, &memid);
    mi_assert_internal(memid.mem.os.size >= size);
  }
  if (theap==NULL) {
    _mi_error_message(ENOMEM, "unable to allocate theap meta-data\n");
    return NULL;
  }
  theap->memid = memid;
  _mi_theap_init(theap, heap, tld);
  return theap;
}

uintptr_t _mi_theap_random_next(mi_theap_t* theap) {
  return _mi_random_next(&theap->random);
}

static void mi_theap_free_mem(mi_theap_t* theap) {
  if (theap!=NULL) {
    mi_subproc_stat_decrease(_mi_subproc(),theaps,1);
    // free the used memory
    if (theap->memid.memkind == MI_MEM_HEAP_MAIN) {  // note: for now unused as it would access theap_default stats in mi_free of the current theap
      mi_assert_internal(_mi_is_heap_main(mi_heap_of(theap)));
      mi_free(theap);
    }
    else if (theap->memid.memkind == MI_MEM_META) {
      _mi_meta_free(theap, sizeof(*theap), theap->memid);
    }
    else {
      _mi_arenas_free(theap, _mi_align_up(sizeof(*theap),MI_ARENA_MIN_OBJ_SIZE), theap->memid ); // issue #1168, avoid assertion failure
    }
  }
}

void _mi_theap_incref(mi_theap_t* theap) {
  if (theap!=NULL && theap->memid.memkind > MI_MEM_STATIC) {
    mi_atomic_increment_acq_rel(&theap->refcount);
  }
}

void _mi_theap_decref(mi_theap_t* theap) {
  if (theap!=NULL && theap->memid.memkind > MI_MEM_STATIC) {
    if (mi_atomic_decrement_acq_rel(&theap->refcount) == 1) {
      mi_theap_free_mem(theap);
    }
  }
}


// called from `mi_theap_delete` to free the internal theap resources.
bool _mi_theap_free(mi_theap_t* theap, bool acquire_heap_theaps_lock, bool acquire_tld_theaps_lock) {
  mi_assert(theap != NULL);
  if (theap==NULL) return true;

  mi_heap_t* const heap = mi_atomic_exchange_ptr_acq_rel(mi_heap_t, &theap->heap, NULL);
  if (heap==NULL) {
    // concurrent interaction, retry in an outer loop (as the other thread may be blocked on our lock)
    return false;
  }
  else {
    // merge stats to the owning heap
    _mi_stats_merge_into(&heap->stats, &theap->stats);

    // remove ourselves from the heap theaps list
    mi_lock_maybe(&heap->theaps_lock, acquire_heap_theaps_lock) {
      if (theap->hnext != NULL) { theap->hnext->hprev = theap->hprev; }
      if (theap->hprev != NULL) { theap->hprev->hnext = theap->hnext; }
                          else { mi_assert_internal(heap->theaps == theap); heap->theaps = theap->hnext; }
      theap->hnext = theap->hprev = NULL;
    }

    // remove ourselves from the thread local theaps list
    mi_lock_maybe(&theap->tld->theaps_lock, acquire_tld_theaps_lock) {
      if (theap->tnext != NULL) { theap->tnext->tprev = theap->tprev;  }
      if (theap->tprev != NULL) { theap->tprev->tnext = theap->tnext;  }
                          else { mi_assert_internal(theap->tld->theaps == theap); theap->tld->theaps = theap->tnext; }
      theap->tnext = theap->tprev = NULL;                        
    }
    theap->tld = NULL;
    _mi_theap_decref(theap);
    return true;
  }
}


/* -----------------------------------------------------------
  Heap destroy
----------------------------------------------------------- */
/*

// zero out the page queues
static void mi_theap_reset_pages(mi_theap_t* theap) {
  mi_assert_internal(theap != NULL);
  mi_assert_internal(mi_theap_is_initialized(theap));
  // TODO: copy full empty theap instead?
  _mi_memset(&theap->pages_free_direct, 0, sizeof(theap->pages_free_direct));
  _mi_memcpy_aligned(&theap->pages, &_mi_theap_empty.pages, sizeof(theap->pages));
  // theap->thread_delayed_free = NULL;
  theap->page_count = 0;
}

static bool _mi_theap_page_destroy(mi_theap_t* theap, mi_page_queue_t* pq, mi_page_t* page, void* arg1, void* arg2) {
  MI_UNUSED(arg1);
  MI_UNUSED(arg2);
  MI_UNUSED(pq);

  // ensure no more thread_delayed_free will be added
  //_mi_page_use_delayed_free(page, MI_NEVER_DELAYED_FREE, false);

  // stats
  const size_t bsize = mi_page_block_size(page);
  if (bsize > MI_LARGE_MAX_OBJ_SIZE) {
    mi_theap_stat_decrease(theap, malloc_huge, bsize);
  }
  #if (MI_STAT>0)
  _mi_page_free_collect(page, false);  // update used count
  const size_t inuse = page->used;
  if (bsize <= MI_LARGE_MAX_OBJ_SIZE) {
    mi_theap_stat_decrease(theap, malloc_normal, bsize * inuse);
    #if (MI_STAT>1)
    mi_theap_stat_decrease(theap, malloc_bins[_mi_bin(bsize)], inuse);
    #endif
  }
  // mi_theap_stat_decrease(theap, malloc_requested, bsize * inuse);  // todo: off for aligned blocks...
  #endif

  /// pretend it is all free now
  mi_assert_internal(mi_page_thread_free(page) == NULL);
  page->used = 0;

  // and free the page
  // mi_page_free(page,false);
  page->next = NULL;
  page->prev = NULL;
  mi_page_set_theap(page, NULL);
  _mi_arenas_page_free(page, theap);

  return true; // keep going
}

void _mi_theap_destroy_pages(mi_theap_t* theap) {
  mi_theap_visit_pages(theap, &_mi_theap_page_destroy, NULL, NULL);
  mi_theap_reset_pages(theap);
}

#if MI_TRACK_HEAP_DESTROY
static bool mi_cdecl mi_theap_track_block_free(const mi_theap_t* theap, const mi_theap_area_t* area, void* block, size_t block_size, void* arg) {
  MI_UNUSED(theap); MI_UNUSED(area);  MI_UNUSED(arg); MI_UNUSED(block_size);
  mi_track_free_size(block,mi_usable_size(block));
  return true;
}
#endif

void mi_theap_destroy(mi_theap_t* theap) {
  mi_assert(theap != NULL);
  mi_assert(mi_theap_is_initialized(theap));
  mi_assert(!theap->allow_page_reclaim);
  mi_assert(!theap->allow_page_abandon);
  mi_assert_expensive(mi_theap_is_valid(theap));
  if (theap==NULL || !mi_theap_is_initialized(theap)) return;
  #if MI_GUARDED
  // _mi_warning_message("'mi_theap_destroy' called but MI_GUARDED is enabled -- using `mi_theap_delete` instead (theap at %p)\n", theap);
  mi_theap_delete(theap);
  return;
  #else
  if (theap->allow_page_reclaim) {
    _mi_warning_message("'mi_theap_destroy' called but ignored as the theap was not created with 'allow_destroy' (theap at %p)\n", theap);
    // don't free in case it may contain reclaimed pages,
    mi_theap_delete(theap);
  }
  else {
    // track all blocks as freed
    #if MI_TRACK_HEAP_DESTROY
    mi_theap_visit_blocks(theap, true, mi_theap_track_block_free, NULL);
    #endif
    // free all pages
    _mi_theap_destroy_pages(theap);
    mi_theap_free(theap,true);
  }
  #endif
}

// forcefully destroy all theaps in the current thread
void _mi_theap_unsafe_destroy_all(mi_theap_t* theap) {
  mi_assert_internal(theap != NULL);
  if (theap == NULL) return;
  mi_theap_t* curr = theap->tld->theaps;
  while (curr != NULL) {
    mi_theap_t* next = curr->next;
    if (!curr->allow_page_reclaim) {
      mi_theap_destroy(curr);
    }
    else {
      _mi_theap_destroy_pages(curr);
    }
    curr = next;
  }
}
*/

/* -----------------------------------------------------------
  Safe Heap delete
----------------------------------------------------------- */

// Safe delete a theap without freeing any still allocated blocks in that theap.
void _mi_theap_delete(mi_theap_t* theap, bool acquire_tld_theaps_lock)
{
  mi_assert(theap != NULL);
  mi_assert(mi_theap_is_initialized(theap));
  mi_assert_expensive(mi_theap_is_valid(theap));
  if (theap==NULL || !mi_theap_is_initialized(theap)) return;

  // abandon all pages
  _mi_theap_collect_abandon(theap);

  mi_assert_internal(theap->page_count==0);
  _mi_theap_free(theap, true /* acquire heap->theaps_lock */, acquire_tld_theaps_lock);
}



/* -----------------------------------------------------------
  Load/unload theaps
----------------------------------------------------------- */
/*
void mi_theap_unload(mi_theap_t* theap) {
  mi_assert(mi_theap_is_initialized(theap));
  mi_assert_expensive(mi_theap_is_valid(theap));
  if (theap==NULL || !mi_theap_is_initialized(theap)) return;
  if (_mi_theap_heap(theap)->exclusive_arena == NULL) {
    _mi_warning_message("cannot unload theaps that are not associated with an exclusive arena\n");
    return;
  }

  // abandon all pages so all thread'id in the pages are cleared
  _mi_theap_collect_abandon(theap);
  mi_assert_internal(theap->page_count==0);

  // remove from theap list
  mi_theap_free(theap, false); // but don't actually free the memory

  // disassociate from the current thread-local and static state
  theap->tld = NULL;
  return;
}

bool mi_theap_reload(mi_theap_t* theap, mi_arena_id_t arena_id) {
  mi_assert(mi_theap_is_initialized(theap));
  if (theap==NULL || !mi_theap_is_initialized(theap)) return false;
  if (_mi_theap_heap(theap)->exclusive_arena == NULL) {
    _mi_warning_message("cannot reload theaps that were not associated with an exclusive arena\n");
    return false;
  }
  if (theap->tld != NULL) {
    _mi_warning_message("cannot reload theaps that were not unloaded first\n");
    return false;
  }
  mi_arena_t* arena = _mi_arena_from_id(arena_id);
  if (_mi_theap_heap(theap)->exclusive_arena != arena) {
    _mi_warning_message("trying to reload a theap at a different arena address: %p vs %p\n", _mi_theap_heap(theap)->exclusive_arena, arena);
    return false;
  }

  mi_assert_internal(theap->page_count==0);

  // re-associate with the current thread-local and static state
  theap->tld = mi_theap_get_default()->tld;

  // reinit direct pages (as we may be in a different process)
  mi_assert_internal(theap->page_count == 0);
  for (size_t i = 0; i < MI_PAGES_DIRECT; i++) {
    theap->pages_free_direct[i] = (mi_page_t*)&_mi_page_empty;
  }

  // push on the thread local theaps list
  theap->tnext = theap->tld->theaps;
  theap->tld->theaps = theap;
  return true;
}
*/


/* -----------------------------------------------------------
  Visit all theap blocks and areas
  Todo: enable visiting abandoned pages, and
        enable visiting all blocks of all theaps across threads
----------------------------------------------------------- */

void _mi_heap_area_init(mi_heap_area_t* area, mi_page_t* page) {
  const size_t bsize = mi_page_block_size(page);
  const size_t ubsize = mi_page_usable_block_size(page);
  area->reserved = page->reserved * bsize;
  area->committed = page->capacity * bsize;
  area->blocks = mi_page_start(page);
  area->used = page->used;   // number of blocks in use (#553)
  area->block_size = ubsize;
  area->full_block_size = bsize;
  area->reserved1 = page;
}

static void mi_get_fast_divisor(size_t divisor, uint64_t* magic, size_t* shift) {
  mi_assert_internal(divisor > 0 && divisor <= UINT32_MAX);
  *shift = MI_SIZE_BITS - mi_clz(divisor - 1);
  *magic = ((((uint64_t)1 << 32) * (((uint64_t)1 << *shift) - divisor)) / divisor + 1);
}

static size_t mi_fast_divide(size_t n, uint64_t magic, size_t shift) {
  mi_assert_internal(n <= UINT32_MAX);
  const uint64_t hi = ((uint64_t)n * magic) >> 32;
  return (size_t)((hi + n) >> shift);
}

bool _mi_theap_area_visit_blocks(const mi_heap_area_t* area, mi_page_t* page, mi_block_visit_fun* visitor, void* arg) {
  mi_assert(area != NULL);
  if (area==NULL) return true;
  mi_assert(page != NULL);
  if (page == NULL) return true;

  _mi_page_free_collect(page,true);              // collect both thread_delayed and local_free
  mi_assert_internal(page->local_free == NULL);
  if (page->used == 0) return true;

  size_t psize;
  uint8_t* const pstart = mi_page_area(page, &psize);
  mi_heap_t* const heap = mi_page_heap(page);
  const size_t bsize    = mi_page_block_size(page);
  const size_t ubsize   = mi_page_usable_block_size(page); // without padding

  // optimize page with one block
  if (page->capacity == 1) {
    mi_assert_internal(page->used == 1 && page->free == NULL);
    return visitor(heap, area, pstart, ubsize, arg);
  }
  mi_assert(bsize <= UINT32_MAX);

  // optimize full pages
  if (page->used == page->capacity) {
    uint8_t* block = pstart;
    for (size_t i = 0; i < page->capacity; i++) {
      if (!visitor(heap, area, block, ubsize, arg)) return false;
      block += bsize;
    }
    return true;
  }

  // create a bitmap of free blocks.
  #define MI_MAX_BLOCKS   (MI_SMALL_PAGE_SIZE / sizeof(void*))
  uintptr_t free_map[MI_MAX_BLOCKS / MI_INTPTR_BITS];
  const uintptr_t bmapsize = _mi_divide_up(page->capacity, MI_INTPTR_BITS);
  memset(free_map, 0, bmapsize * sizeof(intptr_t));
  if (page->capacity % MI_INTPTR_BITS != 0) {
    // mark left-over bits at the end as free
    size_t shift   = (page->capacity % MI_INTPTR_BITS);
    uintptr_t mask = (UINTPTR_MAX << shift);
    free_map[bmapsize - 1] = mask;
  }

  // fast repeated division by the block size
  uint64_t magic;
  size_t   shift;
  mi_get_fast_divisor(bsize, &magic, &shift);

  #if MI_DEBUG>1
  size_t free_count = 0;
  #endif
  for (mi_block_t* block = page->free; block != NULL; block = mi_block_next(page, block)) {
    #if MI_DEBUG>1
    free_count++;
    #endif
    mi_assert_internal((uint8_t*)block >= pstart && (uint8_t*)block < (pstart + psize));
    size_t offset = (uint8_t*)block - pstart;
    mi_assert_internal(offset % bsize == 0);
    mi_assert_internal(offset <= UINT32_MAX);
    size_t blockidx = mi_fast_divide(offset, magic, shift);
    mi_assert_internal(blockidx == offset / bsize);
    mi_assert_internal(blockidx < MI_MAX_BLOCKS);
    size_t bitidx = (blockidx / MI_INTPTR_BITS);
    size_t bit = blockidx - (bitidx * MI_INTPTR_BITS);
    free_map[bitidx] |= ((uintptr_t)1 << bit);
  }
  mi_assert_internal(page->capacity == (free_count + page->used));

  // walk through all blocks skipping the free ones
  #if MI_DEBUG>1
  size_t used_count = 0;
  #endif
  uint8_t* block = pstart;
  for (size_t i = 0; i < bmapsize; i++) {
    if (free_map[i] == 0) {
      // every block is in use
      for (size_t j = 0; j < MI_INTPTR_BITS; j++) {
        #if MI_DEBUG>1
        used_count++;
        #endif
        if (!visitor(heap, area, block, ubsize, arg)) return false;
        block += bsize;
      }
    }
    else {
      // visit the used blocks in the mask
      uintptr_t m = ~free_map[i];
      while (m != 0) {
        #if MI_DEBUG>1
        used_count++;
        #endif
        size_t bitidx = mi_ctz(m);
        if (!visitor(heap, area, block + (bitidx * bsize), ubsize, arg)) return false;
        m &= m - 1;  // clear least significant bit
      }
      block += bsize * MI_INTPTR_BITS;
    }
  }
  mi_assert_internal(page->used == used_count);
  return true;
}



// Separate struct to keep `mi_page_t` out of the public interface
typedef struct mi_theap_area_ex_s {
  mi_heap_area_t area;
  mi_page_t* page;
} mi_theap_area_ex_t;

typedef bool (mi_theap_area_visit_fun)(const mi_theap_t* theap, const mi_theap_area_ex_t* area, void* arg);

static bool mi_theap_visit_areas_page(mi_theap_t* theap, mi_page_queue_t* pq, mi_page_t* page, void* vfun, void* arg) {
  MI_UNUSED(theap);
  MI_UNUSED(pq);
  mi_theap_area_visit_fun* fun = (mi_theap_area_visit_fun*)vfun;
  mi_theap_area_ex_t xarea;
  xarea.page = page;
  _mi_heap_area_init(&xarea.area, page);
  return fun(theap, &xarea, arg);
}

// Visit all theap pages as areas
static bool mi_theap_visit_areas(const mi_theap_t* theap, mi_theap_area_visit_fun* visitor, void* arg) {
  if (visitor == NULL) return false;
  return mi_theap_visit_pages((mi_theap_t*)theap, &mi_theap_visit_areas_page, true, (void*)(visitor), arg); // note: function pointer to void* :-{
}

// Just to pass arguments
typedef struct mi_visit_blocks_args_s {
  bool  visit_blocks;
  mi_block_visit_fun* visitor;
  void* arg;
} mi_visit_blocks_args_t;

static bool mi_theap_area_visitor(const mi_theap_t* theap, const mi_theap_area_ex_t* xarea, void* arg) {
  mi_visit_blocks_args_t* args = (mi_visit_blocks_args_t*)arg;
  if (!args->visitor(_mi_theap_heap(theap), &xarea->area, NULL, xarea->area.block_size, args->arg)) return false;
  if (args->visit_blocks) {
    return _mi_theap_area_visit_blocks(&xarea->area, xarea->page, args->visitor, args->arg);
  }
  else {
    return true;
  }
}

// Visit all blocks in a theap
bool mi_theap_visit_blocks(const mi_theap_t* theap, bool visit_blocks, mi_block_visit_fun* visitor, void* arg) {
  mi_visit_blocks_args_t args = { visit_blocks, visitor, arg };
  return mi_theap_visit_areas(theap, &mi_theap_area_visitor, &args);
}

