# Errors

The stable boundary returns `JarvigResult`. It is a `uint32_t` code. It is not a Rust `Result` and not a C++ exception.

Initial codes, append-only:

| Code | Meaning |
| --- | --- |
| `JARVIG_OK` | The call did what it said |
| `JARVIG_INVALID_ARGUMENT` | A pointer, size, or field is illegal |
| `JARVIG_INVALID_HANDLE` | The handle is null, stale, or the wrong kind |
| `JARVIG_NOT_FOUND` | The named object is not there |
| `JARVIG_UNSUPPORTED` | The engine refuses this operation on purpose |
| `JARVIG_OUT_OF_MEMORY` | Allocation failed |
| `JARVIG_DEVICE_LOST` | The GPU device is gone |
| `JARVIG_VERSION_MISMATCH` | The caller and the engine disagree on a version |
| `JARVIG_WRONG_THREAD` | The call is not legal on this thread |
| `JARVIG_BUSY` | An async request is still running |
| `JARVIG_FAILED` | Everything else, with a log line |

`0` is success. Do not invent a second success value. New codes are added at the end. Existing numeric values do not change.

On any code other than `JARVIG_OK`:

- Out-parameters are unchanged.
- The engine writes a diagnostic through its log, with the code and enough context to find the call. The log is the detailed text. The code is the contract.
- No heap string is returned unless the function's ownership rules say so.

A panic is not an error code. The ABI edge catches it, logs it, and returns `JARVIG_FAILED`. Unwinding into C or C++ is a defect.

A C++ exception thrown in the SDK wrapper stops at the wrapper. It is not thrown from an `extern "C"` function.

The bootstrap header still uses `0`, `-1`, and `-2`. That is legacy. New functions use this table.

Internal Rust code keeps `Result` and the existing `RhiError`. Translation to `JarvigResult` happens at the ABI edge, not in the middle of a crate.
