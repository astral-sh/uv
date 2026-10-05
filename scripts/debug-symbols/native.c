#include <stdint.h>

#ifdef _MSC_VER
__declspec(noinline)
#else
__attribute__((noinline))
#endif
uint32_t native_frame(uint32_t value) {
    return value * 3 + 1;
}
