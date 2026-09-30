# ADR-0049 — Component membership stays on the entity, and payloads stay in their subsystems

Status: Accepted
Date: 2026-09-24

## Context

ADR-0048 is accepted. An actor is an entity. The component stack is membership over the mesh, light, probe, and environment records that already exist. An entity still has one payload besides Transform. That restriction was intentional. It is not the storage model for a game.

This decision says how a later milestone may put many component types on one `EntityUuid` without a second actor identity, without copying subsystem payloads onto the entity, and without letting the renderer walk the registry. It does not implement that milestone. `jarvig.level` version 1 is not changed by accepting this document.

## Decision

### Membership

Entity A owns Transform, Mesh Renderer, Camera, and Script because the entity record for A's `EntityUuid` holds an ordered membership list. Each entry is:

```text
ComponentMembership
    type: TypeId or, if the type is not loaded, its canonical name
    slot: u32
    subsystem: optional runtime id (LightId, ProbeId, MeshId, ObjectId, FrameId)
    preserved: optional unread document body
```

The list is the stack. It lives on the entity, next to the name and parent. It is not a new registry, not an actor id, and not a component uuid.

The light's candela, the probe's radius, and the mesh's material factors stay in the lists that already own them. The membership entry points at that record. It does not copy the payload.

A player, once this exists, is one entity:

```text
Player                         EntityUuid
    Transform                  slot 0    FrameId
    Mesh Renderer              slot 0    ObjectId, MeshId
    Character Controller       slot 0    its own record, later
    Camera                     slot 0    its own record, later
    Audio Listener             slot 0
    Input                      slot 0
    Script                     slot 0
    Script                     slot 1
    Health                     slot 0
```

### Address

A component has no uuid.

A singleton component is addressed as:

```text
EntityUuid + TypeId
```

A repeatable component is addressed as:

```text
EntityUuid + TypeId + slot
```

`slot` is assigned when the component is added. It is not a uuid and it is not "the nth remaining item." Removing slot 1 does not renumber slot 2. The next add on that type receives `max(slot) + 1`. A stale slot is `NotFound`. It must not silently bind to a different audio source or script.

Singleton types always use slot 0. Callers may omit the slot. Passing any other slot is `InvalidOperation`.

### Multiplicity is per type

There is no global "one of everything" rule, and there is no global "unlimited of everything" rule. Each component type declares one of:

| Policy | Meaning | First types |
| --- | --- | --- |
| `One` | At most one on the entity. Slot is always 0. | Transform, Mesh Renderer, Directional Light, Point Light, Spot Light, Reflection Probe, Camera, Character Controller, Health, Input, Audio Listener, World Settings |
| `Many` | More than one, each with its own slot. An optional max. | Script, Audio Source |

Two point lights are still two actors, because Point Light is `One`. Four audio sources on one actor are four slots, because Audio Source is `Many`. Eight scripts are eight slots. A second Camera on the same entity is rejected.

`Many` without a declared max is allowed. A type that needs a cap declares it on the type, not in the command.

### Add and remove are absolute commands

Today's authoring commands set absolute state. There is no undo stack in the world. Cancel sends the previous value again. Add and remove stay in that model.

```text
AddComponent    { entity: EntityUuid, type: TypeId, slot: Option<u32> }
RemoveComponent { entity: EntityUuid, type: TypeId, slot: u32 }
```

`AddComponent` checks multiplicity and dependencies, creates the subsystem record, and appends the membership entry. `RemoveComponent` drops that entry and destroys that subsystem record in the same command. The mesh asset and the material instances stay, as in ADR-0041.

An editor undo entry, when one exists, stores the inverse command and the body needed to restore the record. The world does not grow a history. Undo of an add is a remove of that slot. Undo of a remove is an add that writes the saved body back into a new subsystem record and reuses the same slot. Reusing the slot is the exception to `max+1`, and it happens only for that restore.

Failures stay `NotFound`, `Unsupported`, `ProtectedEntity`, and `InvalidOperation`. Adding a second `One` type is `InvalidOperation`. Removing Transform while a dependent component remains is `InvalidOperation`. Removing an unknown preserved component drops that entry only. It does not delete the actor.

### Dependencies are type metadata

Requires and rejects are declared with the type. Commands consult that declaration. They do not grow a private list of special cases.

| Type | Requires | Rejects | Multiplicity |
| --- | --- | --- | --- |
| Transform | | | One |
| Mesh Renderer | Transform | | One |
| Directional Light | Transform | | One |
| Point Light | Transform | | One |
| Spot Light | Transform | | One |
| Reflection Probe | Transform | | One |
| Camera | Transform | | One |
| World Settings | | Transform | One |

