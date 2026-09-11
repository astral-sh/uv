/* ----------------------------------------------------------------------------
Copyright (c) 2019-2026, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/

/* ----------------------------------------------------------------------------
Implement dynamic thread local variables (for heap's).
Unlike most OS native implementations there is no limit on the number
that can be allocated.
-----------------------------------------------------------------------------*/

#include "mimalloc.h"
#include "mimalloc/internal.h"
#include "mimalloc/prim.h"

/* -----------------------------------------------------------
  Each thread can have (a dynamically expanding) array of
  thread-local values. Each slot has a value and a version.
  The version is used to safely reuse slots.
----------------------------------------------------------- */
typedef struct mi_tls_slot_s {
  size_t  version;
  void*   value;
} mi_tls_slot_t;

typedef struct mi_thread_locals_s {
  size_t        count;
  mi_tls_slot_t slots[1];
} mi_thread_locals_t;

static mi_thread_locals_t mi_thread_locals_empty = { 0, {{0,NULL}} };

mi_decl_thread mi_thread_locals_t* mi_thread_locals = &mi_thread_locals_empty;  // always point to a valid `mi_thread_locals_t`


/* -----------------------------------------------------------
  Each key consists of the slot index in the lower bits,
  and its version it the top bits. When we get a value
  the version must match or we return NULL. When we set
  a value, we also set the version of the key.
----------------------------------------------------------- */

#if MI_SIZE_BITS < 64
#define MI_TLS_IDX_BITS     (MI_SIZE_BITS/2)      // half for the index, half for the version
#else
#define MI_TLS_IDX_BITS     (MI_SIZE_BITS/4)      // 16 bits for the index, 48 bits for the version
#endif
#define MI_TLS_IDX_MASK     ((MI_ZU(1)<<MI_TLS_IDX_BITS)-1)
#define MI_TLS_IDX_MAX      MI_TLS_IDX_MASK
#define MI_TLS_VERSION_MAX  ((MI_ZU(1)<<(MI_SIZE_BITS - MI_TLS_IDX_BITS))-1)

static size_t mi_key_index( size_t key ) {
  return (key & MI_TLS_IDX_MASK);
}

static size_t mi_key_version( size_t key ) {
  return (key >> MI_TLS_IDX_BITS);
}

static mi_thread_local_t mi_key_create( size_t index, size_t version ) {
  mi_assert_internal(version != 0 && version <= MI_TLS_VERSION_MAX);
  mi_assert_internal(index <= MI_TLS_IDX_MAX);
  const mi_thread_local_t key = ((version << MI_TLS_IDX_BITS) | index);
  mi_assert_internal(key != 0);
  return key;
}


// dynamically reallocate the thread local slots when needed
static mi_thread_locals_t* mi_thread_locals_expand(size_t least_idx) {
  mi_thread_locals_t* tls_old = mi_thread_locals;
  const size_t count_old = tls_old->count;
  size_t count;
  if (count_old==0) {
    tls_old = NULL; // so we allocate fresh from mi_thread_locals_empty
    count = 16;     // start with 16 slots
  } 
  else if (count_old >= 1024) {
    count = count_old + 1024;  // at some point increase linearly
  }
  else {
    count = 2*count_old;       // and double initially
  }
  if (count <= least_idx) {   
    count = least_idx + 1;
  }
  if (count > MI_TLS_IDX_MAX) { return NULL; }  // too large
  mi_thread_locals_t* tls = (mi_thread_locals_t*)mi_rezalloc(tls_old, sizeof(mi_thread_locals_t) + count*sizeof(mi_tls_slot_t));
  if mi_unlikely(tls==NULL) return NULL;
  tls->count = count;
  mi_thread_locals = tls;
  return tls;
}

static mi_decl_noinline bool mi_thread_local_set_expand( mi_thread_local_t key, void* val ) {
  if (val==NULL) return true;
  const size_t idx = mi_key_index(key);  
  mi_thread_locals_t* tls = mi_thread_locals_expand(idx);
  if (tls==NULL) return false;
  mi_assert_internal(tls == mi_thread_locals);
  mi_assert_internal(idx < tls->count);
  tls->slots[idx].value = val;
  tls->slots[idx].version = mi_key_version(key);
  return true;
}

// set a tls slot; returns `true` if successful.
// Can return `false` if we could not reallocate the slots array.
bool _mi_thread_local_set( mi_thread_local_t key, void* val ) {
  mi_thread_locals_t* tls = mi_thread_locals;
  mi_assert_internal(tls!=NULL);
  mi_assert_internal(key!=0);
  const size_t idx = mi_key_index(key);
  if mi_likely(idx < tls->count) {
    tls->slots[idx].value = val;
    tls->slots[idx].version = mi_key_version(key);
    return true;
  }
  else {
    return mi_thread_local_set_expand( key, val );  // tailcall
  }
}

