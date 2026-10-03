# Hub

The product hub is `JARVIG.exe` in `native/jarvig_hub`. It is the screen that appears before a project is open. ADR-0060.

A launch with no project shows Recent Projects, New Project, and Open Project. The window reads `.jarvigproject` files and a small recent list. It does not construct a world, scan content, or start the renderer. Closing it exits. Choosing a project reads the manifest, then the editor opens the startup level.

`JARVIGEditor --project <path.jarvigproject>` skips the hub. `--self-test` skips it too and stays on the two-triangle bootstrap. Lighting Lab is a sample project. It is not the startup project.

`pnpm dev:hub` and `apps/hub` are the old phase-0 script. That script is not this product. Do not wrap it in a browser and call it the hub.

The startup contract, the four templates, and the recent-project file are [../editor/hub.md](../editor/hub.md).
