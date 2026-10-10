# JARVIG manual

This manual is how you make something with JARVIG. It is written for a person sitting at the editor: create a project, place a solid, bring in a model, light a level, press Play, and build a loose game. The same pages are the front door if a working session is gone and the repository is all that remains.

The generated site is a reader for these pages and for the architecture library beside them. It does not outrank an accepted decision. Authority stays:

1. Accepted ADRs
2. The architecture pages under `docs/`
3. `designdoc.html` at the repository root
4. Accepted RFCs
5. Source contracts and tests
6. Status notes
7. Chat

Build the site from the repository root:

```powershell
pwsh -File scripts\build-docs.ps1
```

Open `docs/site/index.html`. Rebuild that folder with the command above. It is a reader, not a second authority. Session logs stay in `docs/status/` on disk and are not part of the site. The private surface rule for procedural detail is not taught here.

## Start here

| What you want | Page |
| --- | --- |
| Build the editor and place the first block | [Your first hour](../getting-started.md) |
| Camera, selection, undo, panels | [The editor](editor.md) |
| Extrude, round, inset, and bevel | [Blocks](modeling.md) |
| glTF import and the content browser | [Models and content](content.md) |
| Level, Land, and Character | [Worlds](worlds.md) |
| Environment, lights, and debug views | [Lighting](lighting.md) |
| Play and a standalone exe | [Play and build](play.md) |
| The public API, as it exists | [Programming](programming.md) |
| Meters, axes, and faces | [Coordinates](coordinates.md) |
| What this build does not do | [Limits](limits.md) |
| Where a cold session reads next | [Authority](authority.md) |

## Two kinds of page

A manual page tells you what to click and what gets saved. An architecture page or an ADR tells you what the engine is required to do. When they disagree, the accepted ADR wins, and the manual should be corrected. Do not treat a green test name in a status log as a feature you can ship.

The TypeScript dock at `http://127.0.0.1:4780` is an old prototype host. `JARVIGEditor.exe` is the product.
