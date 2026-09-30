# Platform

JARVIG is a native multi-platform engine. Browser execution is an export target. Browser technology must never become an implicit dependency of engine core.

Targets the engine is being shaped for:

| Target | Host | Renderer |
| --- | --- | --- |
| Windows player | native executable | RHI, first backend wgpu |
| Windows editor | `JARVIGEditor.exe` (`native/jarvig_editor`). `jarvig_editor_host` remains the two-view test | RHI, first GPU backend is private wgpu |
| Dedicated server | native executable | none |
| Web | WASM build of the same core, plus a browser platform layer | browser WebGPU |
| Linux, macOS | same native core, platform module differs | RHI |
| Consoles | not scheduled. The C ABI exists so a C++ platform module can exist later | explicit backend, not WebGPU |

A simple project links the modules it uses. Core, world, render, physics, audio, animation, AI, and networking are separable. The server does not link render. The player does not link the editor.

See [hosts.md](hosts.md), [modules.md](modules.md), and [plugin-abi.md](plugin-abi.md).
