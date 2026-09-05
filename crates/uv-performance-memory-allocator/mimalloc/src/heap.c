/*----------------------------------------------------------------------------
Copyright (c) 2018-2025, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/

#include "mimalloc.h"
#include "mimalloc/internal.h"
#include "mimalloc/prim.h"  // _mi_theap_default


/* -----------------------------------------------------------
  Heap's
----------------------------------------------------------- */

mi_theap_t* mi_heap_theap(mi_heap_t* heap) {
  return _mi_heap_theap(heap);
}

void mi_heap_set_numa_affinity(mi_heap_t* heap, int numa_node) {
  if (heap==NULL) { heap = mi_heap_main(); }
  heap->numa_node = (numa_node < 0 ? -1 : numa_node % _mi_os_numa_node_count());
}

void mi_heap_stats_merge_to_subproc(mi_heap_t* heap) {
  if (heap==NULL) { heap = mi_heap_main(); }
  _mi_stats_merge_into(&heap->subproc->stats, &heap->stats);
}

void mi_heap_stats_merge_to_main(mi_heap_t* heap) {
  if (heap==NULL) return;
  _mi_stats_merge_into(&mi_heap_main()->stats, &heap->stats);
}

static mi_decl_noinline mi_theap_t* mi_heap_init_theap(const mi_heap_t* const_heap)
{
  mi_heap_t* heap = (mi_heap_t*)const_heap;
  mi_assert_internal(heap!=NULL);

  if (_mi_is_heap_main(heap)) {
    // this can be called if the (main) thread is not yet initialized (as no allocation happened)
    // but `theap_main_init_get()` will call `mi_thread_init()`
    mi_theap_t* const theap = _mi_theap_main_safe();
    mi_assert_internal(theap!=NULL && _mi_is_heap_main(_mi_theap_heap(theap)));
    return theap;
  }

  // otherwise initialize the theap for this heap
  // get the thread local
  mi_assert_internal(heap->theap != 0);
  if (heap->theap==0) {  // paranoia
    _mi_error_message(EFAULT, "no thread-local reserved for heap (%p)\n", heap);
    return NULL;
  }
  mi_theap_t* theap = (mi_theap_t*)_mi_thread_local_get(heap->theap);

  // create a fresh theap?
  if (theap==NULL) {
    // set first an invalid value to ensure the thread local storage is allocated
    if (!_mi_thread_local_set(heap->theap, (mi_theap_t*)1)) {
      _mi_error_message(EFAULT, "unable to allocate memory for thread local storage\n");
      return NULL;
    }    
    // then allocate the theap
    theap = _mi_theap_create(heap, _mi_theap_default_safe()->tld);
    _mi_thread_local_set(heap->theap, theap);  // Cannot fail now as it was set before. Always set so the local is valid or NULL (and not 1)
    if (theap==NULL) {
      _mi_error_message(EFAULT, "unable to allocate memory for a thread local heap\n");
      return NULL;
    }    
  }
  return theap;
}


// get the theap for a heap without initializing (and return NULL in that case)
mi_theap_t* _mi_heap_theap_get_peek(const mi_heap_t* heap) {
  if (heap==NULL || _mi_is_heap_main(heap)) {
    return _mi_theap_main_safe(); 
  }
  else {
    return (mi_theap_t*)_mi_thread_local_get(heap->theap);
  }
}

// get (and possibly create) the theap belonging to a heap
mi_theap_t* _mi_heap_theap_get_or_init(const mi_heap_t* heap)
{
  mi_theap_t* theap = _mi_heap_theap_peek(heap);
  if mi_unlikely(theap==NULL) {
    theap = mi_heap_init_theap(heap);
    if (theap==NULL) { return (mi_theap_t*)&_mi_theap_empty_wrong; }  // this will return NULL from page.c:_mi_malloc_generic
  }
  _mi_theap_cached_set(theap);
  return theap;
}


mi_heap_t* mi_heap_new_in_arena(mi_arena_id_t exclusive_arena_id) {
  // always allocate heap data in the (subprocess) main heap
  mi_heap_t* const heap_main = mi_heap_main();
  // todo: allocate heap data in the exclusive arena ?
  mi_heap_t* const heap = (mi_heap_t*)mi_heap_zalloc( heap_main, sizeof(mi_heap_t) );
  if (heap==NULL) return NULL;

  // reserve a thread local slot for this heap (see also issue #1230)
  const mi_thread_local_t theap_slot = _mi_thread_local_create();
  if (theap_slot == 0) {
    _mi_error_message(EFAULT, "unable to dynamically create a thread local for a heap\n");
    mi_free(heap);
    return NULL;
  }

  // init fields
  heap->theap = theap_slot;
  heap->subproc = heap_main->subproc;
  heap->heap_seq = mi_atomic_increment_relaxed(&heap_main->subproc->heap_total_count);
  heap->exclusive_arena = _mi_arena_from_id(exclusive_arena_id);
  heap->numa_node = -1; // no initial affinity
  mi_stats_header_init(&heap->stats);
  mi_lock_init(&heap->theaps_lock);
  mi_lock_init(&heap->os_abandoned_pages_lock);
  mi_lock_init(&heap->arena_pages_lock);

  // push onto the subproc heaps
  mi_lock(&heap->subproc->heaps_lock) {
    mi_heap_t* head = heap->subproc->heaps;
    heap->prev = NULL;
    heap->next = head;
    if (head!=NULL) { head->prev = heap;  }
    heap->subproc->heaps = heap;
  }
  mi_atomic_increment_relaxed(&heap_main->subproc->heap_count);
  mi_subproc_stat_increase(heap_main->subproc, heaps, 1);
  return heap;
}

