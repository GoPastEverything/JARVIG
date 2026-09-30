# Benchmarks

RFC-0001's 2026-09-27 correctness and runtime harness is recorded under [rfc-0001/2026-09-27](rfc-0001/2026-09-27/notes.md). Those numbers are the acceptance evidence. They are not production targets.

Do not add a number to this directory unless a command in the repository produced it and the raw log is stored next to the summary. Invented Nanite comparisons, frame times, and VRAM figures are a defect, not a draft.

When a result exists, put it in a subdirectory named after the RFC or ticket:

```text
docs/benchmarks/rfc-0001/2026-09-22/
  command.txt
  raw.json
  notes.md
```

`notes.md` states the machine, GPU, driver, commit, scene, and what failed. A pass that omits the failures of the same run is incomplete.

Initial engineering budgets in [../architecture/engine-runtime.md](../architecture/engine-runtime.md) are targets from the founding doc. They are not measurements.
