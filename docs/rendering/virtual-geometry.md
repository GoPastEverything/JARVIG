# Virtual geometry

"Better than Nanite" is a hypothesis. It is not an architectural assumption. The research sequence is fixed:

```text
Correct normal renderer
  -> GPU-driven scene
  -> meshlets
  -> hierarchical geometry clusters
  -> geometry streaming
  -> error-based selection
  -> measured virtual-geometry baseline
  -> experimental procedural microgeometry
  -> experimental representation virtualization
```

## Baseline requirements

- Offline meshlet and cluster decomposition.
- Hierarchical geometric-error metric.
- Compressed bounded pages.
- GPU LOD and cluster selection.
- Resident parent fallback when child pages are missing. No holes.
- Streaming feedback for higher-detail pages.
- Page residency is the resource ladder in [resources.md](resources.md): resident, evicted, or requested. It is not a second handle system. Those pages are not implemented.

## Required metrics

GPU frame time at matched quality, screen-space geometric error, VRAM and resident memory, storage size, streaming bandwidth, page-fault rate, latency sensitivity, cook time, temporal stability, and authoring complexity.

Promotion rule: optional until a repeatable win and a graceful fallback are recorded. Raw results live under [../benchmarks/README.md](../benchmarks/README.md). There are no results yet.

RFC: [RFC-0001](../rfc/RFC-0001-virtual-geometry-baseline.md). Milestone P6.

The hierarchy-selection slice was human accepted on 2026-09-26. `View > Cluster Hierarchy` defaults off. It builds a parent/child tree over the accepted 61,702 townshop meshlets and selects a cut by projected error. On the reviewed close view the status read `hier 49762/123403` and `hier 49783/123403`, with the shaded shop intact. Cyan is a selected leaf. The denominator counts internal parents as well as leaves.

The next slice stores a simplified mesh on a parent whose collapse moved a vertex, and draws that mesh instead of the leaves under it. The recorded error is the farthest vertex move, and screen-space error uses that distance. A parent is kept when it has triangles. If simplification comes back empty, the build restores the child merge, and a last resort is a bounds box so the node is not empty. Each level records node count, parent triangles, child triangles, how many parents cover their child centers, and empty parents. Outer boundary vertices stay put. Close range submits the leaf triangles. Distance submits fewer. `lod` on the status line is submitted triangles over the leaf-triangle reference. There is no page streaming and no procedural detail. The human accepted this parent-geometry slice on 2026-09-27 as RFC-0001. It is not a Nanite result, and the measured costs in the acceptance note are not production targets.

`JARVIGEditor --rfc0001` loads townshop, turns on Draw From Meshlets and Cluster Hierarchy, and frames the storefront from close, medium, and far at 0.5, 1, 2, and 4 px. Frustum and occlusion are off for that run so the triangle count is the LOD cut. It writes `native/target/rfc0001/*.png`, `report.csv`, and `report.json`. Each shot is compared with a leaf-reference capture of the same camera. On 2026-09-27 that run printed `RFC0001_PASS` on Intel UHD: the sidecar loaded, the root cut was 699898 triangles, empty parents were 0, close stayed on all 6122214 leaf triangles, medium fell to -60.0%, and far fell to -67.9%.

`JARVIGEditor --rfc0001-runtime` moves the same cameras continuously from close to far and back, then repeats close, medium, and far with frustum culling and conservative occlusion on and meshlet colors off. Selection, cut construction, index upload, draw preparation, and renderer CPU time are separate counters. On 2026-09-27 that run printed `RFC0001_RUNTIME_PASS`. Selection median was 20.1 ms and the maximum was 32.5 ms. Every dolly frame covered all 61702 leaves. Hole ratio stayed 0. The sidecar loaded and was not rebuilt. GPU frame timestamps are not measured. Rebuilding the parent index buffer when the cut changes took about 2.7 s in this debug editor and is the frame-time cost, separate from selection. The human accepted RFC-0001 on 2026-09-27. The frozen record is [docs/benchmarks/rfc-0001/2026-09-27](../benchmarks/rfc-0001/2026-09-27/notes.md). The measured upload, storage, load, and timing gaps in that note stay open.
