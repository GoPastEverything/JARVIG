# Next

A launch of `JARVIGEditor` with no project opens the JARVIG Hub. It does not open Lighting Lab. Lighting Lab is `--project samples/lighting-lab/LightingLab.jarvigproject`. `--self-test` stays the two-triangle bootstrap. ADR-0060.

**JRV-0089 is the primitive mannequin regression fixture. The human saw it on 2026-09-30. It is not the character product. JRV-0091 is the Character Editor over those same joints. JRV-0090, the kinematic FPS pawn, is not started. JRV-0092 places Base and Base Male in `samples/base-characters` as `jarvig.character` version 1: 66 rigid parts each, one Fixed root, Ball parts, wide-default limits. The frame origin is the ball that joins the part to its parent. Opening the project opens the level editor. The startup level is World Settings plus one Player Start. `DefaultPlayer` references Base Male. The bodies are character assets, not level entities. Double-clicking a character opens that asset and leaves the level unchanged on return. ADR-0059. The Character Editor is View > Character Editor, or a double-click of the character asset. Show Joints draws the selected marker. View > Show All Joints draws the rest. A marker is a pick target only in the Character Editor. ADR-0057. Toolbar Play starts the project's free-fly pawn at the editor view. The earlier frames showed the male arm seated and the pivots stacked at the shared node origin. That look is not accepted. The joint parent is a 0.02 m contact span from the hip. FBX is not an importer. This is not a revival of TAA, dynamic GI, page streaming, or prefabs generated from Lighting Lab geometry.**

**The terrain foundation, the Land grid/sculpt slice, and the Level | Land | Character workspaces are implemented and waiting for a human check. The toolbar shows those three words. Level and Land are the authored world. Level keeps the terrain grid and joint helpers off. Land shows a fine grid near the camera, mostly the major grid farther out, and almost nothing at the horizon, including at a grazing angle. Character opens a character asset in its own preview. A level that already has a rig still edits that rig in place. ADR-0059. The last mode is `Saved/Editor/workspace.json`, not the level. File > New Project > Blank or Landscape, choose Land on a Landscape project, create one flat terrain, and confirm the grid sits on the surface. Hover a brush and confirm the ring follows the surface, the inner rings show falloff, and the wheel changes radius. Sculpt a hill, Shift-sculpt a depression, smooth one side, flatten an area, save, and reload. On a level that already has characters, the grid must stop at a shown character and the characters must disappear until Show Full Level. That look is not accepted. The earlier empty-world look is still unaccepted. The grid is painted on the terrain triangles and is not in the level. Spacing is a uniform. A brush dirties only the chunks it touches, and those meshes rebuild on the background job queue. Einstein terrain detail is stored and does not generate microtriangles or write height. ADR-0058. Do not start page streaming, foliage, erosion, water, voxel terrain, or procedural world generation from this.**

**The production Inspector is ACCEPTED / HUMAN SAVE-RELOAD CONFIRMED. It is not backlog JRV-0053 and it is not JRV-0054. Those stay the accepted PBR surface and the accepted lighting tickets.**

JRV-0022, JRV-0028, JRV-0025, JRV-0026, and JRV-0027 are accepted. JRV-0027 is the conservative occlusion correctness stamp. Townshop frames with `occ 0` are not an efficiency result. RFC-0001 is Accepted. The evidence is `docs/benchmarks/rfc-0001/2026-09-27/`. Performance follow-ups in that note stay open. Procedural detail stays off in this repository, and its surface rule is not included. Do not change the meshlet partition. Do not start page streaming, a GPU depth pyramid, GPU compute, or the parent-sidecar size pass. JRV-0088 stays open.

Select townshop. The status line starts with the meshlet counts. Leave both view items off and that is the ordinary indexed mesh. View > Clustered Mesh is the same material through the meshlet index buffer. View > Meshlet IDs is one color per cluster. The first toggle uploads; the output log times it. The next toggle of that actor reuses the upload. The sidecar is `Intermediate/Meshes/affe7d8d-e3e1-448a-a008-76c6e6266867.jarvigmeshlets`, format `JARVMLET` version 2, 50,155,678 bytes, 61,702 meshlets, fingerprint `f55372e5a6527a5a`. Launch `cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor -- --project samples/lighting-lab/LightingLab.jarvigproject`. A launch with no project opens the Hub and does not open this sample. Do not accept JRV-0028 until the clustered shaded view has been compared with the ordinary one.

