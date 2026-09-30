# ADR-0048 — A game framework sits above the world, not inside the renderer

Status: Accepted
Date: 2026-09-24

## Context

JARVIG can author entities, lights, probes, meshes, materials, projects, and levels, save them, reload them, and render them. That is an editor and a renderer. It is not yet the layer a game is built on.

ADR-0009 already rejected class inheritance as the extension model. ADR-0026 already says the renderer consumes a snapshot and does not query the world. ADR-0033 already says the Perspective camera is editor session state. ADR-0040 and ADR-0041 already give every placeable object one `EntityUuid` and keep `LightId`, `ProbeId`, `MeshId`, and `ObjectId` inside their subsystems. ADR-0045 already makes `.jarvigproject` and `.jarviglevel` the documents, and it refuses `jarvig.asset` until an asset database exists.

The missing decision is the lifetime and the vocabulary above that world: project, game application, world, level, actor, component, and the systems that tick them. Copying Unreal's class tree would weld gameplay to the renderer. Inventing a second entity store would throw away the lighting, probe, and level work.

## Decision

The renderer stays downstream of extraction.

```text
Game or editor world
        |
        | extract
        v
RenderSceneSnapshot
        |
        v
Renderer
```

The renderer never reads the entity registry, a script, or a player. A dedicated server runs the world with no renderer, no editor, and no GPU.

### Lifetimes

These lifetimes are different objects. One must not be stored inside another.

| Object | Lives for | Destroyed by | Does not own |
| --- | --- | --- | --- |
| Engine | Process | Engine shutdown | A particular game |
| Game application | One running game | Project close | The current level's actors |
| World | One simulation | World destroyed | The editor session |
| Level | One loaded document | Level unload or replace | The game application |
| Actor | While its level owns it | Level unload or explicit destroy | A subsystem id |

Changing levels replaces the world's level content. It does not destroy the game application. A future logged-in player, settings, network session, or save-game manager hangs off the game application, not off `Forest.jarviglevel`.

Editor Play, when it exists, clones the editor world into a runtime world. Stop destroys that runtime world and leaves the editor world as it was. That clone is not this milestone. The Perspective camera stays editor state and is not copied in as the game camera.

### Actor, entity, component

`Entity` remains the internal record: `EntityUuid`, `EntityHandle`, name, and parent. An actor is an entity that exists as an authored world object. The outliner lists actors, not components and not subsystem ids.

A component is data. A system does the work. JARVIG does not grow `Actor -> Character -> Soldier -> PlayerSoldier`. A future player is an entity whose stack contains the components it needs.

The component stack is membership plus the subsystem id that record already has. Payloads stay where they are.

```text
Point light actor
    Transform
    PointLightComponent  -> LightId -> light list

Mesh actor
    Transform
    MeshRendererComponent -> ObjectId, MeshId -> object list

Reflection probe actor
    Transform
    ReflectionProbeComponent -> ProbeId -> probe list
```

`CameraComponent` is a named component for a later authored camera. It is not the Perspective camera and it is not serialized in `jarvig.level` version 1. No bootstrap or Lighting Lab entity has one.

World Settings stays the singleton environment owner. It has no transform. It is not a light.

One entity still has one payload besides Transform and World Settings. Adding a second payload on the same entity stays `Unsupported` until a later milestone says otherwise. ADR-0041.

### What this milestone implements

`SceneWorld::component_stack` is the composition query. It does not allocate a second store. Order is fixed: Transform, Mesh Renderer, Directional Light, Point Light, Spot Light, Reflection Probe, Camera, World Settings. Only roles the entity owns are present. Each binding carries `ObjectId`, `MeshId`, `LightId`, or `ProbeId` when that role owns one, and nothing otherwise.

The inspector shows that stack as a read-only Components section after the existing fields. The outliner is unchanged. Level JSON is unchanged. Extraction is unchanged.

### What stays later

Not this milestone, and not a reason to rewrite lighting or the level file:

- Game application, game mode, game state, player, player controller, pawn, character, HUD, save game
- Script modules and BeginPlay / EndPlay
- Editor world versus runtime world (Play In Editor)
- Physics, animation, navigation, audio, input maps, networking
- Actor assets (`.jarvigactor`) and overrides
- Asset database. `jarvig.asset` stays rejected. Material set names stay level data. ADR-0045.
- Sublevels and world partition. A level is one document until a streaming ADR says otherwise.
- Save-game bytes. They are not the authored `.jarviglevel`.

Tick phases, when a scheduler exists, are explicit and are not the render loop. A server may run gameplay, physics, and networking with no extraction. The names to use later are: project loaded, world created, level loaded, actor created, begin play, pre-physics, physics, post-physics, update, late update, render extraction, end play, actor destroyed, level unloaded, world destroyed, project closed. Rendering is not the gameplay clock.

### Project layout

The project document already names content, config, saved data, and the startup level. The intended tree, not created by this milestone, is:

```text
MyGame/
    MyGame.jarvigproject
    Content/          authored data
    Source/           game logic, later
    Config/           input, rendering, physics, game
    Plugins/
    Saved/            editor state, autosaves, logs; not required to rebuild
    Intermediate/     disposable generated data
```

`Saved/` is already used that way. `Intermediate/` does not exist yet and must not be treated as content.

## Alternatives Considered

- Unreal-style inheritance as the gameplay API. Rejected. ADR-0009. Composition is the extension model.
- Store component payloads on the entity and delete the light, probe, and object lists. Rejected. Those lists are the working lighting and probe model. The stack wraps them.
- Bump `jarvig.level` and add Camera, Script, and prefab records now. Rejected. ADR-0045. A new version waits until a milestone actually writes those fields.
- Make the Perspective camera a `CameraComponent` so Play has a view. Rejected. ADR-0033. An authored camera is a different object.
- Start Play In Editor, physics, or a script VM in the same change as the stack. Rejected. The stack has to be reviewable on its own.

## Consequences

`docs/architecture/game-framework.md` is the map. `docs/architecture/entity-identity.md` records that the stack is membership, not a new id. The inspector's Components section is read-only. Selecting an actor does not change the snapshot.

Accepted on 2026-09-24 after the stack was checked, not enlarged. A mesh is Transform and Mesh Renderer. A point light, a spot light, and a reflection probe are Transform plus that one payload, and the payload still carries its subsystem id. World Settings is only World Settings. No actor reports Camera. The outliner lists actors. Inspecting them does not change extraction. The committed Lighting Lab still round-trips as `jarvig.level` version 1. The server profile still does not extract. The next design is ADR-0049. Do not implement it, and do not start scripting, physics, networking, or Play, until that design is accepted.

## Supersedes

Nothing. ADR-0009, ADR-0026, ADR-0033, ADR-0040, ADR-0041, and ADR-0045 still stand.

## Superseded By

Nothing.
