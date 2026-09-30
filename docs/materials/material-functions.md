# Material functions

Functions are reusable, versioned graph assets. Edits propagate by explicit dependency invalidation, not by search-and-replace.

JRV-0037. Not started.

A function change must invalidate derived data for dependents and only those dependents. That rule uses the same derived-data key as every other asset: the function's identity and version are inputs. See [../assets/derived-data-cache.md](../assets/derived-data-cache.md).

MaterialX import and export is a planned interoperability layer after the native graph and IR are stable. USD Shade, if ever evaluated, needs its own RFC. Neither is a reason to skip the native IR.
