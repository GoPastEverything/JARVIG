#ifndef JARVIG_CORE_H
#define JARVIG_CORE_H

/*
 * BOOTSTRAP / INTERNAL TRANSITIONAL ABI
 * NOT THE COMPLETE PUBLIC JARVIG SDK
 *
 * This header is the clock and headless runtime used by the prototype hosts.
 * It is not sdk/c/include/jarvig/. Do not add world, mesh, asset, or material
 * functions here. The public SDK layout and the calling-convention macros are
 * described in docs/api/c-abi.md. ADR-0022 and ADR-0023.
 *
 * Return codes in this file are the legacy 0 / -1 / -2 style. New public C
 * functions, when a ticket adds them under the SDK tree, use JarvigResult.
 */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct JarvigClock JarvigClock;

typedef struct JarvigClockAdvance {
    uint32_t fixed_steps;
    double alpha;
    double fixed_delta;
    double frame_delta;
    int clamped;
} JarvigClockAdvance;

/* Returns null if fixed_hz is not positive or max_steps is 0. */
JarvigClock *jarvig_clock_create(double fixed_hz, uint32_t max_steps);
void jarvig_clock_destroy(JarvigClock *clock);

/* Returns 0 on success, -1 on a null pointer or a non-finite delta. */
int jarvig_clock_advance(JarvigClock *clock, double frame_delta_seconds, JarvigClockAdvance *out);

/* Profiles. The numeric values are the ABI. */
#define JARVIG_PROFILE_EDITOR 1u
#define JARVIG_PROFILE_CLIENT 2u
#define JARVIG_PROFILE_SERVER 3u
#define JARVIG_PROFILE_TEST 4u

typedef struct JarvigRuntime JarvigRuntime;

typedef struct JarvigRuntimeTick {
    uint64_t frame;
    uint32_t fixed_steps;
    int render_executed;
    int clamped;
} JarvigRuntimeTick;

/* Returns null on a bad profile or clock configuration. */
JarvigRuntime *jarvig_runtime_create(uint32_t profile, double fixed_hz, uint32_t max_steps);

/* 0 success, -1 bad pointer or non-finite delta, -2 the runtime is shut down. */
int jarvig_runtime_tick(JarvigRuntime *runtime, double frame_delta_seconds, JarvigRuntimeTick *out);

/* 0 success, -1 bad pointer, -2 already shut down. */
int jarvig_runtime_shutdown(JarvigRuntime *runtime);
void jarvig_runtime_destroy(JarvigRuntime *runtime);

#ifdef __cplusplus
}
#endif

#endif
