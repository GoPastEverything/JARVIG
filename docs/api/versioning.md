# Versioning

These counters move on their own. Bumping one does not bump the others.

| Stream | What it versions | Break means |
| --- | --- | --- |
| `JARVIG_API_VERSION` | The stable C ABI and the plugin entry | The loader rejects a module that cannot negotiate |
| C++ SDK | The wrapper headers | A recompile of the game, not a silent ABI break |
| Plugin API | The capability tables a category uses | That category fails to load |
| Type registry | Field ids and type ids | A migration for stored properties |
| World and asset schema | `jarvig.scene`, `jarvig.material`, project files | A loader migration, already the rule for those schemas |
| Material IR | Compiler output | A recook |
| Network protocol | Packets between processes | A version handshake |
| Cooked content | Derived bytes | A cache miss, not a crash |

## ABI rules

- `JARVIG_API_VERSION` is a `uint32_t`. The engine and the module both say what they implement.
- Each long-lived struct and each function table also carries `struct_size` and `api_version`. One global number does not replace that. See [c-abi.md](c-abi.md).
- Within a major version, codes, handle layouts, and existing struct fields do not change meaning.
- New functions and new error codes are append-only.
- Removing or renumbering a field is a new major version.
- A deprecated function stays callable for at least one minor version after the docs mark it, unless it is a safety defect. The replacement is named in the same note.
- A plugin that does not understand the engine's version fails cleanly with `JARVIG_VERSION_MISMATCH` and a log line. It does not run halfway.

The manifest of a plugin carries `api_version`, `plugin_version`, and the capability list it needs. Missing a required capability is a failed load, not a null crash later.

Serialized schemas keep the ids they already have (`jarvig.scene/v1`, `jarvig.material/v1`). This page does not invent a second version for those files.

`native/jarvig_core/include/jarvig_core.h` is a bootstrap slice, not the SDK, and it is not version-negotiated. The first header under `sdk/c/include/jarvig/` defines `JARVIG_API_VERSION`. Do not pretend the clock exports are that header.
