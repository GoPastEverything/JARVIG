# CLI

The CLI calls engine APIs. It does not own a second importer, cooker, or material compiler, and it does not drive the editor UI to fake those operations. When `jarvig material import` exists, it will call the same engine entry the editor panel calls. See [ADR-0018](../adr/ADR-0018-engine-owned-capabilities.md).

Package: `@jarvig/cli`. Binary name: `jarvig`.

Implemented:

```text
jarvig info
jarvig version
jarvig project validate <file>
```

`pnpm cli info` runs the TypeScript entry through `tsx`. Smoke runs `node tools/cli/dist/main.js info`.

Provisional commands from the founding doc that are **not** implemented. Names can change by ADR or by a CLI doc update in the same change. The capability cannot disappear:

```text
jarvig validate project
jarvig import --changed
jarvig cook --target webgpu-web|windows-desktop|dedicated-server
jarvig package --target windows-desktop --configuration shipping
jarvig test --suite smoke|perf

jarvig material import <dir> --recipe auto-review
jarvig material validate <asset>
jarvig material instance create <master> --set roughness=0.72
jarvig material compile <asset> --target webgpu
jarvig material stats <asset> --variants --textures --instructions
jarvig material library audit --missing-license --oversized --unused
jarvig texture inspect <texture> --mips --colorspace --streaming
```

Critical editor operations need a scriptable equivalent before the editor feature is called done. The Phase 0 editor has no authoring operations yet, so the gap is the unimplemented cook and material commands, not a parity bug in a working panel.
