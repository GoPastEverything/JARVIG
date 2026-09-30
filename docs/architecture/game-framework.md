# Game framework

ADR: [ADR-0048](../adr/ADR-0048-game-framework-above-the-world.md), [ADR-0049](../adr/ADR-0049-component-membership-is-not-a-second-registry.md), and [ADR-0050](../adr/ADR-0050-authored-world-is-source-runtime-world-is-instance.md) are accepted. Membership covers the components the world already has, including Camera. `Many` and unknown plugin components are specified and not built. `GameApplication` retains a level document and instantiates a `RuntimeWorld`. It does not play, and it does not own input, script, physics, audio, or networking.

The renderer draws a snapshot. A game is everything that decides what goes into the next world, and what survives when the level changes.

```text
Engine
    Game application          persistent for the running game
        World                 one simulation
            Level             one authored document
                Actors        entities with a component stack
                    Components
        Systems               read components, write their own records
            extract
        RenderSceneSnapshot
            Renderer
```

The arrow stops at the snapshot. The renderer does not call back into the registry.

## What already exists

| Piece | Where it lives now | This milestone |
| --- | --- | --- |
| `EntityUuid`, `EntityHandle`, parent | `EntityRegistry` | Unchanged |
| Transform | `FrameGraph` | Stack role `Transform` |
| Mesh renderer | `WorldObject` (`ObjectId`, `MeshId`) | Stack role, same ids |
| Directional, point, spot lights | `WorldLight` (`LightId`) | Stack role, same id |
| Reflection probe | `WorldProbe` (`ProbeId`) | Stack role, same id |
| World Settings | environment record | Stack role, no transform |
| Editor cameras | `SceneWorld` front and side cameras | Not actors. Not the runtime camera |
| Camera | `WorldCamera` (`CameraId`) on the actor Transform | Version 2 only when a level contains one |
| Project and level files | `jarvig.project`, `jarvig.level` | Version 1 stays version 1 |
| Game application | `GameApplication` | Survives level replacement |
| Runtime world | Instantiated `SceneWorld` | Discarded on stop |
| Extraction | `RenderSceneSnapshot` | Still the only renderer input |

`SceneWorld::component_stack` is the membership query. `entity_ownership` remains the single-payload view duplicate and delete already use. The stack does not replace those records.

## What is only named

These words are reserved so later work does not invent a second set. None of them are implemented. Camera actors and the runtime world are no longer in this list.

| Name | Lifetime | Not |
| --- | --- | --- |
| Game mode | Rules for one world | A renderer feature |
| Game state | Shared match data | The editor selection |
| Player | Logical player | An `EntityHandle` in the level file |
| Player controller | Input and network commands into the world | The Win32 message pump |
| Pawn | A controllable actor | A subclass of mesh |
| Character | A pawn with character movement | An inheritance tier |
| Camera controller | Chooses the active game view | `RenderView` itself |
| HUD | Player-facing UI | The editor dock |
| Save game | Runtime progress | The authored level |
| Actor asset | A template plus overrides | A level entity |

A level is one document: `Forest.jarviglevel`, `MainMenu.jarviglevel`. The game application is what opens the next one.

## Component order

The stack lists only what the entity owns, in this order:

1. Transform
2. Mesh Renderer
3. Directional Light
4. Point Light
5. Spot Light
6. Reflection Probe
7. Camera
8. World Settings

Camera is a real component. It is multiplicity One, requires Transform, and stores no second pose. The inspector prints the owned names in stack order. A level names its runtime camera with `world_settings.startup_camera` or it has none. The key is omitted when absent. Nothing selects the first Camera found.

## Boundaries that stay closed

- Do not put `EntityHandle`, `LightId`, `ProbeId`, `MeshId`, `ObjectId`, or a GPU handle in `.jarviglevel`.
- Do not let the renderer iterate actors.
- Do not save the game into the level, or the level into a save game.
- Do not treat `Saved/` as content, or a future `Intermediate/` as content.
- Do not add `scheme: jarvig.asset` until the asset database exists.
- Sublevels and world partition are a later streaming boundary. One file is one level until then.
- A dedicated server loads the same level and does not extract.

## Editor

The outliner rows stay actors. The row suffix stays the authoring class from the one payload. The inspector adds a read-only Components section so the stack is visible without a second form.

Play, Simulate, and Stop are not in the shell yet. The execution copy already exists behind `GameApplication`. When the shell grows a Play command, it must load the authored document into that application and stop by dropping the runtime world. It must not toggle play on the editor world.
