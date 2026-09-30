# Testing

| Class | Examples | Phase 0 |
| --- | --- | --- |
| Unit | Math, transforms, schemas, codecs, component storage | `tests/unit/foundation.test.ts` |
| Integration | Import to cook to load, editor undo, replication | Host boot and boundary tests in `tests/integration/hosts.test.ts` |
| Golden | Renderer images with tolerance; scene snapshots | `tests/golden/health-scene.json` only. No images |
| Replay | Recorded input or network reproduces state | Not started |
| Stress | Instance count, cell churn, memory pressure | Not started |
| Performance | Fixed scenes and regression thresholds | Not started. No numbers recorded |
| Soak | Hours-long simulation for leaks and desync | Not started |

Merge gates from the founding doc: typecheck and lint, unit and integration tests, forbidden-dependency checks, serialization compatibility, measurement for performance-sensitive changes, and an ADR or RFC for cross-cutting architecture changes.

`pnpm test` runs Vitest against TypeScript source via aliases. `pnpm smoke` runs the emitted JavaScript. Both are required before calling host boot done. A green unit test does not prove `dist/` works.

Do not weaken a test to match a bug. Do not skip a failing test. GPU image tests are not required for a change that never touches pixels, and a null-device run must not be described as a golden image.