File > Import Mesh reads an external `.glb` or `.gltf` and does not modify that file. The project copy is `Content/Meshes/<file>`. The catalog stores the asset id, that relative path, and the external path as reimport metadata. `Intermediate/Meshes/<AssetId>.jarvigmesh` is the canonical mesh plus the pick BVH. The level stores `scheme: jarvig.asset` and the id, not the path and not the vertices. One actor is placed on the glTF material (`scheme: jarvig.mesh`), not on Tiles101. A bad file adds nothing. Import alone does not rewrite Lighting Lab. Save writes the actor. The production fixture is `C:\tmp\townshop.glb` (6,122,214 triangles, one material slot), copied to `samples/lighting-lab/Content/Meshes/townshop.glb`. A small generated GLB still proves two slots. Meshes over 250,000 triangles receive shadows and stay out of shadow-map redraws. The meshlet sidecar is separate and is already built for townshop. Do not start JRV-0066.

JRV-0022's human fixture is `C:\tmp\townshop.glb`: 6,122,214 triangles, one material slot, positions, normals, UVs, generated tangents, and local bounds. It is not a second scene. The project copy is `Content/Meshes/townshop.glb`. The level actor is asset `affe7d8d-e3e1-448a-a008-76c6e6266867` at (4, 0.85, -4), material `Imported`, cast and receive on. It does not cast into shadow maps. Two actors sharing one imported mesh keep one `MeshId`. Viewport selection uses the stored BVH. Built-in Cube, Sphere, and Plane stay. The automated save, reload, server, and one-frame standalone path has been run. Selection speed is human accepted. JRV-0028 is the open review. Do not start JRV-0066 from that look.

JRV-0052 is accepted. Play enters the runtime world and the free-fly pawn moves and looks. The authored world stays separate from that runtime world. PIE and `JARVIGGame.exe` keep the same project input configuration.

The inspector is generated from the type registry. Location, rotation, and scale are numeric. Bools are checks. Meshes, materials, projections, shadow resolution, and probe settings are lists. Add Component, Remove, Reset, Set as Startup Camera, and Recapture go through authoring commands. The entity uuid is under Advanced. Play keeps the inspector read-only. Mesh choices are Cube, Sphere, and Plane. Material choices are staged set names, not file paths. There is no asset browser yet.

Lighting Lab's project settings explicitly select `JARVIG.DefaultFreeFlyPawn`. Play and `JARVIGGame` read Move, Look, MoveUp, Sprint, and Pause from that mapping. Escape releases the mouse. Click captures it again. The editor camera is not the pawn. Play In Editor places that free-fly pawn at the editor view, and the runtime camera is the pawn. Standalone play keeps the default spawn at local (0, 1.6, 8), looking toward −Z. A project that leaves the pawn as `none` does not grow one.

`JARVIGGame.exe` loads a `.jarvigproject` into `GameApplication` without the editor. `--server` does that with no device. Play > Run Standalone launches it. Build > Build Project writes `Build/Windows-x64` as loose files. No pak and no cook.

Toolbar Play instantiates a runtime world from the authored level. The free-fly pawn, when the project selects it, starts at the editor view and the view follows that pawn. Pause freezes simulation time. Stop drops the runtime world and restores the editor camera. Play does not dirty the level and does not save the runtime world. An authored startup camera is still View > Set Selected as Startup Camera. Without a pawn and without that camera, Play does not use the editor camera.

The authored world is the level document. Play, when it exists, must instantiate a runtime world from that document and drop it on stop. Runtime edits must not dirty the saved level. The active runtime camera is `world_settings.startup_camera` or none. It is never the first Camera found, and it is never the Perspective editor camera. A version-1 Lighting Lab file still has no camera and no startup-camera key.

