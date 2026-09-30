# Streaming

Streaming is a universal contract: geometry, textures, entities, navigation, audio, physics proxies, and network state expose residency and relevance. Phase 0 has the asset residency enum and a priority function. It does not stream bytes.

## Priority heuristic

`streamingPriority` in `@jarvig/world` is the initial function. Weights are tuning, not a content decision and not an ADR:

```text
distanceTerm   = 1 / (1 + distance)
directionTerm  = (viewAlignment + 1) / 2
base           = distanceTerm * 0.5 + directionTerm * 0.2 + gameplayImportance * 0.3
priority       = base * (1 - memoryPressure * 0.5)
```

Inputs are clamped to the documented ranges. Higher means load sooner. Tests check that nearer beats farther, importance can beat a far cell, and memory pressure lowers priority.

The founding formula also includes camera direction, player and vehicle velocity, portal visibility, and network relevance. Those inputs are not in the Phase 0 signature. Add them when a caller exists, and extend the tests. Do not silently change the meaning of the current inputs.

## Asset residency

Handles in `@jarvig/assets` are `unresolved -> loading -> resident -> evicted`, with `failed` from unresolved, loading, or evicted, and `failed -> loading` for retry. Skipping states throws. Runtime code uses handles, not editor file paths.

## Render and texture pages

Virtual geometry pages and virtual texture pages are research and later milestones. They must report residency through the same ideas: a budget, a fallback, and an eviction order. See [../rendering/virtual-geometry.md](../rendering/virtual-geometry.md) and [../materials/virtual-textures.md](../materials/virtual-textures.md).
