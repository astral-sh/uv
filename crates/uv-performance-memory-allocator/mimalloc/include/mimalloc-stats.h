/* ----------------------------------------------------------------------------
Copyright (c) 2024-2025, Microsoft Research, Daan Leijen
This is free software; you can redistribute it and/or modify it under the
terms of the MIT license. A copy of the license can be found in the file
"LICENSE" at the root of this distribution.
-----------------------------------------------------------------------------*/
#pragma once
#ifndef MIMALLOC_STATS_H
#define MIMALLOC_STATS_H

#include <mimalloc.h>
#include <string.h>   // memset
#include <stdint.h>   // int64_t

#define MI_STAT_VERSION   5  // increased on every backward incompatible change

// alignment for atomic fields
#if defined(_MSC_VER)
#define mi_decl_align(a)        __declspec(align(a))
#elif defined(__GNUC__)
#define mi_decl_align(a)        __attribute__((aligned(a)))
#elif __cplusplus >= 201103L
#define mi_decl_align(a)        alignas(a)
#else
#define mi_decl_align(a)
#endif


// count allocation over time
typedef struct mi_stat_count_s {
  int64_t total;                              // total allocated
  int64_t peak;                               // peak allocation
  int64_t current;                            // current allocation
} mi_stat_count_t;

// counters only increase
typedef struct mi_stat_counter_s {
  int64_t total;                              // total count
} mi_stat_counter_t;

#define MI_STAT_FIELDS() \
  MI_STAT_COUNT(pages)                      /* count of mimalloc pages */ \
  MI_STAT_COUNT(reserved)                   /* reserved memory bytes */ \
  MI_STAT_COUNT(committed)                  /* committed bytes */ \
  MI_STAT_COUNTER(reset)                    /* reset bytes */ \
  MI_STAT_COUNTER(purged)                   /* purged bytes */ \
  MI_STAT_COUNT(page_committed)             /* committed memory inside pages */ \
  MI_STAT_COUNT(pages_abandoned)            /* abandoned pages count */ \
  MI_STAT_COUNT(threads)                    /* number of threads */ \
  MI_STAT_COUNT(malloc_normal)              /* allocated bytes <= MI_LARGE_OBJ_SIZE_MAX */ \
  MI_STAT_COUNT(malloc_huge)                /* allocated bytes in huge pages */ \
  MI_STAT_COUNT(malloc_requested)           /* malloc requested bytes */ \
  \
  MI_STAT_COUNTER(mmap_calls) \
  MI_STAT_COUNTER(commit_calls) \
  MI_STAT_COUNTER(reset_calls) \
  MI_STAT_COUNTER(purge_calls) \
  MI_STAT_COUNTER(arena_count)              /* number of memory arena's */ \
  MI_STAT_COUNTER(malloc_normal_count)      /* number of blocks <= MI_LARGE_OBJ_SIZE_MAX */ \
  MI_STAT_COUNTER(malloc_huge_count)        /* number of huge bloks */ \
  MI_STAT_COUNTER(malloc_guarded_count)     /* number of allocations with guard pages */ \
  \
  /* internal statistics */ \
  MI_STAT_COUNTER(arena_rollback_count) \
  MI_STAT_COUNTER(arena_purges) \
  MI_STAT_COUNTER(pages_extended)           /* number of page extensions */ \
  MI_STAT_COUNTER(pages_retire)             /* number of pages that are retired */ \
  MI_STAT_COUNTER(page_searches)            /* total pages searched for a fresh page */ \
  MI_STAT_COUNTER(page_searches_count)      /* searched count for a fresh page */ \
  /* only on v1 and v2 */ \
  MI_STAT_COUNT(segments) \
  MI_STAT_COUNT(segments_abandoned) \
  MI_STAT_COUNT(segments_cache) \
  MI_STAT_COUNT(_segments_reserved) \
  /* only on v3 */ \
  MI_STAT_COUNT(heaps) \
  MI_STAT_COUNT(theaps) \
  MI_STAT_COUNTER(pages_reclaim_on_alloc) \
  MI_STAT_COUNTER(pages_reclaim_on_free) \
  MI_STAT_COUNTER(pages_reabandon_full) \
  MI_STAT_COUNTER(pages_unabandon_busy_wait) \
  MI_STAT_COUNTER(heaps_delete_wait)