// get a tls slot value
void* _mi_thread_local_get( mi_thread_local_t key ) {
  const mi_thread_locals_t* const tls = mi_thread_locals;
  mi_assert_internal(tls!=NULL);
  mi_assert_internal(key!=0);
  const size_t idx = mi_key_index(key);
  if mi_likely(idx < tls->count && mi_key_version(key) == tls->slots[idx].version) {
    return tls->slots[idx].value;
  }
  else {
    return NULL;  
  }
}

void _mi_thread_locals_thread_done(void) {
  mi_thread_locals_t* const tls = mi_thread_locals;
  if (tls!=NULL && tls->count > 0) {
    mi_free(tls);
    mi_thread_locals = &mi_thread_locals_empty;
  }
}

/* -----------------------------------------------------------
Create and free fresh TLS key's
----------------------------------------------------------- */
#include "bitmap.h"

static mi_lock_t    mi_thread_locals_lock;    // we need a lock in order to re-allocate the slot bits
static mi_bitmap_t* mi_thread_locals_free;    // reuse an arena bitmap to track which slots were assigned (1=free, 0=in-use)
static size_t       mi_thread_locals_version; // version to be able to reuse slots safely

void _mi_thread_locals_init(void) {
  mi_lock_init(&mi_thread_locals_lock);
}

void _mi_thread_locals_done(void) {
  mi_lock(&mi_thread_locals_lock) {
    mi_bitmap_t* const slots = mi_thread_locals_free;
    mi_free(slots);
  }
  mi_lock_done(&mi_thread_locals_lock);
}

// strange signature but allows us to reuse the arena code for claiming free pages
static bool mi_thread_local_claim_fun(size_t _slice_index, mi_arena_t* _arena, bool* keep_set) {
  MI_UNUSED(_slice_index); MI_UNUSED(_arena);
  *keep_set = false;
  return true;
}

// When we claim a free slot, we increase the global version counter 
// (so if we reuse a slot it will be returning NULL initially when a thread tries to get it)
static mi_thread_local_t mi_thread_local_claim(void) {
  size_t idx = 0;
  if (mi_thread_locals_free != NULL && mi_bitmap_try_find_and_claim(mi_thread_locals_free,0,&idx,&mi_thread_local_claim_fun,NULL)) {
    mi_thread_locals_version++;
    if (mi_thread_locals_version >= MI_TLS_VERSION_MAX) { mi_thread_locals_version = 1; }  /* wrap around the version */
    return mi_key_create( idx, mi_thread_locals_version);
  }
  else {
    return 0;
  }
}

static bool mi_thread_local_create_expand(void) {
  mi_bitmap_t* const slots = mi_thread_locals_free;
  // 1024 bits at a time
  const size_t oldcount = (slots==NULL ? 0 : mi_bitmap_max_bits(slots));
  const size_t newcount = 1024 + oldcount;
  if (newcount > MI_TLS_IDX_MAX) { return false; }
  const size_t newsize = mi_bitmap_size( newcount, NULL );
  mi_bitmap_t* newslots = (mi_bitmap_t*)mi_zalloc_aligned(newsize, MI_BCHUNK_SIZE);
  if (newslots==NULL) { return false; }
  if (slots!=NULL) {
    // copy over the previous bitmap
    _mi_memcpy_aligned(newslots, slots, mi_bitmap_size(oldcount, NULL)); 
    mi_free(slots);
  }
  mi_bitmap_init(newslots, newcount, true /* pretend already zero'd so we do not zero out the copied old entries */);
  mi_bitmap_unsafe_setN(newslots, oldcount, newcount - oldcount);  /* set the new expanded slots as available */
  mi_thread_locals_free = newslots;
  return true;
}


// create a fresh key
mi_thread_local_t _mi_thread_local_create(void) {
  mi_thread_local_t key = 0;
  mi_lock(&mi_thread_locals_lock) {
    key = mi_thread_local_claim();
    if (key==0) {
      if (mi_thread_local_create_expand()) {
        key = mi_thread_local_claim();
      }
    }
  }
  return key;
}

// free a key
void _mi_thread_local_free(mi_thread_local_t key) {
  if (key==0) return;
  const size_t idx = mi_key_index(key);
  mi_lock(&mi_thread_locals_lock) {
    mi_bitmap_t* const slots = mi_thread_locals_free;
    if (slots!=NULL && idx < mi_bitmap_max_bits(slots)) {
      mi_bitmap_set(slots,idx);
    }
  }
}

