# Cooking and packaging

```text
jarvig validate project
jarvig import --changed
jarvig cook --target webgpu-web
jarvig cook --target windows-desktop
jarvig cook --target dedicated-server
jarvig package --target windows-desktop --configuration shipping
jarvig test --suite smoke
jarvig test --suite perf
```

Those commands are the founding CLI contract. Phase 0 implements `jarvig info`, `jarvig version`, and `jarvig project validate`. The rest are not implemented. Do not alias `project validate` to a cook.

| Mode | Purpose |
| --- | --- |
| Debug | Assertions, symbols, validation, debug UI |
| Development | Profiling, hot reload, optimization |
| Test | Automation hooks and deterministic capture |
| Shipping | Editor and debug stripped, cooked assets, optimized runtime |

Server cooks omit unneeded render and audio payloads. A clean checkout reproducing a packaged sample is the P4 exit (JRV-0023 is the cooker skeleton: client and server outputs).

Cooked data is not committed. Golden fixtures under `tests/` are the exception.
