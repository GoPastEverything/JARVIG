# Worlds

Level, Land, and Character are views. They do not load a second world. The toolbar words and **View > Land Mode** and **View > Character Editor** select the same modes.

| Mode | What you are editing |
| --- | --- |
| Level | The authored level. Terrain grid and joint helpers stay off. |
| Land | The terrain workspace. The grid follows the ground. Characters, props, and gameplay actors stay hidden until you turn on Show Full Level. |
| Character | One character asset in its own preview, or a rig that already lives in the level. |

The mode is stored in `Saved/Editor/workspace.json`. It is not stored in the level. Returning from a character asset restores the authored level.

## Templates again

Blank, First Person, and Third Person open in Level when that workspace file is absent. Landscape opens in Land. Landscape does not create a heightfield for you. The level is still World Settings until you create terrain.

First Person and Third Person add one Player Start and a player definition with an empty character path. Preview Character can show a body later, when a character is referenced. The ghost is not saved as a level mesh. A Player Start is a spawn marker.

## Create terrain

1. Open a Landscape project, or switch an existing project to Land.
2. When the level has no terrain, the outliner lists **Create Terrain** and the inspector shows Width, Depth, Meters Per Vertex, Chunk Size, and Height.
3. Press **Create Terrain**.

The default authoring size is 512 by 512 m, 1 m between vertices, 64 m chunks, and height 0. Those numbers are a draft until Create Terrain runs. The level then stores the height samples, not the chunk meshes. Save, and reload rebuilds the meshes.

One terrain actor is one entity: a transform, plus a heightfield in terrain-local meters. X and Z run across the patch. Y is the height. Gameplay queries read that height. There is no physics solver driving objects over it.

## Sculpt

Choose the tool in the Land outliner.

| Tool | Drag |
| --- | --- |
| Sculpt | Left mouse raises. Shift and left mouse lowers. |
| Smooth | Moves a sample toward its neighbors. |
| Flatten | Pulls height toward the height under the cursor at mouse-down. That reference stays for the stroke. |
| Paint Layers | Writes a stored class on the samples. Height stays put. |

The wheel over the terrain changes the brush radius while a brush tool is active. Alt+wheel still dollies. The wheel still changes fly speed when the pointer is off the terrain or you are looking, panning, or orbiting.

Radius, strength, and falloff are in the inspector while Land is showing the terrain. Strength is meters at the center of a sculpt. Falloff is smooth or linear. Every write clamps to the height minimum and maximum. A stroke is one undo entry.

The editing grid is painted on the terrain triangles. It is not an entity and it is not saved. Minor lines fade before major lines. At the horizon the grid is gone and the heightfield is still there. The brush ring shows the radius and the falloff. Play hides the grid and refuses the brush. Sculpt, smooth, flatten, and paint refuse to run outside Land.

**Show Full Level** is an inspector toggle in Land. It shows characters, props, and gameplay actors again. Level itself always shows the authored world, with the terrain grid off.

Chunk meshes do not cast shadows. Changing grid spacing does not rebuild a chunk. The picture of a sculpted chunk is rebuilt on the background job queue. Create Terrain itself still builds the first chunks on the editor thread.

The outliner row **Einstein Detail** stores settings. It does not write height, and it does not generate surface detail onto the ground. **Debug** shows height, slope, and layer. Procedural terrain, erosion, water, foliage, voxels, and page streaming are not in this build.

## Characters

The pose that exists today is a joint on the entity's local frame. Kinds are Fixed, Hinge, Ball, Universal, and Prismatic. Stiffness and damping are stored and are not solved. Clip playback, blending, inverse kinematics, skinning, and ragdolls do not exist yet.

Two sample projects live in this repository:

- `samples/primitive-mannequin/PrimitiveMannequin.jarvigproject` is a 21-bone primitive body. A joint-only level opens as the character workspace.
- `samples/base-characters/BaseCharacters.jarvigproject` holds Base and Base Male. The level is World Settings and one Player Start. The bodies stay in the content browser until you preview them. `DefaultPlayer` references Base Male.

Open either project with **File > Open Project**, or pass `--project` and the `.jarvigproject` path. Double-click a character asset to open it. That does not place a second copy in the level. Dragging a character from the browser still places. Double-clicking a player definition does not place a body.

In the character workspace the tree is the joint list. **View > Show Joints** draws the selected joint. **View > Show All Joints** draws every marker. **View > Mesh Parts** lists imported mesh parts beside the contact hierarchy. The contact parent is a short span, so the tree is not an anatomical bone list. Skinning is absent, so a base body is shown in its rigid rest pose.

Save during a character-asset preview writes nothing to the level. A level that already contains a rig, such as the primitive mannequin, edits that rig in place.

Those character looks are in the editor for you to use on this machine. They are not a stamped acceptance of a character product, and the base-character meshes are not files to publish. A walking pawn, a controller, and input are not created by the First Person or Third Person templates.
