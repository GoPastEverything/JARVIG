# Commands and services

## Two ways in

Runtime code calls the engine API:

```text
World.CreateEntity()
Material.SetParameter()
Asset.Request()
```

Authoring tools submit a command:

```text
ImportAsset
CookProject
BuildTarget
CreateMaterialInstance
RebuildShaders
ValidateWorld
SetProperty
SaveWorld
```

The command calls the same engine API. It does not reimplement it.

The first native command is `AuthoringCommand::SetProperty` on `EngineSession::execute_authoring`. It sets an absolute name or local translation for an `EntityUuid`. The same value does not bump the world revision. Read-only fields, a bad type, a missing entity, and a non-finite translation fail as a result, not a panic. Undo, transactions, and a CLI spelling are not built. A later gizmo drag should reuse this command inside a begin/update/commit transaction.

```text
User moves an entity
        |
        v
SetProperty command
        |
        v
Engine API
        |
        v
World
```

A command that changes authoring state can `execute`, `undo`, and `redo`, and it can be logged. Validation runs before execute. A command that cannot undo, such as "cook the project," says so. It is still one command the CLI and the editor share.

Not every write is a command. The fixed step, the dedicated server, and gameplay `OnUpdate` call the runtime API. Wrapping those in an undo stack would be a mistake. Play-In-Editor already throws runtime mutations away on Stop. That policy stays. See [../architecture/play-in-editor.md](../architecture/play-in-editor.md).

The editor's existing rule is the same idea: a panel mutation is a command, not a poke at a runtime object. See [../architecture/editor-runtime.md](../architecture/editor-runtime.md).

## Services

Capabilities are reached from a session, not from a process-global singleton that only one host can own.

| Service | Owns |
| --- | --- |
| `WorldService` | Worlds, entities, frame poses |
| `AssetService` | Import, identity, requests |
| `MaterialService` | Materials and instances |
| `RenderService` | Views, draws, GPU resources |
| `PhysicsService` | Simulation bodies |
| `AudioService` | Sound |
| `NetworkService` | Replication and connections |

These are names for the doctrine. The crates are not created by this ticket.

A plugin receives the services its category lists. It does not receive all of them by default. Two sessions in one process, such as a test editor and a test server, do not share a hidden global world.

## Events

Engine events are typed. An event has an id in the type registry and a payload the registry describes. Subscribers are the editor, plugins, and game modules that asked for that type.

A string name may be a debug label. It is not the only way to subscribe. An untyped bus of free strings is not the architecture.
