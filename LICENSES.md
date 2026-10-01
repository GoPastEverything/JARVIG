# Licenses

JARVIG does not have an open-source license yet. The root `package.json` says `UNLICENSED` on purpose. Nothing in this repository grants rights to the JARVIG sources until a human lead chooses a license and an ADR records it.

Third-party obligations are tracked in:

- [`docs/legal/THIRD_PARTY.md`](docs/legal/THIRD_PARTY.md) — human-readable register
- [`LICENSES/THIRD_PARTY_NOTICES.md`](LICENSES/THIRD_PARTY_NOTICES.md) — notices required to ship
- [`LICENSES/dependency-manifest.json`](LICENSES/dependency-manifest.json) — machine-readable direct dependencies
- [`docs/legal/CRYENGINE_BOUNDARY.md`](docs/legal/CRYENGINE_BOUNDARY.md)
- [`docs/legal/CLEAN_ROOM_GUIDELINES.md`](docs/legal/CLEAN_ROOM_GUIDELINES.md)
- [`docs/legal/RESEARCH_DISCLOSURE_BOUNDARY.md`](docs/legal/RESEARCH_DISCLOSURE_BOUNDARY.md)

## Zones

| Zone | What may live here |
| --- | --- |
| A — Core | JARVIG code and approved redistributable open source |
| B — Adapters | Optional isolated third-party bridges, behind an engine interface |
| C — Reference | Public docs, papers, and clean-room behavior notes. No restricted source |

PlayCanvas Engine is MIT-licensed prior work that may be used as a reference or optional adapter. It is **not** the JARVIG engine foundation, is not linked into the native runtime, and is not vendored into the engine core. CryEngine is not Zone A or Zone B. Unreal, Nanite, and related Epic materials are Zone C public documentation only.

Adding a package is incomplete until `docs/legal/THIRD_PARTY.md` names the package, version, license, and why it is here.

## Public research source

Public visibility does not convert JARVIG research into open-source code. Experimental providers may remain private/local while their public interfaces, attribution, and acceptance criteria are documented here.

The repository must not imply that an unpublished provider is present in `master`, and it must not publish implementation details merely to prove private progress. See [Research disclosure boundary](docs/legal/RESEARCH_DISCLOSURE_BOUNDARY.md).
