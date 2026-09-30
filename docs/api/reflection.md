# Type registry

One description of a type. Everything else is a view.

```text
                JARVIG TYPE REGISTRY
                         |
     +------------+------+------+-------------+
     |            |             |             |
 Inspector   Serialize      Scripts       Network
     |            |             |             |
     +------------+-------------+-------------+
                         |
                   Sequencer, undo, docs
```

There is not a separate editor Transform, script Transform, and network Transform. There is Transform. Those systems read its fields.

## What exists now

`jarvig_core` has schema version 2. `TypeId` and `FieldId` are JARVIG constants. They are not `std::any::TypeId`, not Win32 control ids, and not byte offsets. The types are `entity`, `spatial_frame`, `directional_light`, `point_light`, `spot_light`, `sphere_reflection_probe`, `environment`, and `mesh_renderer`. Reads go through `SceneWorld::inspect_entity`. Writes go through `EngineSession::execute_authoring` (`SetProperty`). A read-only field rejects the command. The value is a `PropertyValue`, not JSON and not a GPU type. `Bool` and `Float64` are property values. A text box does not send them as strings.

The inspector is a view of that snapshot. It does not own a second schema. This is not the stable C ABI. JRV-0056 still owns that publication. Plugins still use accessors when that ABI exists. A byte offset is not the property contract.

Quaternion orientation stays the engine value. The inspector shows it and does not edit it. Local translation is binary64 meters. Name is editable. UUID and parent are visible and not editable. Light color, intensity, range, cone angles, and enabled are editable on the entity that owns that light. Probe radius, priority, intensity, and enabled are editable. Update mode, capture state, and resolution are read-only. Environment upper color, lower color, intensity, and enabled are editable on World Settings only. The mesh renderer surface and material binding are read-only strings.

## Records

The registry will be able to name:

| Record | Role |
| --- | --- |
| `TypeId` | Stable id for a type |
| `TypeInfo` | Name, size, alignment, kind |
| `FieldInfo` | Name, type, offset, flags |
| `PropertyFlags` | Serialized, replicated, editable, read-only, hidden |
| `ComponentInfo` | How a component attaches to an entity |
| `EnumInfo` | Variants and their values |

Field offsets are the in-memory layout of the engine type. The ABI does not promise that layout to plugins. Plugins use accessors. The registry is how the engine, the inspector, and the serializer agree.

A component attribute in Rust, conceptually `#[jarvig_component]`, is one way to emit these records. The records are the contract. The attribute is an implementation choice and is not required to exist before the first hand-registered type.

## What uses it

- Inspector rows and property editors
- World and prefab serialization
- Undo of a property edit
- Script bindings
- Replication of fields marked for the network
- Sequencer and animation paths such as `Transform.position`
- Generated docs

A system that needs a private copy of the field list is a bug.

## What is not built

No attribute macro, no generated bindings, no serializer, no replication, and no public C export of the registry. The table above still describes the fuller record. The bootstrap registry does not publish a memory offset. A component that is later saved or shown registers here instead of growing a parallel struct in the editor.