World Settings remains the singleton environment owner from ADR-0040. Adding Transform to it is `InvalidOperation`. Adding World Settings to an entity that already has Transform is `InvalidOperation`. Adding a Mesh Renderer before Transform does not invent a frame.

Later types register the same three facts. Character Controller requires Transform. A Script requires nothing unless its own type says otherwise.

### The level file, later, without touching version 1

Version 1 stays one payload plus Transform, in the shape it has now. Opening the Lighting Lab does not rewrite it. Saving it, while it still fits version 1, writes version 1.

A future version, not written by this ADR, stores components as an ordered list so slots and unknown types survive. A map keyed only by type name cannot hold four audio sources.

```json
{
  "uuid": "...",
  "name": "Player",
  "parent": null,
  "components": [
    { "type": "transform", "version": 1, "slot": 0, "body": {} },
    { "type": "mesh_renderer", "version": 1, "slot": 0, "body": {} },
    { "type": "camera", "version": 1, "slot": 0, "body": {} },
    { "type": "audio_source", "version": 1, "slot": 0, "body": {} },
    { "type": "audio_source", "version": 1, "slot": 1, "body": {} }
  ]
}
```

`body` for a known engine type is the same authored fields the subsystem already saves: translation, candela, radius, material factors. It is not a `LightId`, `ProbeId`, `MeshId`, `ObjectId`, `FrameId`, or GPU handle. Load creates the subsystem record and keeps the new runtime id only in memory.

A version 1 file loads through the loader that exists today. The format version increases only when a save must store a second payload, a repeatable slot, or a component version 1 cannot name. There is no silent upgrade.

### Unknown components are kept

If a component's type is not in the loaded registry, the actor still loads. The membership entry stores the canonical name, the document version, the slot, and the body bytes unread. Save writes those bytes back unchanged. The actor is not dropped, and the unknown body is not stripped.

Extraction ignores it. The inspector shows the name and "not loaded" and does not edit the body. A command cannot set its fields until the type is registered. Remove of that slot is allowed and discards the preserved body. Duplicate copies the preserved body onto the new actor's new `EntityUuid` with the same slot.

### The inspector stays on the type registry

`TypeInfo` gains, when this is implemented, the facts the editor needs. They are not a second form.

```text
TypeInfo
    id
    canonical name
    display name
    category
    version
    multiplicity
    requires
    rejects
    fields: FieldInfo
```

The inspector walks the membership list and, for each loaded type, builds rows from `FieldInfo`. Camera does not get its own window procedure. An unknown entry is the read-only row above. ADR-0022.

Systems ask `SceneWorld` for entities that own a type and receive `EntityUuid`, slot, and the subsystem id. They do not scan renderer objects. The renderer still receives only `RenderSceneSnapshot`.

### Duplicate and delete of the actor

Duplicate copies every membership entry, including preserved unknown bodies. Each known subsystem record follows ADR-0041: a new `LightId` or `ProbeId`, a shared `MeshId`, a new object, and no copied cubemap. Slots are copied. The new actor has a new `EntityUuid` and a new `EntityHandle`.

Delete removes every subsystem record for that entity, drops preserved bodies, then retires the entity. Children are not deleted. They reparent to World, as ADR-0041 already specifies.

## Alternatives Considered

- One archetype store that replaces the light, probe, and object lists. Rejected. Those lists are the working model. Membership points at them.
- A uuid per component. Rejected. The actor uuid is the authored identity. A component uuid would become a second thing to save and select.
- One global multiplicity rule. Rejected. Point Light is one per actor. Script and Audio Source are many. The type declares which.
- Reuse the lowest free slot after a remove. Rejected. A script holding slot 2 would start driving a different component.
- Drop an actor, or strip a component, when its plugin is missing. Rejected. The next save would destroy work the editor cannot see.
- A JSON object keyed by type name. Rejected. It cannot represent two audio sources or keep a stable slot.
- Let the renderer query the membership list during a frame. Rejected. ADR-0026 and ADR-0048.
- Implement add, remove, or the format bump in this ADR. Rejected. This document is the design. Code waits until it is accepted.

## Consequences

The first code, accepted with this document, stores the membership list on the entity for Transform, Mesh Renderer, the three lights, Reflection Probe, and World Settings. Add and remove are authoring commands. A second `One` component fails. Removing Transform while a dependent component remains fails. Query returns entities by type. Duplicate copies the list. Delete clears it with the subsystem records. Version 1 still cannot store a second payload: capture refuses that entity instead of dropping it. `Many`, unknown-component blobs, Camera records, scripting, physics, and Play are not in this slice.

## Supersedes

Nothing. ADR-0048's one-payload sentence stays in force until an implementation of this decision replaces it.

## Superseded By

Nothing.