// Size bins for chunks
typedef enum mi_chunkbin_e {
  MI_CBIN_SMALL,    // slice_count == 1
  MI_CBIN_OTHER,    // slice_count: any other from the other bins, and 1 <= slice_count <= MI_BCHUNK_BITS
  MI_CBIN_MEDIUM,   // slice_count == 8
  MI_CBIN_LARGE,    // slice_count == MI_SIZE_BITS  (only used if MI_ENABLE_LARGE_PAGES is 1)
  MI_CBIN_HUGE,     // slice_count > MI_BCHUNK_BITS
  MI_CBIN_NONE,     // no bin assigned yet (the chunk is completely free)
  MI_CBIN_COUNT
} mi_chunkbin_t;


// Define the statistics structure
#define MI_BIN_HUGE             (73U)   // see types.h
#define MI_STAT_COUNT(stat)     mi_stat_count_t stat;
#define MI_STAT_COUNTER(stat)   mi_stat_counter_t stat;

typedef struct mi_stats_s
{
  size_t size;          // size of the mi_stats_t structure 
  size_t version;       

  mi_decl_align(8)  MI_STAT_FIELDS()

  // future extension
  mi_stat_count_t   _stat_reserved[4];
  mi_stat_counter_t _stat_counter_reserved[4];

  // size segregated statistics
  mi_stat_count_t   malloc_bins[MI_BIN_HUGE+1];   // allocation per size bin
  mi_stat_count_t   page_bins[MI_BIN_HUGE+1];     // pages allocated per size bin
  mi_stat_count_t   chunk_bins[MI_CBIN_COUNT];    // chunks per page sizes
} mi_stats_t;

#undef MI_STAT_COUNT
#undef MI_STAT_COUNTER

// Initialization
static inline void mi_stats_header_init(mi_stats_t* stats) {
  stats->size = sizeof(*stats);
  stats->version = MI_STAT_VERSION;
}
static inline void mi_stats_init(mi_stats_t* stats) {
  memset(stats,0,sizeof(*stats));
  mi_stats_header_init(stats);
}

#define mi_stats_t_decl(name)  mi_stats_t name; mi_stats_init(&name);

// Exported definitions
#ifdef __cplusplus
extern "C" {
#endif

// stats from a heap
mi_decl_export bool    mi_heap_stats_get(mi_heap_t* heap, mi_stats_t* stats) mi_attr_noexcept;
mi_decl_export char*   mi_heap_stats_get_json(mi_heap_t* heap, size_t buf_size, char* buf) mi_attr_noexcept;      // use mi_free to free the result if the input buf == NULL
mi_decl_export void    mi_heap_stats_print_out(mi_heap_t* heap, mi_output_fun* out, void* arg) mi_attr_noexcept;

// stats from a subprocess and its heaps aggregated
mi_decl_export bool    mi_subproc_stats_get(mi_subproc_id_t subproc_id, mi_stats_t* stats) mi_attr_noexcept;
mi_decl_export char*   mi_subproc_stats_get_json(mi_subproc_id_t subproc_id, size_t buf_size, char* buf) mi_attr_noexcept;      // use mi_free to free the result if the input buf == NULL
mi_decl_export void    mi_subproc_stats_print_out(mi_subproc_id_t subproc_id, mi_output_fun* out, void* arg) mi_attr_noexcept;
// print subprocess and all its heap stats segregated
mi_decl_export void    mi_subproc_heap_stats_print_out(mi_subproc_id_t subproc_id, mi_output_fun* out, void* arg) mi_attr_noexcept;

// stats aggregated for the current subprocess and all its heaps.
mi_decl_export bool    mi_stats_get(mi_stats_t* stats) mi_attr_noexcept;
mi_decl_export char*   mi_stats_get_json(size_t buf_size, char* buf) mi_attr_noexcept;      // use mi_free to free the result if the input buf == NULL
mi_decl_export void    mi_stats_print_out(mi_output_fun* out, void* arg) mi_attr_noexcept;

// add the stats of the heap to the subprocess and clear the heap stats
mi_decl_export void    mi_heap_stats_merge_to_subproc(mi_heap_t* heap);

// stats from the subprocess without aggregating its heaps
mi_decl_export bool    mi_subproc_stats_get_exclusive(mi_subproc_id_t subproc_id, mi_stats_t* stats) mi_attr_noexcept;

mi_decl_export char*   mi_stats_as_json(mi_stats_t* stats, size_t buf_size, char* buf) mi_attr_noexcept;      // use mi_free to free the result if the input buf == NULL
mi_decl_export size_t  mi_stats_get_bin_size(size_t bin) mi_attr_noexcept;

#ifdef __cplusplus
}
#endif

#endif // MIMALLOC_STATS_H
