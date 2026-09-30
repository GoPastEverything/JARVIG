# Derived data cache

ADR: [ADR-0012](../adr/ADR-0012-derived-data-cache.md).

```text
DerivedKey = HASH(sourceBytes, importSettings, importerVersion,
                   engineFormatVersion, targetPlatform, featureFlags)
```

`derivedKey` in `@jarvig/assets` is SHA-256 of a canonical JSON payload. Object key order does not change the hash. Changing source bytes, settings, importer version, engine format version, target platform, or feature flags does. `undefined` and non-finite numbers throw so a key cannot silently drop a field.

The hash input uses base64 for bytes and stable JSON for the rest. It is a cache identity, not an authentication signature. Callers must still treat collisions as catastrophic and not worth handling specially at this size.

There is no on-disk cache directory yet. `DerivedDataCache/` is gitignored for when the cooker writes it. JRV-0021's exit (invalidation when source, settings, or tool version changes) is met by the function and its tests. The disk cache is part of JRV-0023.
