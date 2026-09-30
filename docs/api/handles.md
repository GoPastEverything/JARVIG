# Handles

External APIs take JARVIG handles. They do not take internal pointers.

Planned public identities:

| Handle | What it names |
| --- | --- |
| `WorldHandle` | One world in a session |
| `EntityHandle` | One entity in that world |
| `AssetHandle` | One asset identity |
| `MeshHandle` | One mesh resource |
| `TextureHandle` | One texture resource |
| `MaterialHandle` | One material or material instance |
| `PluginHandle` | One loaded plugin |
| `RequestHandle` | One async operation |

Names are the doctrine. The typedefs are not in a header yet.

## Shape

Where a slot can be freed and reused, the handle is generational:

```text
index        uint32
generation   uint32
```

Index `0` and generation `0` together are the null handle. After destroy, the slot's generation increments. A later occupant of the same index does not honor the old handle. The call returns `InvalidHandle`.

Use this for entities, GPU resources that escape the renderer, assets, editor selections, plugin instances, and async requests.

A handle that cannot be reused, such as a session-long type id, may be a plain integer. Say so on that type. Do not mix the two quietly.

## What stays inside the process

`BufferId`, `PipelineId`, and `RenderViewId` are Rust newtypes in the current crates. They are the internal API. They become a public generational handle only when a ticket exposes that resource across the ABI. Do not leak the `u64` newtype into a plugin header as a shortcut.

GPU backend objects never become handles. The handle names a JARVIG resource. The backend maps it privately.

## Threads

A handle value is trivially copyable. Using it is not automatically thread-safe. The thread class of the operation still applies. See [threading.md](threading.md).