Each entity now stores an ordered membership list. The stack, the inspector Components line, and `query_component` read that list. Light, probe, and mesh payloads stay in their records. A second One component fails. Removing Transform while Mesh Renderer, a light, or a probe remains fails. `.jarviglevel` version 1 is unchanged, and a save refuses an entity that grew a second payload instead of dropping it. `Many`, unknown plugin blobs, and Camera are specified and not built. JRV-0088 stays open. Do not start TAA, dynamic GI, displacement, or the cluster geometry system.

**Review JRV-0088. It is not accepted. JRV-0087 is accepted. Do not start TAA, dynamic GI, displacement, or the cluster geometry system.**

The Lighting Lab floor is `Tiles101` at UV scale 1. Tile Plane, Tile Cube, and Tile Sphere use that same set and the same saved factors. Gravel Plane is `Gravel035` through the same generic path. The level stores the set name, not the PNG bytes. The normal choice is the floor tangent frame: bitangent opposite texture +V, so DirectX is sampled with no green flip. The Tiles101 OpenGL file matches only after that green flip. It is not chosen because the machine is DX12. Displacement is recognized and not sampled. UV scale, roughness multiplier (through 2), metallic multiplier, and normal strength are inspector fields and do not recompile the shader. View > Lighting Debug can show base color, world normal, roughness, AO, or metallic without saving. Ingest builds mips and asks the material sampler for anisotropy 8. The long side stays at or below 2048.

Human visual confirmed on 2026-09-24, on the normal editor. The structured diagonal floor artifact is presentation and quantization, not a shadow bug. The interleaved-gradient weave was replaced with finer triangular noise. View > Presentation Dither, 8-bit Steps, and Before Tone Curve isolate it. The Lighting Lab file was not rewritten. Reflections, contact shadows, and shadow softness stayed. Broad 8-bit steps and some colored dither remain on large highlights. The 64² mirror is a separate limit.

The saved `samples/lighting-lab` level is the permanent renderer benchmark. Do not regenerate it.

Directional 0.35 lux, the blue point 14 cd, the warm spot 22 cd, environment intensity 0.20, and exposure 0 EV are unchanged. `cpu` on the status line is this frame's shadow-record time. It is not a GPU timestamp. Idle frames read 0.

Look at: a wide floor, the small sphere's shadow where it meets the floor, View > Lighting Debug > Shadow Cascades while flying slowly, the smooth sphere up close, the same sphere after Probe Resolution 128, and a dragged object whose shadow follows it. Then stop.

Launch `native\target\debug\JARVIGEditor.exe --project samples\lighting-lab\LightingLab.jarvigproject`. A launch with no project opens the Hub instead of this sample. The smooth metal sphere's shadow should start at the floor contact, with a darker core and a softer outer edge. The floor should not ripple. Flying the camera should not swim the shadows or show a hard cascade line. Point and spot shadows should still be there. Inside a direct shadow, the environment, the reflection, the indirect diffuse, and the emissive panel should still be visible. View > Lighting Debug > Shadow Cascades tints the cascades and does not save. Contact Shadows is the short screen-space supplement and does not save either.

Shadow settings on a light are in the Inspector and in the `.jarviglevel`. Changing them dirties the title. Lighting Debug does not.

ADR-0046 is the renderer law: one model, quality budgets, no vendor Low/High, ray tracing optional and never required. The Intel UHD machine is the baseline check, not the ceiling.

Launch `native\target\debug\JARVIGEditor.exe --project samples\lighting-lab\LightingLab.jarvigproject`. That opens `samples/lighting-lab/LightingLab.jarvigproject` and the startup level `Content/Levels/LightingLab.jarviglevel`. A launch with no project opens the Hub and does not open this file. The title is `Lighting Lab - Lighting Lab - JARVIGEditor` with no star. Move the white cube or a light, and the title gains a `*`. File > Save writes the level, keeps a backup under `Saved/Backup`, and does not write GPU handles or the editor camera into the level. Close the editor, launch it again, and the move should still be there. File > New Project, Open Project, New Level, Open Level, Save As, and Save All are on the File menu. Autosave, when the level is dirty for a minute, writes `Saved/Autosaves` and does not replace the `.jarviglevel`. `--self-test` still uses the two-triangle bootstrap and does not open the project.

The renderer is unchanged by this. The level describes the world. The cubemap is still captured after load.

