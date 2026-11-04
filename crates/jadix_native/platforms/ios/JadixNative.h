#pragma once
#include <stdbool.h>
#include <stdint.h>

typedef bool (*JadixNativeMeasure)(void *context, const char *json, double *size);
typedef bool (*JadixNativePublish)(void *context, const char *json);
typedef void (*JadixNativeClear)(void *context);

uint64_t jadix_native_create(void *context, JadixNativeMeasure measure, JadixNativePublish publish,
                            JadixNativeClear clear, float width, float height);
char *jadix_native_event(uint64_t root, const char *json);
char *jadix_native_resize(uint64_t root, float width, float height, uint64_t environment);
char *jadix_native_state(uint64_t root);
char *jadix_native_restore(uint64_t root, const char *json);
char *jadix_native_storage_result(uint64_t root, const char *error);
void jadix_native_dispose(uint64_t root);
void jadix_native_string_free(char *value);
