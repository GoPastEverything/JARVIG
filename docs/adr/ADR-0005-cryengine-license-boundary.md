# ADR-0005 — CryEngine license boundary

Status: Accepted
Date: 2026-09-22

## Context

CryEngine is a useful architectural and rendering reference. Its license is not the MIT license of the community patch repository.

## Decision

CryEngine source, shaders, and proprietary formats are out of Zone A and out of the default build. Public docs may inform clean-room specifications. See `docs/legal/CRYENGINE_BOUNDARY.md` and `docs/legal/CLEAN_ROOM_GUIDELINES.md`.

A Zone B adapter that links CryEngine would require a new ADR and a license the human lead accepts. It must be optional.

## Alternatives Considered

- Import the community edition because its patch files are MIT. Rejected. The engine underneath is not MIT.
- Ban reading public CryEngine documentation. Rejected. Reference study is allowed. Copying is not.

## Consequences

Code review rejects CryEngine pastes. Agents say so in the handoff if they have been exposed to restricted source.

## Supersedes

Nothing.

## Superseded By

Nothing.
