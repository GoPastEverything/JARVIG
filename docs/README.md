# JARVIG documentation

`designdoc.html` at the repository root is the founding specification. These pages are the maintainable form of that document. They do not replace it.

## Authority

1. Accepted ADRs in [`adr/`](adr/README.md)
2. The pages in this tree
3. [`../designdoc.html`](../designdoc.html)
4. Accepted RFCs in [`rfc/`](rfc/README.md)
5. Source contracts and tests
6. Notes and session reports
7. Chat or model suggestions

## Map

| Area | Start here |
| --- | --- |
| Make a game | [getting-started.md](getting-started.md) |
| Current state | [status/CURRENT.md](status/CURRENT.md) |
| Tickets | [BACKLOG.md](BACKLOG.md) |
| Milestones | [../ROADMAP.md](../ROADMAP.md) |
| Architecture | [architecture/overview.md](architecture/overview.md) |
| Platform | [platform/overview.md](platform/overview.md) |
| Rendering | [rendering/overview.md](rendering/overview.md) |
| Materials | [materials/overview.md](materials/overview.md) |
| Assets | [assets/asset-system.md](assets/asset-system.md) |
| Simulation | [physics/overview.md](physics/overview.md), [animation/overview.md](animation/overview.md), [ai/overview.md](ai/overview.md), [audio/overview.md](audio/overview.md) |
| Terrain | [terrain/foundation.md](terrain/foundation.md), [adr/ADR-0058-terrain-is-a-local-heightfield.md](adr/ADR-0058-terrain-is-a-local-heightfield.md) |
| Networking | [networking/overview.md](networking/overview.md) |
| Scripting and plugins | [scripting/overview.md](scripting/overview.md), [plugins/overview.md](plugins/overview.md) |
| Build, profile, test | [build/overview.md](build/overview.md), [profiling/overview.md](profiling/overview.md), [testing/overview.md](testing/overview.md) |
| Hosts | [hub/overview.md](hub/overview.md), [editor/overview.md](editor/overview.md), [cli/overview.md](cli/overview.md) |
| Legal | [legal/THIRD_PARTY.md](legal/THIRD_PARTY.md) |
| Decisions | [adr/README.md](adr/README.md) |
| Research proposals | [rfc/README.md](rfc/README.md), [research/README.md](research/README.md) |
| Benchmarks | [benchmarks/README.md](benchmarks/README.md) |

## Founding sections

| `designdoc.html` | Maintained page |
| --- | --- |
| 1 Charter, 2 Principles | [architecture/overview.md](architecture/overview.md) |
| 3 License boundary | [legal/THIRD_PARTY.md](legal/THIRD_PARTY.md) |
| 4 Topology, 5 Repository | [../ARCHITECTURE.md](../ARCHITECTURE.md) |
| 6 Runtime | [architecture/engine-runtime.md](architecture/engine-runtime.md) |
| 7 World / coordinates | [architecture/world.md](architecture/world.md), [architecture/coordinate-frames.md](architecture/coordinate-frames.md) |
| 8 ECS | [architecture/ecs.md](architecture/ecs.md) |
| 9 Editor / PIE | [architecture/editor-runtime.md](architecture/editor-runtime.md), [architecture/play-in-editor.md](architecture/play-in-editor.md) |
| 10 Serialization | [architecture/serialization.md](architecture/serialization.md) |
| 11 Assets | [assets/asset-system.md](assets/asset-system.md) |
| 12 Renderer | [rendering/overview.md](rendering/overview.md) |
| 12A Materials | [materials/overview.md](materials/overview.md) |
| 13 Virtual geometry | [rendering/virtual-geometry.md](rendering/virtual-geometry.md), [rfc/RFC-0001-virtual-geometry-baseline.md](rfc/RFC-0001-virtual-geometry-baseline.md) |
| 14–20 Simulation and net | subsystem overviews |
| 21–27 Platform and process | scripting, plugins, build, profiling, testing |
| 28–29 Milestones and slice | [../ROADMAP.md](../ROADMAP.md) |
| 30 Agents | [../AGENTS.md](../AGENTS.md) |
| 32 ADR / RFC | [adr/README.md](adr/README.md), [rfc/README.md](rfc/README.md) |
| 33 Backlog | [BACKLOG.md](BACKLOG.md) |
