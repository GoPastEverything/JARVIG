# ADR-0059 — A character asset, a player definition, and a player start are different objects

Status: Accepted. ADR-0060 clarifies that the character path may be empty until a character is chosen. The field is still not an absolute path, and this version still has no controller, camera, or input.
Date: 2026-10-01

## Context

Base and Base Male are `jarvig.character` version 1. The startup level instantiated both bodies, so Level showed two dolls because the files existed. Character mode was another view of that same level when the level was joints plus world settings.

A character is the body and the joints. A player is gameplay configuration that uses one character. A level places that player with a spawn, not by embedding the body. ADR-0048 already said a future player is an entity whose stack contains the components it needs, and that JARVIG does not grow `Actor -> Character -> Player`.

This decision supersedes two sentences and nothing else. ADR-0053 still says the pose is the local frame and that FBX is not an importer. ADR-0054 still says the Character Editor reads and writes that same `SpatialFrame` and `Joint`. ADR-0055 still says the character file is rigid parts, wide-default limits, null collision, and a null animation set. ADR-0056 still says sockets sit on the ball, the contact parent is the 0.02 m span, L is world −X, the doll faces −Z, and opening a project or a level opens the level editor. ADR-0057 still says a joint-debug marker is a pick target only in the Character Editor. ADR-0058 still says Land Mode is one world. Its sentence that Land works "in the same way the Character Editor is" now means Land stays on the authored world. Character isolation is this decision.

## Decision

`jarvig.player` version 1 stores a name and one project-relative `*.jarvigcharacter` path. Controller, input, camera, and movement are not fields in this version. The project settings strings `JARVIG.PlayerController`, `JARVIG.DefaultFreeFlyPawn`, and `JARVIG.Default` stay what they were. They are not this document.

A Player Start is a component on an ordinary entity. It stores the project-relative `*.jarvigplayer` path and a preview flag. Position and orientation are the entity's existing transform. Forward is local −Z. The level writes format version 5 only when a Player Start is present. Versions 1 through 4 still load. Version 6 is rejected. There is no second skeleton and no hard-coded player, enemy, or NPC type.

`TYPE_PLAYER_START` is type id 15. `TYPE_REGISTRY_VERSION` is 7. The level fields are `FIELD_PLAYER_DEFINITION` (88) and `FIELD_PREVIEW_CHARACTER` (89). ADR-0058's record that the registry version is 6 stays the historical note for terrain.

The editor draws a Player Start as an overlay gizmo. It is not a mesh. Preview Character, when set, instantiates the referenced character under that entity so the spawn transform places it. Those instances are derived. They are absent from the outliner and from the saved level, and their shadow casting is muted. The shadow budget stays 16 casters. Preview defaults off. Several Player Start entities are allowed.

Level shows the authored entities. Character assets stay in the Content Browser until something opens or previews them. Dragging a character asset does not spawn. Dragging a player definition does not spawn. Double-clicking a character opens that asset. Double-clicking a player definition reports which character it uses and does not place a body.

Character mode on a level that already contains a rig stays an in-place workspace over that rig. That is the primitive mannequin. Character mode on a level with no rig opens the character asset that was last opened, in a preview scene. The authored level is kept aside and restored on the way back. Save and autosave during that preview write nothing, including the character file. Joint edits in the preview are discarded on return. A level with no rig and no opened character asset stays in Level.

The Character Editor's tree has two views of the same asset. Joints is the contact hierarchy. View > Mesh Parts lists imported mesh parts and does not pretend those parents are bones. Skin, animation, and physics are not shown. The same editor is the place for any articulated asset. There is no separate player-rig editor.

This partially supersedes only this sentence of ADR-0055: "The startup level `Content/Levels/Base.jarviglevel` still holds both bodies, so the project opens on the pair."

This partially supersedes only this sentence of ADR-0056: "The Character Editor is entered on purpose: View > Character Editor, when the open level is joints plus world settings, or a double-click of a character asset."

## Consequences

`Content/Levels/Base.jarviglevel` is World Settings plus one Player Start. `Content/Players/DefaultPlayer.jarvigplayer` references `Content/Characters/Base Male.jarvigcharacter`. Preview is off, so the bodies are not in the level. Switching that reference changes the optional preview. Returning to Level leaves the level unchanged.

JRV-0092 visual acceptance stays open. JRV-0091 stays unaccepted. JRV-0089 stays unaccepted. JRV-0090 stays unstarted. Animation playback, IK, ragdoll, physics, skinning, and Einstein terrain generation stay out. The contact parent is still the 0.02 m span. The shadow pass still updates 16 casters. FBX stays outside the engine. No new C ABI was added.
