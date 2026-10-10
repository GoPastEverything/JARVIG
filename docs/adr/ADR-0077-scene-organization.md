# ADR-0077 — A level may organize objects into folders

Status: Proposed
Date: 2026-10-05

## Context

The editor can author solids, face materials, and named surface groups. The Outliner still listed every world root in registry order. A right-click had no command menu. Face, Edge, and Vertex had no box selection. View was one long list.

A folder is scene organization. It is not a geometric parent. ADR-0041 still owns the spatial parent. ADR-0018 keeps that list in the engine: the editor calls it, and the level stores it. ADR-0073 still owns the evaluated body. A folder does not reconstruct a solid.

## Decision

This ADR is Proposed. It is not accepted.

A level may store scene organization beside its entities. A folder has a stable id, a visible name, and an optional parent folder. Membership maps an entity uuid onto one folder. Lock is a list of entity uuids. Moving an object between folders does not change its world transform, its parent uuid, its body, its materials, or its surface groups. It does not append `intent`.

This record does not supersede ADR-0073. Accepting it would not accept ADR-0074, ADR-0075, or ADR-0076.

The slice recorded with this draft:

- A folder name is trimmed and at most 64 characters. An empty name or a control character is refused. Two folders may share a name. Ids start at 1 and are not reused. `next_folder` is omitted when it is 1. Deleting a folder still reserves that id, so a later folder writes `next_folder` when the counter is no longer 1.
- Folders nest. A cycle, a missing parent, a duplicate id, or id 0 is corrupt. Delete Folder does not delete entities. Child folders and members move to the deleted folder's parent, or drop to World when it had none.
- An entity that already has a spatial parent cannot join a folder. The pose stays, and the log says "That object has a geometric parent. A folder does not move it." Duplicate does not join the source folder, and the copy is not locked. World Settings may sit in a folder. It still refuses Duplicate and Delete.
- Folder create, rename, delete, and move call `revise()` and are one editor undo entry each. The labels are Create Folder, Rename Folder, Delete Folder, and Move To Folder. Undo restores that organization snapshot and does not rewrite entity records, transforms, materials, surface groups, or intent. Moving every selected object is that one Move To Folder entry. Lock calls `revise()` and is not an undo entry. None of these edits append intent. Renaming an entity through the prompt still uses the existing name command and stays one undo entry. Play refuses those organization edits: "Stop play before organizing the level." On load, members and locks whose entity is gone are dropped. Empty folders stay.
- The level key is `organization`. It is omitted when there are no folders, no members, no locks, and the next id is still 1. A plain level does not contain that word. Level format 1, block format 6, component version 1, and type registry 9 stay. There is no new component type and no C folder API.
- Lock refuses translation, rotation, scale, destroy, a gizmo drag, and the start of a modeling session. The log says "That object is locked." Rename, duplicate, and focus stay allowed. Hide in Editor is session visibility. It does not write `object.visible`, so it does not survive save and it does not change a mesh renderer's saved visible flag. Visible in Game remains that persistent flag. Editor Only is not this command. World Settings cannot be hidden. Hide in Editor and Lock are not undo entries.
- Object marquee stays in Auto and Object. Left to right requires every projected corner in front of the camera and inside the rectangle. Right to left selects when the on-screen box touches the rectangle. Four pixels or less is still a click. Ctrl toggles and wins over Shift. Shift adds. A plain drag replaces.
- In Face, Edge, and Vertex, a drag on empty space or on the one primary solid that has a display body selects that mode's authored elements. A press on a different solid stays a pick. An open modeling session does not arm the element box. Analytic bevel and inset without a display body stay an object marquee. The element box does not clear the entity selection. Id 0 is ignored. A different solid or element kind replaces the set. Render triangles, meshlets, Einstein patches, and microgeometry are not selectable elements.
- A right-click that stays under four pixels opens a menu of commands that already exist. A move past four pixels keeps look. Play swallows the viewport menu. The Outliner object menu is Rename, Duplicate, Delete, Focus, Hide in Editor or Show in Editor, Lock or Unlock, Create Folder, and Move To Folder. A folder row is Rename, Delete Folder, and Create Folder. Empty space and the World row are Create Folder, Block, and Plane. Object mode is Rename, Duplicate, Delete, Focus, Hide in Editor, Create Material, and Assign Material. Face mode is Extrude, Inset and Bevel only while those analytic commands still apply, Subdivide, Create Surface Group, Add To Group, Remove From Group, Assign Material, Select Connected, Boundary, Grow, and Shrink. Edge mode is Bevel, Split, Extrude Edge, Loop, Ring, and Select Connected. Vertex mode is Move Vertex and Select Connected. The menus call those commands. They do not add a second modeling kernel. Move Edge, Extrude Edge, Split Edge, and Move Vertex still refuse a set larger than one. A refusal does not cancel an open modeling session.
- View is Panels, Grid & Guides, Cameras, Rendering, Lighting, and Diagnostics. Rendering holds Quality, Meshlets, Microgeometry, and Geometry Truth. Meshlets holds Pipeline, and Leaf Truth stays there. Microgeometry holds Visualization. Lighting debug items are direct children of Lighting, including Environment Light, Recapture Reflection Probes, probe update, and probe resolution. No command was removed.

## Consequences

An author can build a tree such as Environment, with Landscape and Architecture under it, plus Gameplay, Lighting, and Props. Renames and folder membership survive save and reload. The world transform and the geometric identity stay. Box selection and the right-click menus operate on the selection the editor already had. Accepting this ADR does not accept ADR-0074, ADR-0075, or ADR-0076, and it does not make a folder a geometric parent.

Folder open or closed state is the optional `open_folders` key in `Saved/Editor/workspace.json`, schema `jarvig.editor-workspace` version 1, keyed by level uuid. It stores the folder ids the editor has seen and the subset that is expanded. A folder that list has not seen starts expanded. The key is omitted when empty. It is not in the level. Later, and not built here: drag-reparent of the spatial parent, Outliner shift-range, saved UVs, textures, material assets, and stylized shading.