JRV-0079 is implemented. Duplicate (Ctrl+D) and Delete keep the light, probe, or mesh record with the entity. World Settings cannot be duplicated or deleted.

The World Outliner lists the seven bootstrap actors. `LightId` and `ProbeId` are not selection ids. The Perspective camera is still not an entity.

JRV-0065, JRV-0071, JRV-0072, and JRV-0073 are accepted. JRV-0066 stays unstarted until an asset database exists.

JRV-0078 is accepted. The hard rectangles are treated as gone. The cube is still 32², so the reflection stays soft. Do not raise the environment intensity above `0.20`.

JRV-0077 is accepted. The near card casts onto the far card.

Relaunch `native\target\debug\JARVIGEditor.exe --project samples\lighting-lab\LightingLab.jarvigproject`. That window adds a floor, a white cube, a metal sphere, and a red emissive panel, and it captures the probe at 64². A launch with no project opens the Hub. `--self-test` still uses the two triangles at 32².

The metal sphere stores a normal along its radius, and the vertices are shared, so the GPU interpolates that normal. Flat Sphere, just behind it, is the same density with one normal per triangle. Do not hide Flat Sphere and do not smooth it in the material shader. Direct Only should make Metal Sphere smooth and Flat Sphere faceted.

View > Probe Resolution is 32, 64, 128, or 256. The interactive editor still starts at 64. `--self-test` stays at 32. Switching resolution does not replace the cube on screen until the new one has finished. A 64² close-up is spatially coarse. Call that a filtering bug only if the same shapes are still coarse at 128 or 256. Mip 0 is the sharp capture. Higher mips stay GGX. The sample is a cubemap direction, so face edges are the hardware cube sample (`seam=cube-sample`).

The status line reports `capture_resolution`, `capture_ms` (the six scene faces), `prefilter_ms` (GGX faces plus the cosine irradiance build), `mip_count`, `selected_lod` (smooth-sphere roughness 0.045), and `probe_weight` at the Metal Sphere. It also says `sphere=smooth flat=face` when both actors are present.

View > Probe Update is Static, On Demand, On Transform, On Lighting, or Time Sliced. Static is the default and still captures once. A later refresh, including Recapture and a resolution change, spends one cubemap face per frame. Time Sliced does that for the first capture too. The camera does not dirty a probe and does not redraw shadow maps. Dynamic probes are not implemented. Do not recapture every probe every frame.

View > Recapture Reflection Probes marks the probe dirty. Flying the camera must not increase the recapture count or the shadow update count.

View > Lighting Debug > Direct Unshadowed is the lamps with shadows forced off. Indirect Diffuse Only is still the bounce. The white cube's face toward the red panel is where the red bleed should show. The metal sphere is where the panel should show up in the reflection. Shadows should be softer than the old hard edge, without the caster floating off the floor.

Do not raise the environment intensity above `0.20`.

View > Environment Light toggles the world light. View > Exposure +, Exposure -, and Reset Exposure are still the view. WASD and Q/E are still the camera. Pitch the near triangle with the rotate gizmo. The fill should follow the normal on the frames during the drag.

```text
source file -> importer -> AssetId -> asset database -> cooked data -> runtime resource -> Content Browser
```

JRV-0058 through JRV-0064 and JRV-0070 are accepted. The shell is Editor theme v1. Do not put the light Win32 face back. Do not save `ObjectId` or `EntityHandle`. Do not invent scale. Do not put the gizmo in the world.

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor
cargo run --manifest-path native/Cargo.toml -p jarvig_editor -- --self-test
cargo test --manifest-path native/Cargo.toml -p jarvig_editor
```

Leave `--self-test` off to keep the window open. RMB looks. WASD flies. Q down, E up. Shift boosts. The wheel changes speed. MMB pans. Alt+LMB orbits. F frames the selection. With Perspective focused and no mouse capture, `1` selects, `2` translates, `3` rotates, and `4` reports that scale is unavailable. View > Reset Layout restores the workspace and does not reset the camera or the world. Typing in the Inspector must not fly and must not change the tool.

The two-view renderer test is still:

```powershell
cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host
```

`pnpm dev:editor` is the transitional TypeScript dock. It is not `JARVIGEditor.exe`.
