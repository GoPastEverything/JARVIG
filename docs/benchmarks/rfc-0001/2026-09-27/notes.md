# RFC-0001 acceptance record

Accepted by the human on 2026-09-27. This is the architectural acceptance of the cluster hierarchy, parent coverage, screen-space LOD selection, the `.jarvigparents` sidecar, and runtime composition with frustum culling and conservative occlusion. It is not a production performance target and it is not a win over another engine.

Machine: this Windows laptop, Intel UHD Graphics through wgpu DX12, debug `JARVIGEditor`. Driver version was not recorded. Scene: Lighting Lab townshop, asset `affe7d8d-e3e1-448a-a008-76c6e6266867`, 61,702 meshlets, 6,122,214 triangles. The meshlet partition was not changed.

## Commands and verdicts

`--rfc0001` printed `RFC0001_PASS`.
`--rfc0001-runtime` printed `RFC0001_RUNTIME_PASS`.

SHA-256 of the copied reports matches `SHA256SUMS.txt`. The PNG hashes in that file name captures that stay under `native/target/` and are not copied here.

## Measured results

Correctness, 1 px, frustum and occlusion off:

| Camera | Distance | Triangles | Change |
| --- | ---: | ---: | ---: |
| Close | 2.40 m | 6,122,214 / 6,122,214 | 0% |
| Medium | 13.59 m | 2,450,164 / 6,122,214 | -60.0% |
| Far | 24.15 m | 1,963,494 / 6,122,214 | -67.9% |
| Far at 4 px | 24.15 m | 1,171,846 / 6,122,214 | -80.9% |

Root cut 699,898 triangles. Empty parents 0. Parent nodes checked 61,701. Missed child centers 0. Hole ratio 0. `cache_hit: true`.

Runtime dolly, 49 frames, close to far and back:

- Selection median 20,110 µs, maximum 32,506 µs.
- Cut construction median 85,458 µs.
- Parent index upload median 2,702,859 µs, maximum 4,464,536 µs, about 23 MB when the cut changes.
- Renderer CPU median 3,248,072 µs.
- Covered leaves 61,702 on every frame.
- Hole ratio 0 at the close, medium, and far checks.
- Cold start until the hierarchy was usable: 426,236 ms.
- Rebuild: false.
- Recorded parent build stored in the sidecar: 677,692.5 ms.

Integration, hierarchy on, frustum on, occlusion on, meshlet colors off: close 6,122,214 triangles and 0 parents; medium 2,450,164 triangles and 5,226 parents; far 1,963,494 triangles and 2,670 parents. Submitted 61,702, frustum rejects 0, occlusion rejects 0, conservative-visible 61,702. Those zeros are the conservative reference drawing the framed shop, not an occlusion rate.

## Memory

| Item | Bytes |
| --- | ---: |
| Leaf meshlet scene data | 3,948,928 |
| Meshlet sidecar | 50,155,678 |
| Hierarchy plus parent indices on CPU | 309,932,248 |
| Parent GPU vertex storage | 2,351,284,320 |
| Parent GPU frame index buffer | 23,480,376 |
| `.jarvigparents` sidecar | 2,325,318,884 |
| Peak temporary build memory | not measured |

## Explicitly still open

- Hierarchy selection at 20.1 ms median / 32.5 ms max.
- Parent index upload at about 2.7 s when the cut changes.
- Parent GPU vertex storage at 2.35 GB.
- `.jarvigparents` sidecar at 2.33 GB.
- Cache reload at 426 s.
- GPU timestamp queries are not implemented.
- Peak hierarchy-build memory is unmeasured.
- JRV-0088 remains open.

GPU frame time was not measured. Do not treat any number above as a shipping budget.
