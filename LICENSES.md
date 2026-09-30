# Licenses

JARVIG does not have an open-source license yet. The root `package.json` says `UNLICENSED` on purpose. Nothing in this repository grants rights to the JARVIG sources until a human lead chooses a license and an ADR records it.

Third-party obligations are tracked in:

- [`docs/legal/THIRD_PARTY.md`](docs/legal/THIRD_PARTY.md) — human-readable register
- [`LICENSES/THIRD_PARTY_NOTICES.md`](LICENSES/THIRD_PARTY_NOTICES.md) — notices required to ship
- [`LICENSES/dependency-manifest.json`](LICENSES/dependency-manifest.json) — machine-readable direct dependencies
- [`docs/legal/CRYENGINE_BOUNDARY.md`](docs/legal/CRYENGINE_BOUNDARY.md)
- [`docs/legal/CLEAN_ROOM_GUIDELINES.md`](docs/legal/CLEAN_ROOM_GUIDELINES.md)

## Zones

| Zone | What may live here |
| --- | --- |
| A — Core | JARVIG code and approved redistributable open source |
| B — Adapters | Optional isolated third-party bridges, behind an engine interface |
| C — Reference | Public docs, papers, and clean-room behavior notes. No restricted source |

PlayCanvas Engine is MIT and is the intended Zone A foundation. It is **not imported** in Phase 0. CryEngine is not Zone A or Zone B. Unreal, Nanite, and related Epic materials are Zone C public documentation only.

Adding a package is incomplete until `docs/legal/THIRD_PARTY.md` names the package, version, license, and why it is here.
