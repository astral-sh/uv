#include <stdint.h>

#ifdef _MSC_VER
__declspec(noinline)
#else
__attribute__((noinline))
#endif
uint64_t native_answer(uint64_t input) {
    return input + 42;
}
