# ADR-0071 — Einstein surface builds have their own latency queue

Status: Accepted
Date: 2026-10-03

## Context

ADR-0052 puts one in-process `JobManager` in `jarvig_core`. The editor starts that pool with two workers. Parent-mesh LOD builds, project registry scans, terrain chunk builds, and Einstein surface builds all submitted to it.

Einstein's result is what the current camera is asking to see. A parent LOD or a content scan can wait. A surface build that sits behind those jobs does not. The laptop this editor runs on is an integrated GPU, so a second large pool would take time from the frame thread.

Exact parametric solids still do not build patches. Editing latency on those solids is pick, validation, mesh rebuild, inspector refresh, and cage draw. Those times are measured on the frame thread. This decision does not replace that measurement with a topology acceleration structure.

## Decision

The editor keeps the ADR-0052 `JobManager` for general work and adds a sibling `JobManager` named `jarvig-einstein` with one worker. The thread is `jarvig-einstein-0`. The general pool stays `jarvig-job-0` and `jarvig-job-1`.

Einstein surface builds submit only to the sibling. Parent LOD, registry scan, and terrain stay on the general pool. The two managers do not share a queue. Job ids are per manager.

A request is the values captured at submit. It is immutable on the worker. The worker does not mutate the world and does not create GPU resources. The frame thread publishes. A newer request cancels the previous unfinished Einstein job. The product carries the epoch and the cache key `micro:{key}`. The host publishes only when that epoch is still wanted. A stale epoch is abandoned and is not uploaded. The same input key still reuses a resident build and does not enqueue another.

The sum of in-process job threads in the editor stays at most four. This slice uses two general workers and one Einstein worker.

The surface builder is unchanged. This decision does not add a patch priority queue, a topology BVH, incremental validation, or GPU generation.

Counters recorded beside the existing queue wait, CPU generation, upload, publish, reuse, and cancelled counts are classification time on the frame thread, and host wait from the worker finishing until the frame thread accepts the mesh. Topology selection records face, edge, and vertex pick time, solid validation, derived mesh rebuild, inspector refresh, and cage draw. The first slow stage is whichever of those counters moves.

## Consequences

This supersedes only ADR-0052's consequence that one shared queue carries every background build, and only for Einstein surface builds. ADR-0052 still owns the job record, the states, the cancel rules, the ban on world mutation and GPU creation on the worker, the four-thread cap, and the rule that `JARVIGWorker.exe` is not built. The ADR-0052 decision text is not rewritten.

Exact solids remain Exact and are not submitted to the builder. RFC-0002 stays unstamped. No JRV ticket is opened.

## Alternatives

Growing the shared pool. Rejected. Extra workers on this machine compete with the editor and the renderer, and they do not stop a surface build from waiting behind a parent LOD.

A process-wide thread cap in code. Rejected. The editor budget is stated here. Each manager still clamps its own pool to four threads.

Rewriting the surface builder into GPU compute. Rejected for this decision. Measure the frame first.

## Supersedes

The single shared queue for Einstein surface builds in ADR-0052.

## Superseded By

Nothing.
