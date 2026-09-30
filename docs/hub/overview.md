# Hub

`apps/hub` is the project launcher. Phase 0 prints the engine version, phase, founding document, and the editor launch command, then exits. It does not create projects and it does not spawn the editor process.

The founding Hub manages projects, engine versions, and SDKs, then opens the editor. That product behavior is later work. The hub must stay a host: it may depend on engine public APIs and must not become a second scene model.

Command: `pnpm dev:hub`.