mi_heap_t* mi_heap_new(void) {
  return mi_heap_new_in_arena(0);
}

// free all theaps belonging to this heap (without deleting their pages as we do this arena wise for efficiency)
static void mi_heap_free_theaps(mi_heap_t* heap) {
  // This can run concurrently with a thread that terminates (see `init.c:mi_thread_theaps_done`), 
  // and we need to ensure we free theaps atomically.
  // We do this in a loop where we release the theaps_lock at every potential re-iteration to unblock 
  // potential concurrent thread termination which tries to remove the theap from our theaps list.
  bool all_freed;
  do {
    all_freed = true;
    mi_theap_t* theap = NULL;
    mi_lock(&heap->theaps_lock) { 
      theap = heap->theaps; 
      while(theap != NULL) {
        mi_theap_t* next = theap->hnext;
        if (!_mi_theap_free(theap, false /* dont re-acquire the heap->theaps_lock */, true /* acquire the tld->theaps_lock though */ )) {
          all_freed = false;
        }
        theap = next;
      }      
    }
    if (!all_freed) { 
      mi_heap_stat_counter_increase(heap,heaps_delete_wait,1); 
      _mi_prim_thread_yield();
    }
    else { 
      mi_assert_internal(heap->theaps==NULL); 
    }               
  }
  while(!all_freed);
}

// free the heap resources (assuming the pages are already moved/destroyed, and all theaps have been freed)
static void mi_heap_free(mi_heap_t* heap) {
  mi_assert_internal(heap!=NULL && !_mi_is_heap_main(heap));

  // free all arena pages infos
  mi_lock(&heap->arena_pages_lock) {
    for (size_t i = 0; i < MI_MAX_ARENAS; i++) {
      mi_arena_pages_t* arena_pages = mi_atomic_load_ptr_relaxed(mi_arena_pages_t, &heap->arena_pages[i]);
      if (arena_pages!=NULL) {
        mi_atomic_store_ptr_relaxed(mi_arena_pages_t, &heap->arena_pages[i], NULL);
        mi_free(arena_pages);
      }
    }
  }

  // remove the heap from the subproc
  mi_heap_stats_merge_to_main(heap);
  mi_atomic_decrement_relaxed(&heap->subproc->heap_count);
  mi_subproc_stat_decrease(heap->subproc, heaps, 1);
  mi_lock(&heap->subproc->heaps_lock) {
    if (heap->next!=NULL) { heap->next->prev = heap->prev; }
    if (heap->prev!=NULL) { heap->prev->next = heap->next; }
                     else { heap->subproc->heaps = heap->next; }
  }

  _mi_thread_local_free(heap->theap);
  mi_lock_done(&heap->theaps_lock);
  mi_lock_done(&heap->os_abandoned_pages_lock);
  mi_lock_done(&heap->arena_pages_lock);
  mi_free(heap);
}

void mi_heap_delete(mi_heap_t* heap) {
  if (heap==NULL) return;
  if (_mi_is_heap_main(heap)) {
    _mi_warning_message("cannot delete the main heap\n");
    return;
  }
  mi_heap_free_theaps(heap);
  _mi_heap_move_pages(heap, mi_heap_main());
  mi_heap_free(heap);
}

void _mi_heap_force_destroy(mi_heap_t* heap) {
  if (heap==NULL) return;
  mi_heap_free_theaps(heap);
  _mi_heap_destroy_pages(heap);
  if (!_mi_is_heap_main(heap)) { mi_heap_free(heap); }  // todo: release locks of the main heap?
}

void mi_heap_destroy(mi_heap_t* heap) {
  if (heap==NULL) return;
  if (_mi_is_heap_main(heap)) {
    _mi_warning_message("cannot destroy the main heap\n");
    return;
  }
  _mi_heap_force_destroy(heap);
}

mi_heap_t* mi_heap_of(const void* p) {
  mi_page_t* page = _mi_safe_ptr_page(p);
  if (page==NULL) return NULL;
  return mi_page_heap(page);
}

bool mi_any_heap_contains(const void* p) {
  return (mi_heap_of(p)!=NULL);
}

bool mi_heap_contains(const mi_heap_t* heap, const void* p) {
  if (heap==NULL) { heap = mi_heap_main(); }
  return (heap==mi_heap_of(p));
}

// deprecated
bool mi_check_owned(const void* p) {
  return mi_any_heap_contains(p);
}

// unsafe heap utilization function for DragonFly (see issue #1258)
// If the page of pointer `p` belongs to `heap` (or `heap==NULL`) and has less than `perc_threshold` used blocks in its used area return `true`.
// This function is unsafe in general as it assumes we are the only thread accessing the page of `p`.
bool mi_unsafe_heap_page_is_under_utilized(mi_heap_t* heap, void* p, size_t perc_threshold) mi_attr_noexcept {
  if (p==NULL) return false;
  const mi_page_t* const page = _mi_safe_ptr_page(p);   // Get the page containing this pointer
  if (page==NULL || page->used==page->capacity || page->capacity < page->reserved) return false;
  // If the page is the head of the queue, it is currently being used for 
  // allocations; we skip it to avoid immediate thrashing.
  if (page->prev == NULL)  return false;

  // match heap?
  const mi_heap_t* const page_heap = mi_page_heap(page);
  if (page_heap==NULL) return false;
  if (heap!=NULL && page_heap!=heap) return false;
    
  // check utilization
  if (page->capacity==0)   return false;
  if (perc_threshold>=100) return true;
  return (perc_threshold >= ((100UL*page->used) / page->capacity));
}
