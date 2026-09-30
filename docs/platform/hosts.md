# Hosts

| Host | Ships as | Calls | Must not include |
| --- | --- | --- | --- |
| Editor | `JARVIGEditor` native process | Engine APIs, then draws panels | Its own importer, cooker, or renderer |
| Player | `MyGame` plus the runtime library, or a static link | Engine APIs | Node, Electron, a private HTTP server, DOM |
| Dedicated server | `JARVIGServer` | Core, world, gameplay, networking | Renderer, editor, UI |
| Web export | WASM plus a small platform layer | The same engine API | A second world model written in JavaScript |
| CLI and build farm | tools | The same engine API the editor uses | A private copy of cook or import |

The processes under `apps/` and `hosts/` today are Node prototypes of those hosts. `apps/editor` serves a temporary dock so the command bus can be tested. That page is not the product editor and it does not present. The product window is `native/jarvig_editor` (`JARVIGEditor.exe`). `native/jarvig_editor_host` is the two-view renderer test. `apps/player/` is reserved for the native player and is empty on purpose. The editor executable is not that player.

`winit` stays inside `jarvig_platform`. The renderer never imports it.

`@jarvig/node-host` is allowed to use Node. `engine/**` is not.
