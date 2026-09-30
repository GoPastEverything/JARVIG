# Contributing to JARVIG

Read [`AGENTS.md`](AGENTS.md) first. Humans and agents follow the same rules.

## Branch names

```text
main                      buildable
feature/<issue>-name      short-lived production work
research/<topic>          experiments; never silently production
release/x.y               only after formal releases exist
```

Use the JRV id in the branch when the work is a backlog ticket, for example `feature/JRV-0007-editor-shell`.

## Change rules

- Public API, schema, and CLI changes update tests and docs in the same change.
- Architecture changes ship with an ADR. Say what it supersedes.
- Research changes ship with an RFC, a feature flag, and a benchmark plan. They do not flip the default on.
- Performance changes include raw before/after data. No unrelated refactor inside a comparison.
- Do not commit Derived Data Cache output, `dist/`, or `node_modules/`. Golden fixtures under `tests/` are allowed.
- Do not add a dependency without a license entry. See [`docs/legal/THIRD_PARTY.md`](docs/legal/THIRD_PARTY.md).
- Generated cooked data is not committed except explicit golden fixtures.

## Checks

```powershell
pwsh -File scripts\test.ps1
pnpm build
pnpm smoke
```

`pnpm lint` is the architectural boundary check, not a stylistic linter. `pnpm typecheck` is the TypeScript gate. Formatting follows `.editorconfig` (LF in the repository, CRLF for PowerShell). There is no Prettier yet.

## Commit and review

Do not claim a ticket is done in `docs/BACKLOG.md` unless the acceptance criteria were run. Name the command in the ticket's validation line or in the session handoff.

A reviewer should be able to answer, from the diff alone: which ticket moved, which ADR/RFC it depends on, which tests ran, and what is still broken.

## Windows

Scripts under `scripts/` are PowerShell. Node is 22+. pnpm is the package manager (`packageManager` in the root `package.json`). Do not require WSL for the Phase 0 loop.

## Licenses

JARVIG itself is `UNLICENSED` until the human lead chooses a license. That is not a grant. Do not relicense third-party code by copying it in.
