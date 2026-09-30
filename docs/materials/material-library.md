# Material library

JRV-0038. Not started. The browser, when built, must:

- Search by tags, physical class, source, resolution, texel scale, shading model, and license or provenance.
- Show thumbnails and turntables from the real renderer.
- Drag a material or instance onto a mesh slot.
- Batch-create instances from imported texture sets.
- Support favorites, collections, project and global libraries, and duplicate or dependency inspection.
- Record provenance: original package, author, license, hashes, and the import recipe.

`jarvig material library audit --missing-license --oversized --unused` is the provisional CLI. License fields are not optional for third-party texture sets. See [../legal/THIRD_PARTY.md](../legal/THIRD_PARTY.md).

Diagnostics (JRV-0040) expose variants, textures, passes, and GPU or resource counters in the editor and the CLI. `jarvig material stats` and `jarvig texture inspect` are the provisional commands. No counters are fabricated in Phase 0.
