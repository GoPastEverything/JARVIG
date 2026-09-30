# Memory, strings, and arrays

Ownership at the ABI is explicit. If a function does not say, it is wrong.

## Rules

| Kind | Meaning |
| --- | --- |
| Caller-owned input | The pointer is valid only for the call. The engine copies what it needs to keep. |
| Caller-provided output | The caller passes a buffer and a capacity. The engine writes into it and reports the count. |
| Engine-owned view | The pointer is valid until the next mutating call on that object, or until a documented lifetime ends. The caller copies it if it must keep the bytes. |
| Explicit release | The engine allocated. The caller later calls the matching `jarvig_*_release`. |

The caller never `free`s memory the engine allocated. The engine never `free`s memory the plugin allocated. If ownership must move, the release function is in the same module that allocated, and the header names it.

Allocators are not mixed. A plugin that needs engine memory asks the engine. It does not pass a function pointer to a different CRT heap and expect the engine to call it, unless a future allocator ticket defines that on purpose.

## Strings

UTF-8. The length is the authority. The bytes are not required to be null-terminated. No `std::string`. No Rust `String`. No `wchar_t` as the canonical encoding.

```text
JarvigString
    const char *data
    uint32_t    length
```

A convenience that also writes a trailing NUL into a caller buffer is allowed. The length still does not include that NUL, so an embedded NUL is data, not a terminator.

## Arrays

```text
JarvigBytes
    const void *data
    uint32_t    length

JarvigSlice
    const void *data
    uint32_t    count
    uint32_t    stride
```

`stride` is the byte step between elements. It lets a newer struct be read by an older caller only when the versioning rule for that struct allows it. Do not pass a `Vec<T>` or a C++ vector across the boundary. The SDK wrapper may build one on its own side after the call returns.
