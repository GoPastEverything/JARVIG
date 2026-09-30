# Content addressing

Derived products are addressed by [derived-data-cache.md](derived-data-cache.md). Source assets are addressed by stable ids in project data (UUIDs on materials and projects, entity UUIDs in scenes). Paths are an authoring convenience. The runtime handle id is not a disk path.

Implications already in force:

- Two different import settings of the same file are different derived products.
- A dedicated-server target is a different key from `webgpu-web`, so server cooks cannot accidentally reuse a GPU payload.
- Feature flags are part of the key. An experiment cannot share derived data with the production flag set and then call the result comparable.

Source control stores source assets and schemas. It does not store the addressed products except golden fixtures.
