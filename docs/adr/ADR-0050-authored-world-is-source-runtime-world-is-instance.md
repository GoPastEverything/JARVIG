# ADR-0050 — The authored world is source data. The runtime world is an execution instance

Status: Accepted
Date: 2026-09-25

## Context

ADR-0048 placed a game application above the world and said a level change must not destroy that application. ADR-0049 put component membership on the entity. The Camera component now exists: an actor is Transform plus Camera, the pose is that Transform, and the Perspective camera remains editor session state.

What is still missing is the object a running game actually is. Today the editor's `SceneWorld` is both the document being saved and the world being simulated. A later Play button cannot toggle a flag on that world. Knocking over a chair, spawning an actor, or deleting one would then be a save.

## Decision

The authored world is source data. The runtime world is an execution instance of that source. Mutations of the instance do not write back. There is no "apply runtime changes" path in this decision.

```text
JarvigProject
    |
    v
GameApplication          survives level replacement
    |
    v
RuntimeWorld             instantiated copy, destroyed on stop
    |
    v
extract
    |
    v
RenderSceneSnapshot
    |
    v
Renderer
```

The renderer still does not query actors, components, or `SceneWorld`. A dedicated server can construct a `GameApplication` and a `RuntimeWorld` and never extract and never create a device.

### What the application owns

`GameApplication` owns the retained level document, the current `RuntimeWorld` while one exists, the lifecycle phase, the simulation clock, and the explicit runtime-camera reference.

It does not own renderer internals, material compilation, GPU resources, editor selection, the inspector, the dock, or the Perspective camera.

The clock lives on the application. Stop destroys the runtime world and does not reset the clock. The next start instantiates a new world whose own simulation tick begins again at zero.

### Lifecycle

The phase is an enum. These names are not editor menu labels.

| Phase | Resting | Meaning |
| --- | --- | --- |
| `Created` | yes | No document |
| `Loading` | no | A document is being accepted |
| `Ready` | yes | A document is retained. No runtime world |
| `Starting` | no | A runtime world is being instantiated |
| `Running` | yes | The instance exists and `tick` advances the clock |
| `Paused` | yes | The instance exists. `tick` does not advance |
| `Stopping` | no | The instance is being dropped |
| `Stopped` | yes | The document remains. The instance is gone |

`load` is legal from `Created`, `Ready`, and `Stopped`. It retains the document and does not instantiate. `start` is legal from `Ready` and `Stopped`. It instantiates a fresh world from the retained document. `stop` is legal from `Running` and `Paused`. It drops that world. `start` again instantiates another fresh world from the same document, so a destroyed or spawned runtime actor is gone.

`load` while `Running` or `Paused` is refused. The host stops first, then loads. The same `GameApplication` value performs that replacement. A level is content. It is not the application.

`Loading`, `Starting`, and `Stopping` occur inside the synchronous call. A successful call does not return while still in a transient phase. A failed instantiate leaves the previous resting phase and no runtime world.

### The copy

Instantiation is `LevelDocument::instantiate`. Entity UUIDs, names, parents, transforms, and component payloads are the authored ones. `EntityHandle`, `ObjectId`, `LightId`, `ProbeId`, `CameraId`, and `FrameId` are new. Material instances are not created by instantiation. The engine binds those only for a profile that renders, and this application does not bind them.

`SceneWorld::new_session` still builds the two non-entity view frames every world has today. They are not actors and they are not the runtime camera. This decision does not remove them and does not teach the renderer to read them as the game view.

### Active runtime camera

The choice is an explicit reference, or there is no active camera. Nothing scans for the first `Camera` component.

The reference is `world_settings.startup_camera`, an optional `EntityUuid` on the level document. It is omitted from JSON when absent, so a version-1 Lighting Lab file does not change and does not gain a key. A document that names one must already be version 2, because the target has a Camera component. A reference to a missing entity, or to an entity with no Camera, is corrupt. A disabled Camera stays in the document and is not eligible: the active camera is then none. Another enabled Camera on a different actor is not a fallback.

The Perspective camera cannot be named. It has no `EntityUuid`.

### What this does not build

No Play menu behavior, pause button, possess, or eject. No input, character controller, script runtime, physics, audio, networking, prefabs, player controller, or gameplay components. Those names stay reserved. This milestone does not add empty systems so the names can be said to exist.

Tick order, when those systems exist, is not the render loop: input, gameplay, physics, then extraction. Extraction remains optional. The server skips it.

## Alternatives Considered

- Simulate by mutating the editor `SceneWorld` and restoring from undo. Rejected. Undo is not the boundary between authoring and play, and a dedicated server has no editor undo stack.
- Pick the first enabled Camera, or the highest priority. Rejected. Insertion order and priority are not an authored choice. An explicit reference is.
- Store a second camera transform on the application. Rejected. The Camera component already uses the actor Transform.
- Bump every `.jarviglevel` to a new version to hold the reference. Rejected. The key is absent unless a level actually names a startup camera.
- Wire the editor Play command in the same change. Rejected. The lifecycle has to be true before the shell calls it.

## Consequences

`GameApplication` and `RuntimeWorld` live in the core, beside the level document, not in the renderer and not in the editor shell. `docs/architecture/game-framework.md` names this lifetime. `docs/architecture/play-in-editor.md` still refuses to implement Play by toggling the authoring world. The shell's Play command stays unwired.

Accepted with the lifecycle, not with gameplay. Lighting Lab still loads as version 1. A runtime destroy or spawn does not change the authored world. Stop drops the instance. The next start is a new instance. The server path still does not extract or create a device.

## Supersedes

Nothing. ADR-0033, ADR-0045, ADR-0048, and ADR-0049 still stand.

## Superseded By

Nothing.
