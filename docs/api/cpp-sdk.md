# C++ SDK

The C++ SDK is a wrapper. The binary boundary underneath is [c-abi.md](c-abi.md).

Game code should be able to read:

```text
Jarvig::Entity player = world.CreateEntity("Player");
player.SetTransform(transform);
```

instead of filling `JarvigEntityHandle` at every call. Both forms call the same C function. The wrapper is allowed to:

- turn `JarvigResult` into a C++ error type inside the wrapper's own translation unit
- tie a handle's lifetime to a C++ object's destructor, which calls the engine release function
- offer typed component access generated from the type registry

The wrapper is not allowed to:

- throw across the ABI
- store a decoded engine pointer
- be the thing a plugin DLL exports
- depend on a particular standard-library ABI in its public headers if that layout would leak into the DLL boundary

`MyGame.dll` exports the C function table in [game-module.md](game-module.md). A class such as `Jarvig::GameModule` can exist as a suggestion in a header that the game compiles, with the exported C functions forwarding into it. The engine does not load that class. It loads the table.

Headers ship as source with the SDK. They are regenerated from the canonical API description when generation exists. Until then, a handwritten wrapper is allowed only as a thin layer over a C header, and it is deleted when the generator covers it. Do not maintain a third handwritten copy in another language.

No C++ SDK library is in the tree today.
