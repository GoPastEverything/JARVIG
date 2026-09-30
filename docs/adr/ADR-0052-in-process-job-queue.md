# ADR-0052 — An in-process job queue is not a second engine

Status: Accepted
Date: 2026-09-26

## Context

Building parent meshes for the townshop cluster hierarchy ran on the editor UI thread. Windows marked JARVIGEditor not responding, and the process then stopped. The leaf meshlets were already a valid draw. The build did not need the live world or the device.

`docs/api/threading.md` already names a Job thread class and says asset work is a request. That queue did not exist. A second `EngineSession` per job would copy the world and the renderer. A `JARVIGWorker.exe` process is a later backend for crash isolation, not the first queue.

## Decision

`JobManager` lives in `jarvig_core`. It is a bounded in-process worker pool.

A job has `JobId`, a name, an optional `AssetId`, an optional cache key, dependencies, and one of: Queued, Running, CancelRequested, Cancelled, Completed, Failed. The host can read progress from 0 to 1, a stage line, and elapsed time, and it can cancel.

The worker receives the values captured at submit. It does not borrow the live world, the editor, or the renderer. A panic or an error becomes Failed. Cancel drops the result. The previous representation stays.

The host takes the result on the thread that owns the device and publishes it at a frame boundary. The queue does not create GPU resources.

The cache key is `AssetId`, the source fingerprint, and the builder version, stored on the job. This queue does not write a disk cache and does not start `JARVIGWorker.exe`.

The first caller is the RFC-0001 parent-mesh build. While it runs, the editor keeps drawing the accepted leaf meshlets.

## Alternatives Considered

- One engine process per job. Rejected. It duplicates the world and the device.
- A worker executable in this change. Rejected. The editor died on an in-process stall. A process boundary is a later backend for the same job record.
- Blocking the UI with a load overlay. Rejected. That is the failure this decision replaces.

## Consequences

Hosts poll `JobManager` and publish results themselves. A job that mutates the live scene is a threading bug. Disk reuse of derived parent meshes is still later. The pool size is at most four threads.

## Supersedes

Nothing.

## Superseded By

Nothing.
