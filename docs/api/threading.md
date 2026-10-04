# Threading and async work

Every public operation states where it may run. An unstated thread is a bug in the API, not a freedom.

| Class | Meaning |
| --- | --- |
| Simulation | The thread that owns world mutation for that session. Today this is the thread that calls the runtime. |
| Render | The thread that owns the GPU device. The current native host is one thread, so simulation and render are the same thread until a ticket splits them. The class is still named so the split does not invent the rule later. |
| Any | Safe to call from any thread. The function says what it synchronizes. |
| Job | Safe inside the engine job system, once that system exists. Not a license to spawn an OS thread and touch the world. |
| Async | The call only starts work. Completion is a request handle. |

The in-process queue is `JobManager` in `jarvig_core`. ADR-0052. Workers are the Job class. They do not mutate the world and they do not create GPU resources. The host publishes a finished result on the simulation and render thread. `JARVIGWorker.exe` is not built. Einstein surface builds use a sibling `JobManager` with one worker, `jarvig-einstein-0`. Parent-mesh LOD, registry scans, and terrain stay on the general pool. ADR-0071. The editor's combined worker count stays at most four. That worker still does not mutate the world or create GPU resources.

- World mutation is simulation-thread only.
- GPU resource creation is render-thread only, which is that same thread.
- Logging must be safe from any thread. The bootstrap logger is not proven for that yet. New log APIs are.
- A call on the wrong thread returns `JARVIG_WRONG_THREAD`. It does not silently queue unless the function is documented as a command submission.

## Async

Asset loads, streaming, cooking, shader compiles, and network calls are requests. They are not required to block the simulation thread.

```text
JarvigRequestHandle
jarvig_request_poll
jarvig_request_cancel
```

`poll` is Any-thread if the request says so. `wait` is allowed as an explicit function for tools. It is not the shape of the runtime API. A completion callback, if offered, names the thread it runs on.

The scheduler itself is not this doctrine. A ticket that adds jobs inherits these classes. It does not replace them with "the caller will be careful."

Blocking the simulation thread on a cook or a shader compile is a tool behavior. A game frame does not do it.
