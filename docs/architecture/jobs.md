# Jobs

The job queue is engine code in `jarvig_core`. The editor submits work and paints the result. It does not run a second world or a second renderer. ADR-0052.

```text
JARVIG engine     world, simulation, snapshots, rendering
JARVIG job queue  in-process workers, progress, cancel, one result
JARVIG editor     submits, lists, cancels, publishes on the frame thread
```

`JARVIGWorker.exe` is not built. A later process can carry the same job description for offline work. This queue does not schedule GPU tasks, page streaming, or a build farm.

A job is Queued, Running, CancelRequested, Cancelled, Completed, or Failed. The snapshot shows the name, optional asset, progress, stage, elapsed time, dependencies, and error. Cancel of a queued job does not run it. Cancel of a running job is a flag the work checks. A failed or cancelled job does not replace the last good result.

Workers see the inputs captured at submit. The editor copies the mesh and the meshlets, then returns to the frame loop. The frame thread takes a finished result and uploads it. Until that upload, Draw From Meshlets keeps the leaf meshlets.

The parent-mesh job is `Build Parent Mesh LODs`. Its cache key is the asset id, the meshlet source fingerprint, and the parent-mesh builder version. A parent that covers a pair of meshlets stays at 32 triangles. A parent that covers more of the mesh is allowed more, up to 2048. The upload is split into 64 MB pieces and refuses a single buffer over 200 MB. A finished hierarchy is written to `Intermediate/Meshes/<AssetId>.jarvigparents` and a later launch loads that file instead of rebuilding. The original glTF is not modified. While Cluster Hierarchy is on, a sidecar at or under 64 MB loads without a build. A missing sidecar, or a larger one, stays on the leaves until Cluster Hierarchy is turned off and on. That toggle may build. The original glTF is not modified.

View > Background Jobs lists the queue. View > Cancel Background Job cancels the parent-mesh job. The status line shows the running job. The output log records state changes and each 5% step.
