# Authority

The manual is the creator front door. It is not a second specification. If a manual page and an accepted ADR disagree, follow the ADR and fix the manual.

## Order

1. Accepted ADRs in `docs/adr/`
2. Architecture and subsystem pages in `docs/`
3. `designdoc.html` at the repository root
4. Accepted RFCs in `docs/rfc/`
5. Source contracts and tests
6. Status notes and session reports
7. Chat or a model's suggestion

An ADR outranks the founding document only for the decision it names, and only when it says what it supersedes. A research RFC is not production architecture. `designdoc.html` stays. Do not delete it because these pages exist.

The documentation map is [the docs index](../README.md). Decisions are indexed in [the ADR index](../adr/README.md).

## If a session is gone

Read in this order before changing the engine:

1. This manual, so you know what a person can do in the editor.
2. `docs/status/CURRENT.md` on disk. That file is the recovery log: what runs, what was last verified, and what must not be started. The generated site does not copy it, because it contains session measurements.
3. `docs/status/NEXT.md` for the next constraint.
4. The subsystem page for the area you will touch, then the accepted ADRs that page names.
5. The current source and tests for that subsystem.
6. `Agents.md` at the repository root, for how a change is validated.

Then look at `git status` when you are in a clone that has commits. The private working tree at `C:\JARVIG` may have no commits of its own. The public source, when it has been published, is a separate tree. Publishing is a separate decision from writing docs. This manual does not authorize a push.

## What the site includes

`scripts/build-docs.ps1` turns the Markdown library into `docs/site/`. The site includes the manual, the editor, the architecture, the API, the ADRs, and the subsystem overviews.

It leaves these out on purpose:

| Left on disk | Why |
| --- | --- |
| `docs/status/` | Session logs and measurements. They are the recovery record, and they are not a creator tutorial. |
| `docs/BACKLOG.md` | The ticket board, including harness names. |
| `docs/benchmarks/` | Measured runs. |
| `docs/rendering/microgeometry.md` | The private surface rule. |
| `docs/research/aperiodic-detail.md` | The same research, in note form. |
| `docs/rfc/RFC-0002-procedural-microgeometry.md` | The unstamped proposal and its results. |

Those files remain in the repository. A link in the site that would have opened one of them is marked as repository-only. RFC-0002 stays proposed until a person accepts the look. Do not treat a manual page as that acceptance.

The local site can still contain an accepted ADR whose text names the surface rule, because that ADR is architecture. That is why `docs/site/` is gitignored and must be excluded from any copy onto the public tree. The manual chapters do not teach the rule.

## What a documentation change includes

A correction of a creator page updates the page, the docs index when the map changed, and `docs/status/CURRENT.md` when the change is the session's work. A new behavior still needs the engine change, a test, and the architecture page in the same change. A manual page that describes a click does not mark a ticket done.

## Tickets the manual must not close

Character looks, terrain looks, and rendering experiments stay open until a person accepts the picture. The manual can tell you how to open the window. It does not stamp the ticket. Roadmap phases in `ROADMAP.md` are the sequence from the founding document. Later phases are not implied by the existence of this site.
