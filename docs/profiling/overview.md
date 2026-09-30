# Profiling

Every major subsystem exposes named timings and counters from its first implementation. Phase 0 does this with `Engine.telemetry` counters, not with estimated milliseconds.

Required eventually:

- CPU hierarchical trace
- GPU timestamp trace
- Draw and dispatch counts
- World-cell residency and churn
- Geometry and texture page faults
- Asset IO bandwidth
- Physics bodies, islands, and contacts
- Network bytes, entities, and events
- JS and WASM heap and GC
- Frame, input, and network capture packages for replay

The editor profiler panel (`editor/profiler/`) is not built. Do not print a frame time the clock did not measure. `addTime` throws on a negative or non-finite sample so a caller cannot record a guess by accident.

Performance PRs include raw before/after data. No unrelated refactor inside the comparison. Results go under [../benchmarks/README.md](../benchmarks/README.md).
