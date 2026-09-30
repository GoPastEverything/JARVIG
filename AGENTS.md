# JARVIG agent rules

GPT CLI and Grok Build work on this repository as implementers, not as substitute architects. Conversation memory is not a source of truth. The files below are.

## Read before editing

1. This file.
2. [`docs/status/CURRENT.md`](docs/status/CURRENT.md).
3. The architecture doc for the subsystem you are about to touch.
4. Accepted ADRs linked from that doc.
5. The current source and tests for that subsystem.
6. `git status` / `git diff` when a repository exists.

`designdoc.html` remains the founding specification. Read the relevant section when the architecture docs are silent or when you suspect drift. Do not skim it and invent a different engine.

## Authority

Use this order. A lower source never silently overrides a higher one.

1. Accepted ADRs
2. Current architecture documentation
3. `designdoc.html`
4. Accepted RFCs
5. Source-code contracts and tests
6. Temporary implementation notes
7. Chat or model suggestions

An ADR may supersede the founding design only when it names what it supersedes and why. Research RFCs are not production architecture.

## Hard rules

- JARVIG is a native multi-platform engine. Browser execution is an export target. Browser technology must never become an implicit dependency of engine core. ADR-0017.
- The editor never owns engine functionality. Import, cook, shaders, materials, worlds, physics setup, navmesh, builds, and profiling are engine APIs. The editor, the CLI, and automation call those APIs. ADR-0018.
- Public APIs follow `docs/api/` and ADR-0022. Internal crates use Rust types. The stable boundary is C, with opaque handles and `JarvigResult`. Do not invent a per-subsystem ABI, do not pass Rust or C++ standard-library types across it, and do not let a panic or a C++ exception cross it. One type registry. Commands are authoring transactions, not a wrapper around every tick.
- JARVIG is one engine used by multiple hosts. The editor hosts the engine. The dedicated server uses the same world and simulation architecture without editor or renderer modules.
- Do not wrap the TypeScript prototype in Electron, a WebView, or a hidden browser and call that the engine.
- The TypeScript packages under `engine/` are the executable prototype of the contracts. New hot-path systems go to `native/jarvig_core` or behind its C ABI. Do not delete a prototype module until the native one passes the same tests.
- Do not introduce BSP, or any baked brush/portal world, as the authoritative world representation.
- Engine modules never import editor, hub, client, server, sample, or tool modules.
- The dedicated server must not import `@jarvig/render` or editor packages. `pnpm lint` enforces the table in `tools/validation/check-boundaries.mjs`.
- Do not silently change a public interface, schema id, snapshot shape, CLI command, or module id. If a break is required, change the version, add a migration or an ADR, and update tests in the same change.
- Architectural changes require an ADR. Large experimental systems require an RFC with measurable success and failure criteria.
- Research systems stay behind feature flags and default off: virtual geometry, procedural microgeometry, representation virtualization, server meshing, advanced GI. Do not claim a win over Nanite or any other engine without a recorded benchmark.
- Never fabricate benchmark numbers, test results, or license statements.
- Never hide a failing test, skip it to go green, or weaken an assertion to match a bug.
- Do not copy proprietary CryEngine, Unreal, or other restricted implementation source. Public docs are behavioral references only. See `docs/legal/`.
- Do not vendor PlayCanvas, or any other third-party code, without recording the license, version, and upstream remote in `docs/legal/THIRD_PARTY.md` and `LICENSES/dependency-manifest.json`.
- Keep Windows and PowerShell working. Do not replace `scripts/*.ps1` with bash-only flows.
- Keep CLI and Editor capabilities aligned where the docs require parity. A critical editor operation without a CLI/API path is unfinished.
- Do not redesign an unrelated subsystem in the same change.
- Experimental code stays feature-flagged until an accepted ADR promotes it.
- Prefer the smallest change that satisfies the ticket's acceptance criteria.

## While implementing

Every meaningful change moves together:

```text
CODE
TESTS
DOCS
docs/status/CURRENT.md
```

Docs to touch are the subsystem page, the backlog status for the ticket, and CURRENT.md. Do not mark a ticket Done in `docs/BACKLOG.md` unless its acceptance criteria passed in this session and the command is named.

## Validation

Run the narrowest check that proves the change, then the suite that covers it. For engine/host work that is at least:

```powershell
pwsh -File scripts\test.ps1
```

Before calling a milestone task done, also run:

```powershell
pnpm build
pnpm smoke
```

Report the commands and the real results. If a browser or GPU check could not be run, say so. Do not describe a WebGPU frame as verified when only the null device ran.

## Session handoff

End every substantial session by updating `docs/status/CURRENT.md` and leaving this block in the session report (and, when the change is large, in `docs/status/` as a dated note):

```text
SESSION HANDOFF

Goal:
Completed:
Changed files:
Tests run:
Results:
Architecture decisions:
Known issues:
Blocked:
Next:
```

The next line must name the next JRV ticket and the exact command a cold session should run first.

## Roles

| Agent | Responsibility |
| --- | --- |
| GPT CLI | Integration, contracts, implementation, tests, review |
| Grok Build | Independent challenge, stress cases, renderer research, benchmark criticism, implementation |
| Human lead | Promotion gates, product direction, license choice, milestone priority |

Disagree in the handoff. Do not "resolve" a disagreement by editing the architecture without an ADR.
