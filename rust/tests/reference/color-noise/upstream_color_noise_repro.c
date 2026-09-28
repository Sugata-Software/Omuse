#include "AdjustPixels.h"

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void *repro_malloc(size_t size) {
    void *memory = malloc(size);
    if (!memory) return NULL;
    const char *value = getenv("REPRO_FILL_BYTE");
    unsigned long fill = value ? strtoul(value, NULL, 0) : 0xa5;
    memset(memory, (int)(fill & 0xff), size);
    return memory;
}

void repro_free(void *memory) { free(memory); }

static uint64_t hash_bytes(const uint8_t *bytes, size_t count) {
    uint64_t hash = UINT64_C(1469598103934665603);
    for (size_t i = 0; i < count; ++i) {
        hash ^= bytes[i];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static int run_case(const char *name) {
    enum { width = 5, height = 3, bytes = width * height * 4 };
    uint8_t pixels[bytes];
    memset(pixels, 0, sizeof(pixels));

    if (strcmp(name, "mixed") == 0) {
        const uint8_t input[bytes] = {
              0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
              0,   0,   0,   0, 220,  40,  20, 255,  80, 150,  40, 255,  20,  60, 210, 255,   0,   0,   0,   0,
              0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
        };
        memcpy(pixels, input, sizeof(input));
    } else if (strcmp(name, "opaque") == 0) {
        for (size_t i = 0; i < width * height; ++i) {
            pixels[i * 4 + 0] = (uint8_t)(30 + i * 11);
            pixels[i * 4 + 1] = (uint8_t)(210 - i * 7);
            pixels[i * 4 + 2] = (uint8_t)(50 + i * 5);
            pixels[i * 4 + 3] = 255;
        }
    } else if (strcmp(name, "transparent") != 0) {
        fprintf(stderr, "unknown case: %s\n", name);
        return 2;
    }

    uint8_t before[bytes];
    memcpy(before, pixels, sizeof(before));
    adjust_camera_raw_detail(pixels, width, height, width * 4,
                             0, 0, 0, 0,
                             0, 0, 0,
                             80, 50, 50, 1);

    for (size_t i = 0; i < width * height; ++i) {
        if (pixels[i * 4 + 3] != before[i * 4 + 3]) {
            fprintf(stderr, "%s: alpha changed at pixel %zu\n", name, i);
            return 1;
        }
        if (before[i * 4 + 3] == 0 &&
            memcmp(pixels + i * 4, before + i * 4, 4) != 0) {
            fprintf(stderr, "%s: transparent pixel changed at pixel %zu\n", name, i);
            return 1;
        }
    }
    if (strcmp(name, "transparent") == 0 && memcmp(pixels, before, bytes) != 0) {
        fprintf(stderr, "transparent: transparent-only image changed\n");
        return 1;
    }

    printf("%s %016llx", name, (unsigned long long)hash_bytes(pixels, sizeof(pixels)));
    for (size_t i = 0; i < width * height; ++i) {
        if (pixels[i * 4 + 3] != 0)
            printf(" %u,%u,%u,%u", pixels[i * 4], pixels[i * 4 + 1],
                   pixels[i * 4 + 2], pixels[i * 4 + 3]);
    }
    putchar('\n');
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s transparent|mixed|opaque\n", argv[0]);
        return 2;
    }
    return run_case(argv[1]);
}
