# JARVIGEditor

`JARVIGEditor.exe` is the native editor. It hosts the engine. It is not a second engine, not the player, and not the dedicated server.

The TypeScript dock is still available for the old command-bus tests. It is transitional. Do not wrap it in a WebView and call that the editor. `pnpm dev:hub` is not the product hub. The product hub is `JARVIG.exe`. See [hub.md](hub.md).

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor -- --self-test
cargo run --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor -- --project samples/lighting-lab/LightingLab.jarvigproject
```

A launch with no project shows the hub and does not open Lighting Lab. `--self-test` opens the editor, exercises the dock, flies the Perspective camera, presents, and exits. It does not open a project. `--project` skips the hub. RMB looks. WASD flies.

The two-view renderer test is still `jarvig_editor_host`. It is not this product.

| Doc | What it is |
| --- | --- |
| [architecture.md](architecture.md) | Who owns the window, the world, and the viewport |
| [hub.md](hub.md) | The project browser before the editor |
| [workspaces.md](workspaces.md) | The dock tree. JRV-0059 is accepted |
| [viewport.md](viewport.md) | The center child is one engine `RenderView` |
| [camera.md](camera.md) | Perspective fly camera. Not a scene entity. Accepted |
| [picking.md](picking.md) | Viewport click to `EntityUuid`. Not an `ObjectId` |
| [gizmos.md](gizmos.md) | Translate and rotate overlay. Scale is not faked |
| [theme.md](theme.md) | Editor theme v1. Dark shell. Do not revert it |
| [outliner.md](outliner.md) | Hierarchy view of the entity registry |
| [selection.md](selection.md) | The one editor selection service |
| [inspector.md](inspector.md) | Placeholder now, type registry later |
| [content-browser.md](content-browser.md) | A slot, not an asset database |
| [commands.md](commands.md) | Authoring goes through engine commands |
| [../architecture/editor-runtime.md](../architecture/editor-runtime.md) | Host versus engine |
| [../adr/ADR-0031-native-editor-shell.md](../adr/ADR-0031-native-editor-shell.md) | Why the shell is Win32 |

The epic is JRV-0057. JRV-0058 through JRV-0064 and JRV-0070 are accepted. JRV-0065 is implemented and is not accepted until a human drags the gizmo. The shell is Editor theme v1. Do not start JRV-0066 from this page. The content browser still waits on an asset database that does not exist.
