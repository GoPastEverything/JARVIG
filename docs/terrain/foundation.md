# Terrain foundation

ADR-0058. The heightfield is the ground. Einstein detail is not.

## What exists

A terrain actor is one entity: a transform on the scene frame, plus a `Terrain` component. The component owns a regular heightfield in terrain-local meters. X and Z run from negative half-extent to positive half-extent. Y is the sample. The default authoring size is 512 × 512 m, 1 m spacing, 64 × 64 m chunks, height 0. Each chunk mesh is 8,192 triangles and does not cast shadows. The shadow budgets are unchanged: 16 casters, 250,000 triangles.

`height_at` is bilinear over that grid. Gameplay collision and navigation read it. Slope class is derived from the neighbor gradient: below 0.25 is flat, below 1.0 is slope, and steeper is cliff. Paint is a stored class per sample. It is not a second height.

Land Mode is the editor workspace that creates and sculpts this actor. A brush ray hits the heightfield. Left mouse raises. Shift plus left mouse lowers. Smooth moves a sample toward its neighbors. Flatten moves it toward the height under the cursor at mouse-down, and that reference stays for the drag. Radius, strength, and falloff are editor brush settings. Strength is meters at the center of a sculpt. Falloff is smooth or linear. The wheel over the terrain changes the radius while a brush tool is active. Alt+wheel still dollies, and the wheel still changes fly speed when the pointer is off the terrain or the camera is looking, panning, or orbiting. Every write clamps to Height Min and Height Max. The height write happens on the editor thread, so `height_at` changes before the picture does. Only chunks the brush touched are marked dirty.

The picture of those chunks is rebuilt on the existing background job queue. The worker sees a copy of the heightfield and returns meshes. It does not change the live world and it does not create GPU buffers. The editor installs finished meshes at a frame boundary, drops the old mesh id, and evicts that id from the GPU cache. A stroke that arrives while a job is running waits, then the next job builds the latest samples. Opening another level cancels the in-flight job. Create Terrain still builds its chunks on the editor thread. The level stores the samples, not the chunk meshes. Reload rebuilds them.

The samples are base64 of little-endian f32 bits. They do not go through the ordinary JSON number writer. A save writes `jarvig.level` format version 4 only when a terrain component is present.

## Editor grid and brush

The editing grid is painted in the terrain pass, on the same chunk triangles the view already drew. It is not a second mesh, not an entity, and not in the level. A fragment is a line only when its world XZ is near a grid spacing, a dot when it is near a heightfield vertex, or a line when it is near a chunk edge or a chunk-center cross. Everything else is discarded, so the terrain color stays. Depth write stays off. The pass does not blend: a line that should fade is drawn thinner until it is discarded. A small reversed-Z bias keeps the line on the surface. A character, a prop, or any closer surface already in the depth buffer stops the line. The grid does not draw on the sky.

World lines use the camera's binary64 X and Z. The billion-meter root never enters the shader as an absolute position. Minor and major spacing are uniforms. Changing them does not rebuild a chunk. Minor lines are the local spacing. Major lines are the major spacing, ten minor cells at the default. A line still disappears when its screen spacing would fill the view. Camera distance and grazing angle do the rest, by drawing the line thinner until it is discarded. There is no dash pattern. Minor lines fade first. Major lines stay farther, and a grazing view pulls both in. Close to the camera the minor grid is still there, dimmer than the major lines. Farther out the picture is mostly the major grid. At the horizon both are gone. The heightfield is still there. This is only the editing aid. Vertex marks are dots at the samples and follow the minor fade. Chunk edges follow the major fade. Vertex, chunk, and LOD marks use terrain-local positions, so a rotated terrain actor does not stretch the world grid with the heightfield UVs. The LOD mark is a short cross at each chunk center. It is brighter when the stored LOD flag is on. It does not build a second mesh. The brush ring is not distance-faded.

The brush cursor is the same pass, glued to those triangles. The outer ring is the radius in terrain-local XZ. Inner rings sit at falloff weights 0.75, 0.5, and 0.25, so smooth and linear contours are not the same shape. Strength thickens the inner rings and the center dot. The center and the outer ring stay visible when strength is zero. Play hides the grid and the cursor and refuses the brush.

The grid and the brush draw only in Land. Level shows the authored world with that overlay off. Character hides terrain, props, and gameplay actors and draws joint helpers there. Sculpt, smooth, flatten, and paint refuse to run outside Land. Play hides the grid and refuses the brush. See [workspaces.md](../editor/workspaces.md).

While Land Mode is showing a terrain, the Terrain inspector adds editor sections. They are not type-registry fields and they are not in the level. The editor remembers the mode and these settings in `Saved/Editor/workspace.json`.

- Grid: overlay, world units, minor spacing, major spacing, and snap.
- Brush: radius, strength, and falloff.
- Debug overlay: vertex dots, chunk boundaries, and LOD boundaries. Each one is its own check. Turning world units off leaves the others.
- Visibility: terrain, landscape helpers, lighting, characters, props, gameplay actors, and Show Full Level.

Defaults are overlay on, world units on, minor 1 m, major 10 m, snap off, vertex dots off, chunk boundaries off, and LOD off. Terrain, helpers, and lighting are on. Characters, props, and gameplay actors are off until their check is on, or until Show Full Level is on. That hide is a draw filter. It does not write `visible` and it does not enter the level. Level ignores those flags, so a Land overlay cannot leak into the ordinary view. Leaving Land does not reset the stored settings. Lighting off skips direct lights and the lighting terms for that frame. It does not rewrite the view's lighting debug or the saved environment. Helpers off removes the grid and the ring and leaves the terrain mesh. The order stays heightfield, then the chunk mesh, then material and layers, then Einstein microgeometry. Einstein detail still does not own the heightfield or the collision surface.

## Random-access hat lookup

Einstein terrain detail, when it is later generated, has to answer one question: given a chunk-local 2D coordinate and a detail level, what is the displacement? The function is a lookup. It does not grow a mesh outward from the world origin, and it does not walk a chain of neighbors to discover the next sample.

The coordinate is XZ while the surface is the horizontal heightfield. `CliffProjection::DominantAxis` is stored so a later cliff can project along its dominant axis instead of XZ. That mode is not sampled yet. `detail_displacement` returns nothing, including when the detail flag is on. Turning the flag on must not change a height sample.

Generation, when it is accepted, uses the existing projected-error gate (`DETAIL_ERROR_THRESHOLD_PX`, 1 px) and the background job queue. A job publishes a complete patch or it publishes nothing. Detail below the threshold is absent. An unchanged camera and an unchanged patch reuse the cached patch. That path is not implemented in this foundation.

## Large-coordinate precision

The scene frame sits under the bootstrap root at x = 1e9 m. Terrain samples are f32 meters relative to the terrain origin on that frame. They are not absolute universe positions. A brush ray is subtracted from the terrain frame in f64 and only then narrowed to f32. Chunk vertices are in that same terrain-local space, so the GPU matrix never holds the billion-meter origin. Moving the terrain actor moves the frame. The samples stay put.

## What this foundation does not do

No page streaming, clipmap, voxel terrain, erosion, foliage, water, or procedural world generation. No second geometry hierarchy. No Einstein collision. No microtriangles, no noise displacement, and no three-patch comparison. Those wait until this foundation is accepted. RFC-0004 stays a proposal. RFC-0002 stays unstamped.
