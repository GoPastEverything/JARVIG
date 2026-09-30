# Audio

Not implemented. `engine/audio/` is reserved.

- Emitters and listeners use frame-aware transforms.
- Zones, occlusion, and environmental sends.
- Streaming ambience and music.
- Gameplay audio events are decoupled from sample playback.
- Server cooks keep gameplay-relevant metadata only, not the renderer or the sample bank, unless a target needs it.

Audio streaming uses the same residency language as assets: unresolved, loading, resident, evicted, failed. Do not give audio a private world origin.
