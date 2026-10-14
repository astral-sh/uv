#include <stdint.h>

extern void capability_checkpoint(uint64_t marker);

struct native_record {
    uint64_t count;
    uint64_t tag;
};

#ifdef _MSC_VER
__declspec(noinline)
#else
__attribute__((noinline))
#endif
uint64_t capability_native(uint64_t seed) {
    const uint64_t native_local = seed * 3 + 1;
    struct native_record record = {native_local, seed + 9};
    capability_checkpoint(9);
    return record.count + record.tag;
}
