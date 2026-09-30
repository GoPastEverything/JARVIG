# JARVIG Initial Bootstrap Report

Founding design understood: Yes. `designdoc.html` (AAA Engine Platform — Project Start & Technical Framework v0.2, 411 lines) was read in full and used as the architectural authority. The TBD codename was set to JARVIG by ADR-0014. No other founding technical decision was replaced.

Repository created: Yes. `git init` on branch `main`. Remote `upstream-playcanvas` → `https://github.com/playcanvas/engine`. No `origin`. No commit. The working tree is untracked because this session was not asked to commit.

Documentation created: Yes. Root guides (`README.md`, `AGENTS.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `LICENSES.md`), `docs/` architecture, rendering, materials, assets, subsystem, legal, ADR, RFC, research, benchmarks, backlog, and status pages. `designdoc.html` was not modified.

Modules created: `@jarvig/math`, `@jarvig/core`, `@jarvig/ecs`, `@jarvig/scene-schema`, `@jarvig/world`, `@jarvig/assets`, `@jarvig/materials`, `@jarvig/project-schema`, `@jarvig/render`, `@jarvig/editor-shell`, `@jarvig/editor`, `@jarvig/hub`, `@jarvig/client`, `@jarvig/dedicated-server`, `@jarvig/cli`, `@jarvig/sample-minimal`. Unimplemented founding directories have READMEs and are not packages.

Toolchain: Node v22.19.0, pnpm 10.15.1, TypeScript 5.9.3, Vitest 3.2.7, tsx 4.23.15, `@types/node` 22.20.4. Windows PowerShell scripts under `scripts/`.

Build result: `pnpm typecheck` and `pnpm build` passed (`tsc -b`).

Test result: `pnpm lint` passed. `pnpm test` passed, 25 tests in 2 files.

Editor bootstrap result: `node apps/editor/dist/main.js --once` printed `JARVIG_OK editor` with modules `world,render`, viewport bound, backend `null`. Serving mode is `pnpm dev:editor` on `127.0.0.1:4780`. Not a GPU viewport.

Client bootstrap result: `JARVIG_OK client` with modules `world,render`, null device, material schema `jarvig.material/v1`.

Server bootstrap result: `JARVIG_OK dedicated-server` with module `world`, `graphics=none`. Sources do not import `@jarvig/render`.

CLI result: `jarvig info` printed version 0.0.1, phase P0, WebGPU target, and research flags off. `jarvig project validate tests/fixtures/minimal.project.json` printed `JARVIG_OK project Minimal`.

Known issues: No license. No git commit. CI workflow not executed on a forge. PlayCanvas not imported. Renderer is a null device. JRV-0017 render stress test not done. Editor is not the P1 shell.

Architecture questions: None that blocked the bootstrap. Open product choices are the JARVIG license and when to vendor PlayCanvas. Both are recorded rather than guessed.

ADRs created: ADR-0001 through ADR-0016, all Accepted.

RFCs created: RFC-0001 through RFC-0006, all Proposed. No research flag enabled. No benchmark numbers.

Next ticket: JRV-0007 — Editor shell.
