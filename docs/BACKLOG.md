# JARVIG backlog

Tickets are the founding list in `designdoc.html` section 33, numbered `JRV-`. Status is Done only when the acceptance line was met by a command named in Validation. "In progress" means a real subset landed and the exit is not met.

Next: RFC-0001 is Accepted. Procedural detail stays off in this repository, and its surface rule is not included. JRV-0088 stays open. Do not start page streaming, a GPU depth pyramid, GPU compute, or the parent-sidecar size pass. RFC-0001's measured performance follow-ups stay open.

## JRV-0001 — Freeze dependency rules

- Status: Done
- Milestone: P0
- Subsystem: platform
- Dependencies: none
- Goal: Forbidden engine-to-editor and server-to-render imports fail in CI.
- Acceptance Criteria: The boundary checker rejects those imports and the suite runs it.
- Validation: `pnpm lint`. `tests/integration/hosts.test.ts` asserts a clean tree and synthetic violations. Passed 2026-09-22.
- Documentation: `docs/architecture/dependency-rules.md`, ADR-0015.

## JRV-0002 — Document PlayCanvas fork and upstream strategy

- Status: Done
- Milestone: P0
- Subsystem: platform
- Dependencies: none
- Goal: Upstream remote and merge policy are recorded.
- Acceptance Criteria: Remote and merge policy recorded. A vendor dump is not required.
- Validation: `git remote -v` shows `upstream-playcanvas` → `https://github.com/playcanvas/engine`. Policy is ADR-0002. No PlayCanvas files in the tree. Checked 2026-09-22.
- Documentation: ADR-0002, `docs/legal/THIRD_PARTY.md`.

## JRV-0003 — Minimal EngineHost

- Status: Done
- Milestone: P0
- Subsystem: core
- Dependencies: JRV-0001
- Goal: The same lifecycle boots a client host and a test host.
- Acceptance Criteria: Client and test profiles initialize, tick, and shut down.
- Validation: `tests/unit/foundation.test.ts`, `tests/integration/hosts.test.ts`, `pnpm smoke`. Passed 2026-09-22.
- Documentation: `docs/architecture/engine-runtime.md`.

## JRV-0004 — Clock and fixed-step service

- Status: Done
- Milestone: P0
- Subsystem: core
- Dependencies: JRV-0003
- Goal: Variable render deltas produce a stable fixed step.
- Acceptance Criteria: Tests pass under variable deltas, including a step cap.
- Validation: `tests/unit/foundation.test.ts` clock cases. Passed 2026-09-22. Cap policy: leftover time is dropped and `clamped` is reported.
- Documentation: `docs/architecture/engine-runtime.md`.

## JRV-0005 — Stable entity IDs

- Status: Done
- Milestone: P0
- Subsystem: ecs
- Dependencies: JRV-0003
- Goal: IDs survive save, load, and duplicate.
- Acceptance Criteria: Load keeps ids. Duplicate allocates a new id and copies data.
- Validation: Golden scene test in `tests/unit/foundation.test.ts`. Passed 2026-09-22.
- Documentation: `docs/architecture/ecs.md`, ADR-0009.

## JRV-0006 — Component schema metadata

- Status: Done
- Milestone: P0
- Subsystem: ecs
- Dependencies: JRV-0005
- Goal: Sample components serialize and can be inspected.
- Acceptance Criteria: A sample component has version, replication, and field metadata, and round-trips.
- Validation: `Health` schema in the golden scene test. `registry.get('Health').replicated` is asserted. Passed 2026-09-22.
- Documentation: `docs/architecture/ecs.md`.

## JRV-0007 — Editor shell

- Status: Done
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0003
- Goal: Dock layout, commands, and a viewport pane.
- Acceptance Criteria: The editor shows a dock layout driven by commands, with a viewport pane bound to the engine camera. Layout is view state. Tick calls the engine.
- Validation: `tests/integration/editor-shell.test.ts`. The temporary panel host is not `JARVIGEditor.exe`. Pixel output is JRV-0008 and must be the engine RHI, not a page-owned canvas.
- Documentation: `docs/architecture/editor-runtime.md`, ADR-0018.

## JRV-0041 — Native core boundary

- Status: Done
- Milestone: P0
- Subsystem: core
- Dependencies: ADR-0017
- Goal: A headless native runtime the editor and server call through the C ABI, plus an RHI contract that does not name wgpu.
- Acceptance Criteria: `cargo test` passes. The editor tick advances the native frame in step with the prototype. The server profile reports no render stage. `jarvig_rhi` has a null backend and no GPU crate.
- Validation: `cargo test --manifest-path native/Cargo.toml`. `tests/integration/editor-shell.test.ts` checks `nativeFrame`. Server boot rejects a native render stage.
- Documentation: ADR-0017, ADR-0019, `docs/rendering/rhi.md`, `native/jarvig_core/include/jarvig_core.h`.

## JRV-0042 — Clear frame through the RHI

- Status: Done
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0041, ADR-0019
- Goal: One native presented clear through engine, renderer, RHI, and a private wgpu backend.
- Acceptance Criteria: The native host opens, the engine ticks, the renderer clears, the frame presents, resize and a zero-size frame do not panic, shutdown is clean, the null backend and the headless server still run, and wgpu is private to `jarvig_rhi_wgpu`.
- Validation: `cargo test --manifest-path native/Cargo.toml --workspace` and `cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host -- --self-test` on 2026-09-22. Adapter Intel UHD Graphics, four presents, `JARVIG_OK native-present frames=4`. `pnpm lint`, `pnpm test`, and `pnpm smoke` passed. `cargo tree` shows wgpu only under `jarvig_rhi_wgpu` and winit only under `jarvig_platform`.
- Documentation: `docs/rendering/rhi.md`, `docs/architecture/engine-runtime.md`, `docs/architecture/editor-runtime.md`.

## JRV-0043 — Triangle

- Status: Accepted
- Visual confirmation: Pass. On 2026-09-22 a human launched the native host and saw the JARVIG clear plus the large interpolated RGB triangle.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0042
- Goal: One procedural triangle through the renderer and RHI. No vertex buffer.
- Acceptance Criteria: The native host presents the clear plus a triangle. The pipeline is created once. The null backend records a bound pipeline and a draw of 3. The host does not name the shader. wgpu stays in `jarvig_rhi_wgpu`.
- Validation: `cargo test --manifest-path native/Cargo.toml --workspace` and `cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host -- --self-test` on 2026-09-22. Four presents after resize and a zero-size frame. Self-test requires `pipeline_count=1`. Null tests check pass order, a bound pipeline, and one draw of 3. `pnpm lint`, `pnpm test`, and `pnpm smoke` passed. Visual confirmation is recorded above.
- Documentation: `docs/rendering/rhi.md`. WGSL in `jarvig_renderer` is a bootstrap carrier, not the material IR. JRV-0044 replaced the procedural vertices with buffers. This ticket stays the accepted procedural milestone.

## JRV-0054 — First lighting system

- Status: Accepted
- Human visually confirmed: PASS
- World light ownership: PASS
- Directional light: PASS
- Point light: PASS
- Spot light: PASS
- Physical units: PASS
- Single light extraction: PASS
- Per-view light packets: PASS
- Large-world light precision: PASS
- StandardMetalRough integration: PASS
- Visible direct illumination: PASS
- Visible specular response: PASS
- Metal/dielectric differentiation: PASS
- Emissive independence: PASS
- Multi-view: PASS
- Reversed-Z: PASS
- Headless server: PASS
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0053, ADR-0028
- Goal: Directional, point, and spot lights live in the world, extract once, and drive the standard BRDF. The graph does not own them.
- Acceptance Criteria: One shared snapshot light list. Each view uploads its own camera-relative packet. Unlit ignores lights. A light edit does not recompile or reupload a mesh or texture. No shadows, IBL, or ambient hack. The server does not upload.
- Validation: On 2026-09-22 the lit split frame was looked at. The outer smoother metal showed a localized blue-white specular response, the warm light hit another region, and the inner rougher dielectric read more diffusely. Highlight clipping is the missing tone map, not a BRDF bug, and was not treated as one. Self-test before that look: four presents, swapchain `Bgra8UnormSrgb`, `world_light_count=3`, `gpu_light_packet_count=2`, `light_buffer_upload_count=4`, `material_compile_count=1`, `material_pipeline_count=1`.
- Documentation: `docs/rendering/lighting.md`, ADR-0029.

## JRV-0055 — GPU adapter policy

- Status: Accepted
- Adapter enumeration: PASS
- JARVIG-owned selection policy: PASS
- Auto policy: PASS
- HighPerformance policy: PASS
- LowPower policy: PASS
- Specific selection: PASS
- Specific-missing failure: PASS
- Surface compatibility filtering: PASS
- Software fallback policy: PASS
- Deterministic selection: PASS
- Headless server isolation: PASS
- Backend type isolation: PASS
- Live host: Lenovo development laptop. Integrated Intel UHD only. No discrete GPU is expected. The RX 6800 XT belongs to a different desktop and is not on this machine.
- HighPerformance fallback to Intel UHD: CORRECT. LowPower selecting Intel UHD: CORRECT. Windows enumeration is correct. Do not track a missing-AMD or driver investigation for this machine. Do not add a vendor export or weaken selection to compensate. Keep the synthetic discrete-versus-integrated tests.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0042, ADR-0019
- Goal: JARVIG chooses the GPU. A backend power hint is not the product policy.
- Acceptance Criteria: Auto, high performance, low power, and a specific fingerprint. Surface compatibility and required limits are applied before the choice. A missing specific adapter fails. Software is an explicit last resort. The log states the device and the reason. No vendor is a score. The server does not enumerate. The lit scene still presents.
- Validation: On 2026-09-22, synthetic policy tests passed. `--list-gpus` on this laptop listed Intel UHD Graphics (Vulkan, DX12, OpenGL) and the Microsoft Basic Render Driver. Auto selected Intel UHD on DX12 and said there was no compatible discrete adapter. That fallback is the correct result on a machine with no discrete GPU. Low power selected the same integrated adapter as the preferred class. `--gpu 1002:73bf:dx12` failed closed. `--gpu 8086:9b41:dx12 --self-test` presented four lit frames. `cargo test --manifest-path native/Cargo.toml --offline --workspace` passed. `pnpm lint`, `pnpm typecheck`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/platform/graphics-device-selection.md`, `docs/rendering/rhi.md`, ADR-0030.

## JRV-0057 — EPIC: Native JARVIG Editor

- Status: In progress. This is the epic, not a code ticket. JRV-0058 through JRV-0064 and JRV-0070 are accepted. JRV-0064 was human-flown. JRV-0065 is implemented and not accepted. The dark icon shell is Editor theme v1. Do not start JRV-0066.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0055, ADR-0017, ADR-0018, ADR-0022, ADR-0025, ADR-0031. Not JRV-0056.
- Goal: `JARVIGEditor.exe` hosts the same engine a future player and the dedicated server use. The editor does not reimplement the engine. The TypeScript dock does not become this product.
- Children, in order:
  - JRV-0058 — native application shell and one engine viewport. Accepted.
  - JRV-0059 — docking and workspace framework. Accepted.
  - JRV-0060 — native entity identity. Accepted. `ObjectId` stays the bootstrap slot.
  - JRV-0061 — world outliner bound to that identity and to `SceneWorld`. Accepted.
  - JRV-0062 — one selection model for the outliner, viewport, inspector, and content browser. Accepted.
  - JRV-0063 — inspector / details. Accepted. Reflected fields and the first `SetProperty` commands.
  - JRV-0070 — native child viewport surface resize. Accepted. Blocker for JRV-0064 closed.
  - JRV-0064 — Perspective editor camera. Accepted. Human input confirmed. Navigation icons are drawn. The shell is Editor theme v1.
  - JRV-0065 — viewport picking and the transform gizmo. Accepted. Human input confirmed. Scale stays unavailable on purpose.
  - JRV-0066 — content browser fed by an asset database. Not started. Do not start it until an asset identity exists. The slot is JRV-0058.
  - JRV-0067 — engine-owned log stream. Not started. The shell's edit control is temporary.
  - JRV-0068 — play / simulate controls. Not started. Not the Play-In-Editor world copy, and not the player executable.
  - JRV-0069 — editor layout persistence. Not started.
- Not in this epic: the Hub, a project file format, asset importers, material-graph UI, scripting, physics, terrain, world partition, animation, a shader editor, and a profiler implementation.
- Documentation: `docs/editor/`, `docs/architecture/editor-runtime.md`, ADR-0031.

## JRV-0058 — Native editor application shell

- Status: Accepted
- Native JARVIGEditor.exe: PASS
- Engine hosted in-process: PASS
- Real engine RenderView embedded: PASS
- Editor does not own renderer/world/materials: PASS
- Native shell/menu/toolbar: PASS
- Outliner shell: PASS
- Inspector shell: PASS
- Content Browser shell: PASS
- Output/status shell: PASS
- Player/editor separation: PASS
- Server/editor separation: PASS
- Headless server unchanged: PASS
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0057, JRV-0055, ADR-0031
- Goal: A real `JARVIGEditor.exe` with the product layout, hosting one engine `RenderView` in the center child. Panels may be placeholders. The viewport may not be a second renderer.
- Acceptance Criteria: The window shows File, Edit, View, Build, Play, and Help; a toolbar; a world outliner; one perspective engine viewport; an inspector; a content slot; an output log; and a status bar. The viewport is `JARVIG.Perspective` filling that child. The bootstrap split is not the default layout. No play-in-editor, no gizmo, no asset database, no editor-owned world or material system. Closing the window drops the renderer first.
- Validation: On 2026-09-22, `cargo run --manifest-path native/Cargo.toml --offline -p jarvig_editor -- --self-test` printed `JARVIGEditor views=1 objects=2 lights=3 compiles=1 presents_last=1` and `JARVIG_OK editor frames=4`. Auto selected Intel UHD on DX12 and stated the integrated fallback. A captured frame shows the menu, the toolbar, World / Object 1 / Object 2, one lit triangle view (not the left/right split), the inspector and content placeholders, the output text, and `Ready | Intel(R) UHD Graphics | IntegratedGpu`. `cargo test --manifest-path native/Cargo.toml --offline --workspace` passed (core 31, engine 2, material 9, renderer 14, rhi 16, rhi_wgpu 1). `pnpm lint`, `pnpm typecheck`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/editor/`, ADR-0031.

## JRV-0059 — Docking and workspace framework

- Status: Accepted
- Native editor workspace: PASS
- Perspective RenderView integration: PASS
- Panel composition: PASS
- Outliner region: PASS
- Inspector region: PASS
- Content Browser region: PASS
- Output Log region: PASS
- Application chrome separation: PASS
- Status bar: PASS
- Dark editor presentation: PASS
- Splitter dragging: PASS
- Tab activation: PASS
- Panel show/hide: PASS
- Dock rearrangement: PASS
- Empty-stack collapse: PASS
- Reset Layout: PASS
- Focus tracking: PASS
- Viewport surviving workspace mutations: PASS
- Full acceptance, recorded 2026-09-23:
- Visual workspace: PASS
- Splitters: PASS
- Tabs: PASS
- Show/hide: PASS
- Dock operation: PASS
- Empty-stack collapse: PASS
- Reset layout: PASS
- Focus model: PASS
- Stable viewport HWND/RenderView: PASS
- Headless dock-model tests: PASS
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0058
- Goal: Replace the fixed child layout with a dock tree of splits and tab stacks. The tree is editor state. It is not a window tree and not engine state.
- Acceptance Criteria: Default workspace is data. Panels have stable ids. Tabs show one active panel and hide the others without destroying them. Splitters use ratios. Empty stacks collapse. View menu toggles singletons and Reset Layout restores only the workspace. The perspective HWND and its `RenderView` survive those operations. Engine crates do not name the dock. Floating windows and disk persistence are not required.
- Validation: On 2026-09-23, `cargo test -p jarvig_editor` passed 18 dock tests with no GPU, including one test that drives splitter ratio, tab activation, show/hide, dock, empty-stack collapse, focus, and reset. `--self-test` presented 7 frames, kept the same viewport HWND and `RenderView`, and printed `JRV-0059 behavior splitter=PASS tabs=PASS show_hide=PASS dock=PASS collapse=PASS reset=PASS focus=PASS viewport=PASS`. The mouse path calls those same workspace commands. DXGI `ResizeBuffers` on the child viewport is still refused, so the image stretches when the panel moves.
- Documentation: `docs/editor/workspaces.md`, ADR-0032.

## JRV-0060 — Native entity identity

- Status: Accepted
- Persistent UUID identity: PASS
- Runtime generational handle: PASS
- UUID ↔ Handle registry: PASS
- Stale-handle rejection: PASS
- Duplicate-new-identity semantics: PASS
- Edit-preserves-identity semantics: PASS
- Parent-cycle rejection: PASS
- Reference-frame lookup by Handle: PASS
- Render snapshot identity propagation: PASS
- No stable ABI leakage: PASS
- Milestone: P1
- Subsystem: world
- Dependencies: JRV-0058, ADR-0009. Not a panel ticket.
- Goal: A persistent `EntityUuid` and a generational `EntityHandle`. The uuid is what save files and the editor store. The handle is the hot lookup. `ObjectId` stays a bootstrap drawable slot.
- Acceptance Criteria: UUID text, not a slot number. Handle is index plus generation and is not serialized. A stale generation fails after the slot is reused. A duplicate receives a new uuid and a new handle. An edit does not change the uuid. Name and parent live on the registry. Components are not implemented. The reference frame is reached by the handle. The id is not a C ABI type. The outliner may print the uuid. Selection and the real hierarchy view are not this ticket.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core` passed 37 tests, including the registry round-trip, stale generation after slot reuse, duplicate, parent cycle, the scene test that an edit keeps the uuid while a duplicate mints a new uuid and a new handle, and the outline test that a retired parent is not replaced by a reused slot. The reverse uuid lookup remains a scan. Hot lookup is one slot read. Accepted.
- Documentation: `docs/architecture/entity-identity.md`, `docs/editor/outliner.md`.

## JRV-0061 — World outliner

- Status: Accepted
- Authoritative-world source: PASS
- WorldRoot != Entity: PASS
- EntityUuid row identity: PASS
- Friendly-name presentation: PASS
- Non-renderable entities: PASS
- Rename preserves identity: PASS
- Duplicate mints identity: PASS
- Hierarchy/reparenting: PASS
- Cycle rejection: PASS
- Retire/stale-handle safety: PASS
- Slot-reuse safety: PASS
- Expansion state keyed by UUID: PASS
- Render-snapshot independence: PASS
- Docking regression: PASS
- Viewport regression: PASS
- Server/editor boundary: PASS
- Public ABI isolation: PASS
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0060
- Goal: The outliner lists the authoritative entity hierarchy, not render instances and not RHI resources.
- Acceptance Criteria: `World` is an editor root, not an entity. Entity rows are keyed by `EntityUuid`. Labels are names. Expansion is editor state. A non-renderable entity is still a row. Rename keeps the uuid. Duplicate mints a new uuid. Reparent follows the registry and a cycle is rejected. Retire removes the uuid. A reused slot does not resurrect it. The tree caret is not selection. The inspector stays unselected. Dock layout and the perspective view survive a refresh. No C ABI export.
- Validation: On 2026-09-23, `cargo test -p jarvig_core` passed 37 tests and `cargo test -p jarvig_editor` passed the outliner model tests with no GPU. `JARVIGEditor.exe --self-test` printed `JRV-0061 probe entity_count=3 render_instances=2 outliner_nodes=3 outliner_roots=3 rename=PASS duplicate=PASS reparent=PASS cycle=PASS retire=PASS stale=PASS`, then the JRV-0059 behavior line, and `JARVIG_OK editor frames=7`. The captured window shows World / Near Triangle / Far Triangle. Accepted.
- Documentation: `docs/editor/outliner.md`, `docs/architecture/entity-identity.md`.

## Debt — Normalize a retired parent's children

- Status: Closed by JRV-0079. `SceneWorld` destroy writes no parent before the slot is retired. Children stay. They are not cascade-deleted.
- Documentation: `docs/editor/outliner.md`, `docs/architecture/entity-identity.md`, ADR-0041.

## Debt — Editor viewport child-surface resize

- Status: Moved to **JRV-0070**. That number was free. Do not keep a second copy of this debt.

## JRV-0062 — Selection model

- Status: Accepted
- Editor-owned SelectionService: PASS
- EntityUuid identity: PASS
- World-session scope: PASS
- Replace/add/remove/toggle/clear: PASS
- Multi-selection: PASS
- Deterministic selection order: PASS
- Primary selection semantics: PASS
- Selection revision: PASS
- Outliner integration: PASS
- Caret/selection separation: PASS
- World-root clear behavior: PASS
- Inspector observation: PASS
- Status observation: PASS
- Rename preserves selection: PASS
- Reparent preserves selection: PASS
- Retire reconciliation: PASS
- Runtime-slot reuse safety: PASS
- Duplicate remains distinct/unselected: PASS
- Docking independence: PASS
- Focus independence: PASS
- World state isolation: PASS
- Renderer/material isolation: PASS
- Server boundary: PASS
- Public ABI isolation: PASS
- Revision contract: `revision` is the effective semantic selection. Membership, order, and primary changes bump it. A future operation that changes only primary or order must bump it too. No redesign in this acceptance.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0061
- Goal: One `SelectionService` on the editor session. Entity selection is an `EntityUuid`. Panels do not own a second selection.
- Acceptance Criteria: Replace, add, remove, toggle, and clear. Order is explicit selection order. Primary is the last item. Nil is rejected. `World` clears selection and is not an item. Rename and reparent keep the uuid. Retire removes it. A reused slot is not selected. Duplicate does not select the copy. Hide, show, reset, and focus do not clear it. The inspector summarizes and does not edit. Picking is not included. Shift-range is deferred. No C ABI export.
- Validation: On 2026-09-23, `cargo test -p jarvig_editor` passed 28 tests, including the selection order, nil rejection, primary survivor, and world reconcile tests. `cargo test -p jarvig_core` passed 37. `JARVIGEditor.exe --self-test` printed `JRV-0062 probe selection_count=0 selection_revision=7 click=PASS ctrl=PASS dock=PASS focus=PASS rename=PASS reparent=PASS retire=PASS reuse=PASS clear=PASS`, then the dock behavior line, then `JRV-0062 selection selection_count=0 selection_revision=7 selection_primary=none`, and `JARVIG_OK editor frames=7`. A capture during the probe shows Near Triangle highlighted, the inspector summary for that entity, the status line `Selected: Near Triangle`, and the same lit view. Not accepted.
- Debt: Shift-click range selection is not implemented. It is an outliner presentation order, not a second selection model.
- Documentation: `docs/editor/selection.md`.

## JRV-0063 — Inspector

- Status: Accepted
- Engine-owned TypeRegistry: PASS
- JARVIG TypeId / FieldId: PASS
- Independent schema version: PASS
- Semantic FieldInfo metadata: PASS
- PropertyValue model: PASS
- Inspection snapshot boundary: PASS
- Editor-owned InspectorModel: PASS
- No mutable World references retained by controls: PASS
- SelectionService integration: PASS
- Multi-select safe behavior: PASS
- Entity section: PASS
- Transform section: PASS
- UUID read-only: PASS
- Parent read-only: PASS
- f64 local translation editable: PASS
- Quaternion remains authoritative: PASS
- No fake Scale field: PASS
- Engine authoring command path: PASS
- Structured command results: PASS
- No-op mutation semantics: PASS
- Invalid-value rejection: PASS
- Read-only enforcement: PASS
- Rename preserves UUID/Handle/selection: PASS
- Outliner updates through World revision: PASS
- Translation changes authoritative frame: PASS
- Render snapshot naturally observes change: PASS
- No mesh/material/texture rebuild: PASS
- Large-world precision preserved: PASS
- Docking regression: PASS
- Server/editor boundary: PASS
- Public ABI isolation: PASS
- Guardrail: `SetProperty { TypeId, FieldId, PropertyValue }` stays a validated semantic operation. It is not a write to arbitrary reflected memory.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0062, ADR-0022
- Goal: The details panel is a view of engine reflection. Edits are `SetProperty` commands. Win32 does not write `SceneWorld`.
- Acceptance Criteria: One selected entity shows Entity and Transform from the type registry. Name and local translation edit through the engine and preserve `EntityUuid` and `EntityHandle`. UUID, parent, and the quaternion are read-only. The same value does not bump the world revision. A non-finite translation is rejected. Several selected entities do not open an edit form. Selection does not change. The next extraction shows the moved pose. No mesh or material upload. No C ABI export. Scale is omitted. Rotation stays a quaternion.
- Validation: On 2026-09-23, `cargo test -p jarvig_core` passed 39 tests, `cargo test -p jarvig_engine` passed 3, and `cargo test -p jarvig_editor` passed 30. `JARVIGEditor.exe --self-test` printed `JRV-0063 probe inspector_sections=2 inspector_fields=5 name=PASS translation=PASS noop=PASS invalid=PASS readonly=PASS selection=PASS`. The authoring capture shows Near Triangle selected, the property form, and the near triangle shifted on X. Accepted.
- Documentation: `docs/editor/inspector.md`, `docs/api/reflection.md`, `docs/api/commands.md`.

## JRV-0070 — Native child viewport surface resize

- Status: Accepted
- BLOCKER FOR JRV-0064: CLOSED
- Root cause identified: PASS
- No resize inside WM_SIZE/layout: PASS
- Pending-size/coalescing model: PASS
- Actual child client pixels authoritative: PASS
- DPI/UI-vs-drawable distinction: PASS
- Safe frame-boundary configure: PASS
- Swapchain view lifetime fixed: PASS
- Zero-size suspension: PASS
- Same-size no-op: PASS
- Surface resize: PASS
- Depth recreation: PASS
- Reversed-Z preserved: PASS
- Projection aspect follows configured surface: PASS
- Stable viewport HWND: PASS
- Stable RenderViewId: PASS
- No adapter/device recreation: PASS
- Splitter resize: PASS
- Top-level resize: PASS
- Rapid resize: PASS
- Minimize/restore: PASS
- Docking regression: PASS
- Authoring regression: PASS
- Mesh/material/texture residency preserved: PASS
- Hard client=surface=depth invariant: PASS
- Milestone: P1
- Subsystem: editor / renderer
- Dependencies: JRV-0059, JRV-0046, JRV-0048
- Goal: A dock resize reconfigures the existing child-window surface and depth to the child client pixels. The HWND and `RenderView` stay. The image is not a stretched old buffer.
- Acceptance Criteria: Layout records a pending size and does not call DXGI inside `WM_SIZE`. The next frame drops swapchain views, configures the surface, and recreates depth to that size before drawing. Zero or minimized skips a 0×0 configure. Projection aspect is the configured width over height. Same HWND and `RenderViewId`. No mesh, material, or texture rebuild. A translation command still moves the triangle after a resize.
- Validation: On 2026-09-23, `cargo test -p jarvig_renderer resize_retires_depth_without_dropping_meshes_or_view_ids` passed on the null device. `JARVIGEditor.exe --self-test` printed `JRV-0070 viewport viewport_client_px=749x450 surface_configured_px=749x450 depth_px=749x450 projection_aspect=1.6644444 viewport_resize_requests=4 viewport_resize_applied=4 viewport_resize_failures=0 viewport_zero_size_skips=1`, the JRV-0059 behavior line including `viewport=PASS`, and `JARVIG_OK`. The sequence resized the top-level window three times in one step, moved the outliner splitter, minimized and restored, then changed local X by 0.25 m and restored it. Intel UHD DX12. Accepted. `device.poll(Wait)` on resize remains bootstrap debt. A configure that succeeds and a following depth allocation that fails belongs with OOM and device-loss recovery, not with the camera.
- Documentation: `docs/editor/viewport.md`, `docs/rendering/views.md`.

## JRV-0064 — Perspective editor camera

- Status: ACCEPTED / HUMAN INPUT CONFIRMED
- Native Perspective editor camera: PASS
- RMB mouse-look: PASS
- WASD fly navigation: PASS
- Vertical movement: PASS
- High-precision camera pose: PASS
- Editor-camera/world separation: PASS
- SceneWorld revision isolation: PASS
- Selection isolation: PASS
- Large-world precision: PASS
- Per-view lighting response: PASS
- Focus Selected architecture: PASS
- Resize/aspect integration: PASS
- Docking integration: PASS
- MANUAL NAVIGATION: Human-tested movement/look: PASS
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0058, JRV-0070, ADR-0033
- Goal: The Perspective viewport flies an editor camera. That camera is session state on the render view, not a scene entity.
- Post-acceptance correction: The lines "navigation icons are stored and not drawn" and "the toolbar is still text" are stale. The toolbar draws the navigation PNGs. The shell is Editor theme v1 (charcoal panels, blue accent, dark Outliner, Inspector, Content, and Output). Do not revert that chrome to the light Win32 face. Select, Translate, Rotate, and Scale highlight one tool. At the time of this acceptance they did not move actors. JRV-0065 later made Translate and Rotate drag. Scale stayed disabled.
- Validation: On 2026-09-23, `cargo test -p jarvig_core --lib` passed 40 tests. `cargo test -p jarvig_editor` passed. `cargo test -p jarvig_renderer -- view_pose_moves` passed. `JARVIGEditor.exe --self-test` printed `world_revision_before_navigation=31 world_revision_after_navigation=31`, `editor_camera_fov=60`, and `camera_reset=PASS`. Position stayed near `999999999.4105` m. Intel UHD DX12. The human then flew and looked around the Perspective view. Accepted.
- Documentation: `docs/editor/camera.md`, `docs/editor/theme.md`, ADR-0033.

## JRV-0065 — Viewport picking and transform gizmo

- Status: ACCEPTED / HUMAN INPUT CONFIRMED
- Viewport picking: PASS
- EntityUuid selection path: PASS
- Outliner integration: PASS
- Inspector integration: PASS
- Translate gizmo: PASS
- Rotate gizmo: PASS
- Authoritative f64 translation: PASS
- Authoritative quaternion rotation: PASS
- Authoring edit begin/update/commit/cancel: PASS
- Escape cancel: HUMAN PASS
- Camera input handoff after gizmo: HUMAN PASS
- Large-world precision: PASS
- Lighting transform follow-up: PASS
- Scale intentionally unavailable until a real authoritative scale model exists.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0062, JRV-0064, JRV-0070
- Goal: Click a visible object, select its `EntityUuid`, and drag a gizmo through an authoring command so the authoritative f64 transform changes.
- Result: Select picks. Translate and Rotate drag. Scale is visible and disabled. The gizmo is an editor overlay, not an entity. A hit is an `EntityUuid`. A drag is absolute `SetProperty`. Cancel restores the original. Commit keeps the value. Local and World are both real. Only the primary of a multi-selection moves. Tool keys are `1` `2` `3` `4` so W and Q/E stay camera keys.
- Acceptance Criteria: Met. Human confirmed Escape during a drag restores the transform, and RMB plus WASD flies immediately after the drag is released. Do not start JRV-0066. The content browser still waits on an asset database.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core --lib`, `-p jarvig_engine --lib`, `-p jarvig_editor`, and `-p jarvig_renderer --lib` passed. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JRV-0065 probe pick=PASS centimeter=PASS translate_cancel=PASS translate_commit=PASS rotate_cancel=PASS far=PASS empty=PASS scale=UNAVAILABLE local=PASS primary_only=PASS`, `gizmo_visible=1`, `authoring_edit_active=0`, `gizmo_commits=1`, `gizmo_cancels=4`, and `JARVIG_OK editor frames=9`. Client, surface, and depth stayed `749x441`. Mesh uploads stayed 2. Material compiles stayed 1. The navigation world revision stayed 31. The presented frame shows the translate gizmo on Near Triangle.
- Documentation: `docs/editor/picking.md`, `docs/editor/gizmos.md`, `docs/rendering/editor-overlays.md`, ADR-0034.

## JRV-0066 — Content browser

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: an asset database that does not exist yet. The shell slot is JRV-0058.
- Goal: Browse cooked content. Do not implement the database or the importers in this ticket, and do not scan the disk and call that the database.
- Acceptance Criteria: Not written yet.
- Validation: Not run.
- Documentation: `docs/editor/content-browser.md`.

## JRV-0067 — Engine log stream

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0058
- Goal: The output panel reads an engine-owned diagnostics stream. Categories can include Engine, Renderer, RHI, Assets, Materials, Physics, Network, and Editor.
- Acceptance Criteria: Not written yet. The string copied into the edit control is not this ticket.
- Validation: Not run.
- Documentation: `docs/editor/architecture.md`.

## JRV-0068 — Play and simulate controls

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0058, the command and undo tickets that Play-In-Editor already requires
- Goal: Toolbar and menu controls for play and simulate. They are not the player executable and they are not the Play-In-Editor world copy by themselves.
- Acceptance Criteria: Not written yet. Do not toggle `engine.profile` on the authoring world.
- Validation: Not run.
- Documentation: `docs/editor/commands.md`, `docs/architecture/play-in-editor.md`.

## JRV-0069 — Editor layout persistence

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0059
- Goal: Remember the workspace layout. Do not design the project file here.
- Acceptance Criteria: Not written yet.
- Validation: Not run.
- Documentation: `docs/editor/workspaces.md`.

## JRV-0071 — HDR, tone mapping, and exposure

- Status: ACCEPTED / HUMAN VISUAL CONFIRMED
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0053, JRV-0054, JRV-0070
- Goal: Accumulate direct light in linear HDR and tone-map it with an explicit exposure. Stop treating the swapchain as the light buffer. Do not add ambient, IBL, bloom, or a gamma hack inside the BRDF.
- Acceptance Criteria:
  - HDR scene-color target: PASS
  - RGBA16Float accumulation: PASS
  - Values >1 survive scene shading: PASS
  - Explicit per-view exposure: PASS
  - Tone-map output pass: PASS
  - sRGB encoding exactly once: PASS
  - SceneWorld isolation: PASS
  - Material compile regression: PASS
  - Mesh upload regression: PASS
  - Texture upload regression: PASS
  - Resize / zero-size integration: PASS
  - Editor overlay after tone-map: PASS
- Manual checks: HDR highlight rolloff visible: PASS. Exposure − recovers highlight detail: PASS. Exposure + brightens the HDR scene predictably: PASS. Reset Exposure returns to baseline: PASS. Editor gizmo colors remain display-space / exposure-independent: PASS. The rotated-away black surface was not part of this acceptance.
- Validation: On 2026-09-23 the human confirmed the checks above. `JARVIGEditor.exe --self-test` on Intel UHD DX12 still prints `hdr_scene_format=rgba16float`, exposure 0 EV, one tone-map pipeline, and matching client, surface, HDR, and depth.
- Documentation: `docs/rendering/hdr.md`, `docs/rendering/post-processing.md`, ADR-0035.

## JRV-0072 — Environment light foundations

- Status: ACCEPTED / HUMAN VISUAL CONFIRMED
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0071
- Goal: One scene environment contributes normal-dependent diffuse irradiance. A dielectric turned away from the three direct lights must not be pure black. The environment can be switched off. It is not a constant added inside the direct-light loop, and it is not full GI. Metals do not receive this diffuse term. Specular environment reflections are JRV-0073.
- Acceptance Criteria:
  - Environment diffuse lighting: PASS
  - Rotated-face environment response: PASS
  - Two-sided reverse-face lighting: PASS
  - Back-face normal correction: PASS
  - Tangent-basis correction: PASS
  - Lighting follows live transform: PASS
  - Environment toggle behavior: PASS
  - Direct-light separation: PASS
  - HDR integration: PASS
  - Large-world integrity: PASS
- Manual checks: Lighting follows rotation continuously: PASS. The reverse visible side now lights correctly: PASS. Moving the camera around the rotated two-sided cards remains coherent: PASS. Back-face normal/tangent fix is visually confirmed: PASS. Environment diffuse keeps the dielectric readable away from direct lights: PASS.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core -p jarvig_material -p jarvig_renderer -p jarvig_engine -p jarvig_editor` passed. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JRV-0072 environment rotation=PASS rotate_back=PASS direct_plus_environment=PASS disabled=PASS metallic=PASS ao=PASS resource_regression=PASS live_authoring=PASS`, `environment_light_count=1 environment_enabled=1 environment_intensity=0.200 environment_upper_rgb=0.550,0.680,0.860 environment_lower_rgb=0.220,0.160,0.110 environment_packet_upload_count=4 environment_diffuse_near=0.0770,0.0840,0.0970`, and `JARVIG_OK editor frames=24`. Compiles stayed 1. `jarvig_editor_host --self-test` printed `JARVIG_OK native-present frames=4` with `environment_packet_upload_count` held at 1 across four frames and `buffer_count=19` after the zero-size cycle recreated the two exposure uniforms.
- Documentation: `docs/rendering/environment-lighting.md`, `docs/rendering/lighting-phase-2.md`, ADR-0036.

## JRV-0073 — Specular environment reflections

- Status: ACCEPTED / HUMAN VISUAL CONFIRMED
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0072
- Goal: Roughness-aware environment reflections on the existing hemisphere light. Metals stay readable when the direct highlight leaves. Rough surfaces must not stay as sharp as smooth ones. Not a cubemap, not screen-space reflections, and not a placed probe.
- Acceptance Criteria:
  - Metal remains environmentally lit after the direct hotspot leaves: PASS
  - Environment reflection changes as the camera orbits: PASS
  - Environment reflection follows object rotation: PASS
  - Reverse-side/two-sided reflection remains valid: PASS
  - Roughness changes reflection character: PASS
  - Environment off removes the indirect specular response: PASS
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_material -p jarvig_core --lib` passed, including roughness, Fresnel, metal, dielectric, and a disabled environment. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JRV-0073 specular rotation=PASS camera=PASS roughness=PASS metallic=PASS dielectric_model=PASS reverse=PASS disabled=PASS exposure=PASS environment_reflection_updates=4`, `environment_specular_enabled=1 environment_specular_model=analytical environment_specular_far=0.0685,0.0748,0.0863`, and `JARVIG_OK editor frames=34`. Compiles stayed 1. Client, surface, HDR, and depth stayed 749×441.
- Documentation: `docs/rendering/environment-lighting.md`, ADR-0038.

## JRV-0074 — Local reflection probes

- Status: Accepted. Human visual confirmed on 2026-09-23.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0073
- Goal: One sphere probe captures the opaque scene into an HDR cubemap. Objects inside the radius sample it. Objects outside keep the JRV-0073 sky. The material does not name the probe. Capture is static.
- Acceptance Criteria: Accepted. The large metal shows captured scene radiance that changes with camera direction and surface orientation, and that radiance is distinct from the analytical sky. The rectangular shapes were a known limit of the 32² static box-filter mips, not a failed capture. JRV-0078 replaces that filter. The resolution, the one-time capture, and the lack of a production seam filter remain.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core -p jarvig_material -p jarvig_rhi -p jarvig_renderer -p jarvig_engine --lib` passed. `jarvig_editor_host --self-test` on Intel UHD DX12 printed `pipeline_count=4`, `indexed_draw_count=28`, `draw_count=96`, `buffer_count=26`, `texture_upload_count=5`, `material_compile_count=1`, and `JARVIG_OK native-present frames=4`. `JARVIGEditor.exe --self-test` printed `JRV-0074 probe inside=PASS outside=PASS restore=PASS camera=PASS reverse=PASS disabled=PASS roughness_lod=0.50->4.50 capture_count=1`, `reflection_probe_count=1 reflection_probe_active_count=1 reflection_probe_capture_count=1 reflection_probe_selected_far=1 reflection_probe_fallback_count=0 reflection_probe_resolution=32 reflection_probe_mip_count=6`, and `JARVIG_OK editor frames=41`. Compiles stayed 1. Client, surface, HDR, and depth stayed 749×441. No framebuffer was read back, so the checker card's presence in the reflection is not proven by a pixel.
- Documentation: `docs/rendering/environment-lighting.md`, ADR-0039.

## JRV-0075 — Indirect diffuse

- Status: Accepted. Human visual confirmed on 2026-09-23. Indirect Diffuse Only leaves the gold card black and shows warm bounce on the checker when that card is turned toward it. This is static probe irradiance, not dynamic GI.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0072, JRV-0074
- Goal: One bounced diffuse term from the captured probe. Not a constant ambient, not a change to the direct BRDF, and not real-time GI from every surface.
- Acceptance Criteria: The term is separate from direct light, the analytical sky, specular, and emissive. It is cosine irradiance of the existing capture, built once. Metals do not receive it. Shadows do not darken it. Camera motion does not recapture. A human should see the checker change when this term is isolated, and see the other card's color when a face points at it.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core --lib probe::` passed, including the cosine hemisphere. `-p jarvig_renderer --lib` passed 23. `-p jarvig_material --lib` passed. `jarvig_editor_host --self-test` on Intel UHD DX12 printed `pipeline_count=6`, `draw_count=118`, `alive_textures=16`, compiles 1, and `JARVIG_OK native-present frames=4`. `JARVIGEditor.exe --self-test` printed `JRV-0075 indirect indirect_diffuse_texture_count=1 indirect_diffuse_samples=64 capture_count=1`, compiles 1, and `JARVIG_OK editor frames=41`. No framebuffer was read back.
- Documentation: `docs/rendering/environment-lighting.md`, ADR-0044.

## JRV-0085 — Project and level documents

- Status: Accepted. Human save-reload confirmed on 2026-09-23. The Lighting Lab survived close and reopen and stayed editable.
- Milestone: P1
- Subsystem: core, engine, editor
- Dependencies: ADR-0040, ADR-0041, ADR-0033, ADR-0026
- Goal: A project file and a level file describe the authored world. The renderer is not part of the save. `EntityUuid` stays the persistent identity.
- Acceptance Criteria: File > New Project, Open Project, New Level, Open Level, Save, Save As, and Save All exist. The Lighting Lab sample loads from `samples/lighting-lab`. Moving an actor, saving, and reopening restores the uuid, name, parent, transform, light, probe, environment, and builtin mesh/material reference. Runtime handles may differ. A corrupt file, a newer schema, a duplicate uuid, a bad parent, a cycle, an unknown component, and a missing asset do not replace the open world. A failed save leaves the previous level. Autosave does not overwrite the `.jarviglevel`. The editor camera is not saved in the level. The server can load the document and still does not touch a GPU. `--self-test` stays the two-triangle bootstrap.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core -p jarvig_engine --lib` passed, including the Lighting Lab round trip, the bad-file cases, the atomic backup, and `server_loads_the_lighting_lab_without_a_gpu_and_the_editor_round_trips_a_move`. `cargo test -p jarvig_editor` passed 42 tests. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JARVIG_OK editor frames=41`, `objects=2`, `entity_count=7`, `reflection_probe_resolution=32`, and `capture_count=1`. The interactive open/save/relaunch path was not exercised by that self-test.
- Documentation: ADR-0045, `docs/architecture/serialization.md`.

## JRV-0088 — Real PBR material ingest

- Status: Implemented. Not human-accepted. Hold open for a PBR validation look. Do not start TAA, displacement, dynamic GI, or cluster geometry.
- Milestone: P1
- Subsystem: core, engine, editor
- Dependencies: JRV-0053, JRV-0085, JRV-0087
- Goal: Staged sets reach the Lighting Lab through one generic path. The level stores the set name. It does not store PNG pixels.
- Acceptance Criteria: The floor is Tiles101 at a readable UV scale. A plane, cube, and sphere share that set. Gravel035 uses the same path and no set-specific shader. Base color, normal, roughness, AO, and metallic can be isolated without saving that view. DirectX versus OpenGL is proven from the tangent frame and the Tiles101 files, not from DX12. Roughness multiplier, UV scale, and normal strength change the shading without a recompile. AO does not darken direct light. Normal maps do not move shadow silhouettes. Mips and anisotropy stay viable on Intel UHD. Height stays metadata. Save and reload keep the set name and the instance factors, not the pixels.
- Validation: On 2026-09-24 the Tiles101 DirectX and OpenGL files matched as one green flip, and the floor frame shaded the DirectX texel the same way as the flipped OpenGL texel. `cargo test --offline` passed for core, material, rhi, renderer, and engine. The editor test decoded both normal files. Not human-accepted. The self-test still does not open the Lighting Lab.
- Documentation: `docs/rendering/material-sets.md`.

## JRV-0089 — Native joint pose and primitive mannequin

- Status: Regression fixture. The human saw the hierarchy on 2026-09-30. It stays the joint test. It is not the character product and it is not Done.
- Milestone: P1
- Subsystem: core, engine, editor
- Dependencies: ADR-0049, ADR-0053, JRV-0085
- Goal: One character pose, which is the entity's local frame, plus a JARVIG-owned humanoid saved as an ordinary prefab.
- Acceptance Criteria: Joint kinds are Fixed, Hinge, Ball, Universal, and Prismatic. Rest pose, limits, stiffness, and damping are stored. Stiffness and damping are not solved. Shoulders and hips are ball joints. Elbows and knees are hinges. Wrists and ankles are present. Fingers are not. The hierarchy shows in the World Outliner. Selecting a joint shows its type, rest pose, limits, and current local rotation. Editor rotation clamps to those limits and children follow the existing frame parent. Debug draws pivots, axes, and the selected joint's swing or hinge range. The mannequin round-trips as `jarvig.prefab`. Editing one pose does not change an unrelated actor. Physics, ragdoll, IK, skinning, clips, root motion, and FBX skeletons are absent.
- Validation: On 2026-09-30, `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib -- joint:: prefab::` passed 7 tests. `bad_levels_do_not_instantiate` passed after a version-4 file was rejected and a `jarvig.asset` mesh with no id was rejected as corrupt. `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_engine --lib` passed 8 tests. `cargo check --offline --manifest-path native/Cargo.toml -p jarvig_editor` exited 0. The sample files are under `samples/primitive-mannequin/`. Human visual acceptance has not been recorded. No GPU frame of the mannequin was run.
- Documentation: ADR-0053, `docs/animation/overview.md`, `docs/architecture/game-framework.md`.

## JRV-0090 — Kinematic FPS pawn

- Status: Not started. Do not start until JRV-0089 is accepted on screen.
- Milestone: P1
- Subsystem: core, engine, editor
- Dependencies: JRV-0089, ADR-0053
- Goal: A kinematic first-person pawn for the Quake-style example.
- Acceptance Criteria: Mouse look, WASD, jump, gravity, ground detection, step and slope handling, and collision. The pawn drives the existing local frame. Physics, ragdoll, IK, skinning, animation clips, root motion, and FBX skeletons stay out. Ghost City is a map sample and is not the character definition.
- Validation: Not run.
- Documentation: ADR-0053 names the pawn as a later consumer. This entry records the milestone. It does not authorize the work.

## JRV-0091 — Character editor workspace

- Status: Implemented. Not human-accepted. The imported ball-joint doll is not this ticket. FBX is not an importer.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0089, ADR-0053, ADR-0054
- Goal: A character workspace over the same spatial frames and joints. The level editor stays the world tool.
- Acceptance Criteria: A joint-only level opens with a Skeleton tree, a Character Preview, and the joint inspector. The tree suffix is the joint kind. Parent and child are shown. Reset Pose restores every rest pose. Show Joints and Show Joint Limits toggle the overlay. Double-clicking the prefab does not place a second copy. Dragging it still does. Animation, IK, skinning, and ragdoll controls are absent.
- Validation: On 2026-09-30, `cargo check --offline --manifest-path native/Cargo.toml -p jarvig_editor` exited 0. The on-screen character workspace has not been accepted.
- Documentation: ADR-0054.

## JRV-0092 — Default base character meshes

- Status: Implemented. Not human-accepted. Base and Base Male are rigid parts on Joint and SpatialFrame. They are not skinned.
- Milestone: P1
- Subsystem: core, editor
- Dependencies: JRV-0022, ADR-0053, ADR-0055
- Goal: The engine's default bodies are Base and Base Male, in the same role as Unreal's mannequin and female mannequin. The primitive mannequin stays the joint fixture.
- Acceptance Criteria: Each source contributes one standing body, reconstructed from the source node transforms. Disconnected posed copies and the outline shell stay out. Base Male includes `upperArmMesh.002` and `shoulderMesh.002` and excludes `foreArmMesh.002`. Feet sit near y = 0. The character file is `jarvig.character` version 1 in source space. Each rigid part's frame origin is the ball that joins it to its parent, and the mesh stays in the source world place. The startup level adds one root translation and opens in the level editor. View > Character Editor, or a double-click, opens the character workspace and does not place a copy. A click on a part selects that part. A joint-debug marker selects that joint only in the Character Editor, and the gizmo handle is tested before either pick. Limits are wide defaults. Collision and the animation set are null. FBX is not an importer. Skinning, clips, IK, and ragdoll are absent. The meshes stay local and credited. Lighting Lab is unchanged.
- Validation: On 2026-10-01 the first bind-pose import passed at 35 and 38 parts and six GPU frames were written. Those frames stacked every shared-node pivot at the feet. The human rejected that. Later the same day, `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib base_characters_import_as_bind_pose_meshes -- --ignored --nocapture --test-threads=1` passed in 100.09 s. Base is 66 parts, 158,224 triangles, feet y = −0.000359. Base Male is 66 parts, 150,752 triangles, feet y = 0.002134. Lighting Lab bytes were unchanged. `JARVIGEditor --project samples/base-characters/BaseCharacters.jarvigproject --bind-pose-shots samples/base-characters/bind-pose` printed `BIND_POSE_SHOTS_OK` on Intel UHD DX12 and rewrote the six PNGs with joint debug on. The window is the level editor. The human has not accepted those frames. Later the same day, `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib play::` passed 3 tests. `cargo build --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor` exited 0 with the existing unread `EinsteinCapture.height` warning. `JARVIGEditor --self-test` printed `JARVIG_OK editor frames=41` and `JRV-0065 probe pick=PASS centimeter=PASS` on Intel UHD DX12. That self-test does not open Base Characters and does not click a joint marker. The same day, `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib the_joint_marker_is_a_small_cross_and_a_short_axis` passed, and the editor build exited 0. Show Joints now draws the selected joint as a 2.4 cm cross. View > Show All Joints draws the rest and defaults off. No new GPU frame of that overlay was captured.
- Documentation: ADR-0055, ADR-0056, ADR-0057, `docs/animation/overview.md`, `docs/architecture/play-in-editor.md`, `samples/base-characters/CREDITS.md`, `samples/base-characters/bind-pose-report.txt`.

## Terrain foundation

- Status: Implemented under ADR-0058. Not a numbered ticket. Not human-accepted. Einstein terrain detail is stored and does not generate geometry. The Land grid is painted on the terrain surface and is waiting for a human look.
- The authoritative ground is one chunked heightfield. Empty World is World Settings only. Level, Land, and Character are editor views of that same world. The toolbar shows those words. Land creates and sculpts the heightfield. The grid is a depth-tested pass on the chunk triangles, not a saved object. Minor lines fade with camera distance and grazing angle before major lines, and both are gone at the horizon. Vertex marks are dots. The brush cursor shows radius, falloff, and strength. Land hides characters, props, and gameplay actors until Show Full Level. The mode and those overlay settings are remembered in `Saved/Editor/workspace.json` and stay out of Level. A sculpt writes samples immediately and rebuilds only the dirty chunk meshes on the background job queue. Page streaming, foliage, erosion, water, voxel terrain, and procedural world generation stay out. JRV-0090, JRV-0091, and JRV-0092 are unchanged.

## JRV-0087 — Renderer presentation and lighting stability

- Status: Accepted. Human visual confirmed on 2026-09-24. The structured diagonal floor artifact no longer reads as a shadow or filter failure. Final display quality is not claimed.
- Milestone: P1
- Subsystem: renderer, core, editor
- Dependencies: JRV-0071, JRV-0086, ADR-0046
- Goal: Quieter Lighting Lab lighting without changing authored intensities, and one hardware-independent quality budget.
- Acceptance Criteria: A flat lit surface does not show a crawling ripple when the camera flies. The cause is identified. Shadow edges still meet the floor. Camera motion does not rebuild stationary point and spot maps. Moving an object rebuilds the maps that object affects. Close reflections stay on mip 0 at the level's capture size. Baseline, Enhanced, and High are not vendor names and are not selected from integrated versus discrete. The Lighting Lab file is not rewritten. Intel UHD still runs the same renderer.
- Validation: Human visual confirmed on the normal editor, not `--self-test`. The source is presentation and quantization, not a new shadow bug. The diagonal interleaved-gradient weave was replaced with finer triangular noise. View > Presentation Dither, 8-bit Steps, and Before Tone Curve isolate it. The Lighting Lab file was not rewritten. Broad 8-bit steps and some colored dither on large highlights remain, and the 64² mirror stays a separate limit. On 2026-09-24, `JARVIGEditor.exe --self-test` on Intel UHD DX12 still printed `JARVIG_OK editor frames=41`. That run does not open the lab. GPU timestamps are not available. `cpu` is the shadow-record time for that frame.
- Documentation: `docs/rendering/hdr.md`, `docs/rendering/shadows.md`, `docs/rendering/environment-lighting.md`, ADR-0046.

## JRV-0086 — Shadow quality

- Status: Implemented. Not human-accepted. Stop here for a visual look. Do not start dynamic GI.
- Milestone: P1
- Subsystem: renderer, core, editor
- Dependencies: JRV-0077, JRV-0085, ADR-0046
- Goal: Stable, physically credible direct-light visibility on the Lighting Lab. Same PBR model, environment, probes, and indirect diffuse.
- Acceptance Criteria: Four texel-snapped cascades. Meter bias so the smooth sphere meets the floor. PCF plus contact-hardening on directional and spot. Point-cube filtering without a face seam. Short optional screen-space contact. Authored shadow settings save into the level and dirty it. Lighting Debug does not. Camera motion does not rebuild stationary point and spot maps. A direct shadow does not erase sky, reflections, indirect diffuse, or emissive. Intel UHD remains a first-class baseline, not a low-end renderer. Hardware ray tracing is not required.
- Validation: On 2026-09-23, `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core -p jarvig_material -p jarvig_renderer -p jarvig_engine --lib` passed, including cascade snap, meter bias, and the Lighting Lab parse. `jarvig_editor` passed 42 tests. `jarvig_editor_host --self-test` on Intel UHD DX12 printed `JARVIG_OK native-present frames=4`. `JARVIGEditor.exe --self-test` printed `JARVIG_OK editor frames=41`, `objects=2`, `entity_count=7`, `reflection_probe_resolution=32`, `capture_count=1`, and `shadow_directional_px=1024`. A later presentation pass replaced the aligned PCF grid with a 12-tap disk, packed shadow depth into two fp16 channels, softened the cascade blend, and faded grazing contact shadows. The floor, the penumbra, and the mirror still need a human look. They are not pixel-proven.
- Documentation: ADR-0046, ADR-0047, `docs/rendering/shadows.md`.

## JRV-0076 — Emissive contribution through probes

- Status: Implemented. Not human-accepted. Not a hidden point light and not multi-bounce GI.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0053, JRV-0074, JRV-0075
- Goal: An emissive surface stays self-lit. The same HDR radiance is in the probe capture, so metal can reflect it and a dielectric can pick it up as indirect diffuse after that capture.
- Acceptance Criteria: The red panel glows on itself. After the probe that includes it is captured, a metal surface reflects it and a white dielectric shows a red bleed on the face that points at it. Changing emissive marks lighting dirty. Camera motion does not recapture.
- Validation: The capture shader already writes emissive before the probe is convolved. `JARVIGEditor.exe --self-test` on Intel UHD DX12 still printed `JARVIG_OK editor frames=41` with `capture_count=1`. The lab panel is only in the interactive editor, so the red bleed is not pixel-proven.
- Documentation: `docs/rendering/environment-lighting.md`.

## JRV-0080 — Shadow filtering and bias

- Status: Implemented. Not human-accepted.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0077
- Goal: Soften the existing maps and separate constant bias from slope bias. Filtering stays inside direct-light visibility.
- Acceptance Criteria: Directional and spot use a 5×5 PCF. Point uses a 5-tap filter. Grazing receivers get more bias than faces aimed at the light. Environment, probes, emissive, and indirect diffuse stay unshadowed.
- Validation: Shader compiled on Intel UHD DX12 during `JARVIGEditor.exe --self-test` (`JARVIG_OK editor frames=41`). No pixel readback of the penumbra.
- Documentation: `docs/rendering/shadows.md`.

## JRV-0081 — Probe capture resolution

- Status: Implemented. Not human-accepted.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0074, JRV-0078, JRV-0075
- Goal: 32, 64, 128, and 256 are settings. Mip 0 stays the sharp capture. GGX mips stay specular. Irradiance stays a separate cosine cube. Cubemap edges are the hardware cube sample, not a 2D wrap.
- Acceptance Criteria: The interactive editor starts at 64. View can switch 32, 64, 128, and 256. The cube already on screen stays until the new capture finishes. The regression self-test stays at 32. A coarse reflection at 64 is not a filtering bug unless it is still coarse at 128 or 256.
- Validation: `reflection_probe_resolution_supported` accepts those four and rejects 512. On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core -p jarvig_material -p jarvig_renderer --lib` passed, including the 256 mip count and the budgeted-refresh tests. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JARVIG_OK editor frames=41`, `reflection_probe_resolution=32`, `reflection_probe_mip_count=6`, and `capture_count=1`. 128 and 256 were not pixel-proven.
- Documentation: `docs/rendering/environment-lighting.md`.

## JRV-0082 — Probe update policy

- Status: Implemented. Not human-accepted.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0074
- Goal: Static, On Demand, On Transform, On Lighting, and Time Sliced. One in-flight rebuild. One cubemap face per frame after the first cube, and for every Time Sliced capture. A later dynamic mode would spend that same budget. It does not recapture every probe every frame.
- Acceptance Criteria: Static captures once unless the probe is dirty. View > Recapture and a resolution change are dirty, and they do not replace the sampled cube until the new one finishes. On Transform ignores camera-only navigation. On Lighting ignores a pure object move. Time Sliced spreads the first capture. Recapture does not compile a material or reupload a mesh. The camera does not dirty a probe.
- Validation: Renderer tests `time_sliced_capture_finishes_across_frames_and_the_camera_does_not_reset_it` and `a_later_probe_refresh_keeps_the_old_cube_until_the_budget_finishes` passed. The bootstrap self-test path is still one synchronous Static capture. A forced recapture was not part of the editor self-test.
- Documentation: `docs/rendering/environment-lighting.md`.

## JRV-0083 — Lighting lab geometry

- Status: Implemented. Not human-accepted.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0047
- Goal: Engine-owned floor, cube, sphere, and emissive panel so lighting can be judged on more than two cards. Not imported assets.
- Acceptance Criteria: A normal `JARVIGEditor.exe` launch adds Floor, White Cube, Metal Sphere, and Emissive Panel beside the bootstrap cards. `--self-test` does not, so the regression scene stays two triangles.
- Validation: `--self-test` still reported `objects=2` and `entity_count=7`. The lab path was not executed by that test.
- Documentation: `docs/status/NEXT.md`.

## JRV-0084 — Lighting debug: direct unshadowed

- Status: Implemented. Not human-accepted.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0077, JRV-0075
- Goal: Keep every existing debug mode and add a direct-light view with shadows forced off.
- Acceptance Criteria: View > Lighting Debug > Direct Unshadowed shows the three direct lights and no shadows, sky, probe, bounce, or emissive. Full Lighting remains.
- Validation: The menu item is wired to `LightingDebug::direct_unshadowed`. Not a separate screenshot.
- Documentation: `docs/rendering/shadows.md`.

## JRV-0078 — GGX reflection-probe prefilter

- Status: Accepted. Human visual confirmed on 2026-09-23, on the assumption that the hard rectangular box-filter shapes are gone. The cube is still 32², so the reflection stays soft. That softness is the remaining limit, not a rectangle.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0074
- Goal: Replace the 32² box-filter mip chain with a GGX prefilter. The lod select stays. This is not a new probe actor and not a second capture policy. Not indirect diffuse. Not shadows.
- Acceptance Criteria: Accepted. Mip 0 stays the sharp capture. Higher mips are GGX at `roughness = mip / (mip_count - 1)`. Direct lights stay analytic. Capture still runs once. The hard rectangles are treated as gone. 32² softness remains.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core --lib probe::` and `-p jarvig_renderer --lib` passed. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JARVIG_OK editor frames=41`, `capture_count=1`, `roughness_lod=0.50->4.50`, compiles 1. The first shader compile used reserved WGSL words and aborted the process. That binary was rebuilt before the stamp.
- Documentation: `docs/rendering/environment-lighting.md`, ADR-0042.

## JRV-0079 — Authoring entity lifecycle

- Status: Implemented. Not a visual ticket. The tests below passed on 2026-09-23.
- Milestone: P1
- Subsystem: engine
- Dependencies: ADR-0040
- Goal: Duplicate and delete keep the entity registry and the subsystem record together. World Settings is a protected singleton. Deleting a parent reparents children to World.
- Acceptance Criteria:
  - Duplicate mesh: new uuid, new handle, shared mesh, independent transform, both render
  - Delete that mesh: row gone, instance gone, original remains
  - Duplicate point light: new uuid, new LightId, copied values, both extracted
  - Delete point light: extracted light gone, no material recompile
  - Duplicate probe: new uuid, new ProbeId, a separate cube, uncaptured until its own capture
  - Delete probe: logical probe gone, that cube retired, the other cube kept
  - World Settings: selectable, not duplicable, not deletable, not reparentable, no gizmo
  - Deleting the primary selection clears it; a survivor stays primary
  - Parent delete leaves no stale parent handle
  - Server duplicate/delete does not extract or compile
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core -p jarvig_engine -p jarvig_renderer --lib` passed, including `duplicate_and_delete_keep_subsystem_records_with_the_entity`, `duplicate_and_destroy_do_not_compile_or_give_the_server_a_gpu`, and `a_duplicated_probe_gets_its_own_cube_and_delete_retires_that_cube`. `cargo test -p jarvig_editor` passed 42 tests. `JARVIGEditor.exe --self-test` on Intel UHD DX12 printed `JARVIG_OK editor frames=41` with objects=2, lights=3, compiles=1, and `reflection_probe_capture_count=1`.
- Documentation: ADR-0041, `docs/editor/outliner.md`, `docs/architecture/entity-identity.md`.

## JRV-0077 — Direct-light shadows

- Status: Accepted. Human visual confirmed on 2026-09-23. The near card casts onto the far card. The reflection stays. The status line showed `dir 1024 spot 1024 point-cube 256 maps 3 updates 37`.
- Milestone: P1
- Subsystem: renderer
- Dependencies: JRV-0054, JRV-0071
- Goal: Real visibility for the existing directional, point, and spot lights. The shadow multiplies only that light's direct term. Not contact darkening, not an ambient term, and not indirect diffuse.
- Acceptance Criteria:
  - Directional light casts a shadow. Point light casts an omnidirectional shadow. Spot light casts a perspective-cone shadow.
  - Emissive, environment diffuse, environment specular, and reflection-probe radiance stay unshadowed. Probe capture binds the maps disabled.
  - Moving or rotating a caster, a receiver, or a light, or changing geometry, redraws the maps. Camera movement does not.
  - No float32 absolute-world matrix. Reversed-Z stays. A light or caster move does not recompile a material or reupload a mesh.
  - Two-sided cards cast from the geometric triangle. The shadow pass culls nothing.
  - Lighting Debug can show directional plus its shadow, point plus its shadow, spot plus its shadow, and shadows off.
  - The status line shows map type, resolution, count, and update count.
- Validation: On 2026-09-23, `cargo test --manifest-path native/Cargo.toml --offline -p jarvig_core --lib shadow::` passed 4 tests. `-p jarvig_renderer --lib` passed 23, including caster, receiver, light, rotate-back, large-world, resize, and camera-does-not-redraw. `-p jarvig_engine --lib` passed, including the headless server. `jarvig_editor_host --self-test` on Intel UHD DX12 printed `pipeline_count=5`, `indexed_draw_count=44`, `draw_count=112`, `buffer_count=31`, `alive_textures=15`, compiles 1, and `JARVIG_OK native-present frames=4`. `JARVIGEditor.exe --self-test` printed `JRV-0077 shadows shadow_map_count=3 shadow_directional_px=1024 shadow_spot_px=1024 shadow_point_px=256 shadow_update_count=23 shadow_cull=none`, `capture_count=1`, compiles 1, objects 2, and `JARVIG_OK editor frames=41`. No framebuffer was read back, so the moving shadow is not pixel-proven.
- Documentation: `docs/rendering/shadows.md`, ADR-0043.

## JRV-0056 — API / SDK / reflection foundation

- Status: Accepted
- Implementation: Intentionally deferred. ADR-0023 adds struct_size negotiation, calling-convention macros, and the bootstrap-header boundary. The SDK tree is not created.
- Milestone: P5
- Subsystem: platform
- Dependencies: ADR-0017, ADR-0018, ADR-0019
- Goal: Freeze how every external consumer talks to JARVIG before world, asset, material, and gameplay APIs spread.
- Acceptance Criteria: The layers, the C ABI rules, handles, errors, memory, threading, versioning, reflection, commands, plugins, the game module, scripting, and automation are written down. No scripting language is chosen. No wire protocol is chosen. The renderer path is unchanged. The doctrine is ADR-0022.
- Validation: Documentation review on 2026-09-22. No runtime suite. This ticket does not change code. `native/jarvig_core/include/jarvig_core.h` stays the bootstrap slice it already is.
- Documentation: `docs/api/`, ADR-0022. Review this doctrine before JRV-0050 or any broad public gameplay API.

## JRV-0044 — Vertex and index buffers

- Status: Accepted
- Visual confirmation: Pass. On 2026-09-22 a human ran `cargo run --manifest-path native/Cargo.toml -p jarvig_editor_host`, saw the clear plus the same interpolated RGB triangle, and shut the window down after 275 presented frames (`JARVIG_OK native-present frames=275`).
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0043
- Goal: The same triangle, drawn from persistent vertex and index buffers.
- Acceptance Criteria: `draw_indexed(3)` replaces the procedural vertex index. Buffers are created once. Null checks reject a missing pipeline, a missing index buffer, and an invalid buffer handle. The host does not own the geometry. wgpu stays in `jarvig_rhi_wgpu`.
- Validation: `cargo build` and `cargo test --manifest-path native/Cargo.toml --workspace` on 2026-09-22. Self-test presents four frames and requires `pipeline_count=1`, `buffer_count=2`, and `indexed_draw_count` equal to the frame count, including a programmatic resize and a zero-size frame. `pnpm lint`, `pnpm test`, and `pnpm smoke` passed. Dedicated server smoke stays `graphics=none`. The continuous host was then watched by a human, as noted above. A hand drag of the window and a title-bar minimize were not part of that session.
- Documentation: `docs/rendering/rhi.md`. Positions are object-local. They are not a float32 world origin. See ADR-0004.

## JRV-0045 — Camera and transforms

- Status: Accepted
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0044, JRV-0016, ADR-0004, ADR-0020
- Goal: Move the indexed triangle through object-local, reference frame, camera-relative float32, view, and projection. No float32 world matrix.
- Acceptance Criteria: The triangle vertices are object-local meters. The CPU keeps binary64 frame poses. A large origin still uploads a meter-scale delta. The camera belongs to the render view, not the host. Null RHI rejects a draw that skips the uniform binding.
- Validation: `cargo build` and `cargo test --manifest-path native/Cargo.toml --workspace` on 2026-09-22, including the billion-meter and 8,000,000 m cases. Self-test presents four frames with `pipeline_count=1`, `buffer_count=3`, and `indexed_draw_count=4`, including resize and a zero-size frame. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Dedicated server smoke stays `graphics=none`.
- Documentation: `docs/architecture/coordinate-frames.md`, ADR-0020. ADR-0021 later changed the perspective depth direction. It did not change this ticket's frame or camera contract.

## JRV-0046 — Depth buffer

- Status: Accepted
- Visual confirmation: Not yet. Automated validation passed. A human look at the near triangle in front of the gold one is still worth doing once.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0045, ADR-0021
- Goal: Depth-tested occlusion on the existing camera path. Reversed-Z, infinite far, clear 0, compare greater-or-equal.
- Acceptance Criteria: A nearer triangle stays visible when a farther one is drawn after it. The depth target matches the drawable size, is recreated on resize, and is not created at 0×0. The frame tree and camera-relative upload stay in place.
- Validation: `cargo build` and `cargo test --manifest-path native/Cargo.toml --workspace` on 2026-09-22. Reversed-Z tests: near maps to 1, distance falls toward 0, 1e7 m stays finite. Self-test: four presents, `pipeline_count=1`, `buffer_count=6`, `indexed_draw_count=8`, `depth=Some((1600, 900))`, zero-size does not allocate depth, restore recreates it. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/rendering/rhi.md`, ADR-0021. No human look at the two-triangle frame yet.

## JRV-0047 — Basic mesh

- Status: Accepted
- Visual confirmation: Optional. Not a blocker. Automated validation passed.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0046, ADR-0022
- Goal: An engine-owned local mesh, with submeshes and local bounds, uploaded through RHI handles. Not an asset and not a C API.
- Acceptance Criteria: Two bootstrap meshes replace the loose triangle buffers. Each has one submesh. Bounds stay object-local. The near mesh is drawn before the far mesh. Depth, frames, and the headless server are unchanged. `MeshId` is not added to `jarvig_core.h`.
- Validation: `cargo build` and `cargo test --manifest-path native/Cargo.toml --workspace` on 2026-09-22, including mesh range checks and a Uint32 index mesh. Self-test: `mesh_count=2`, `buffer_count=6`, `indexed_draw_count=8`, `depth=Some((1600, 900))`. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/rendering/rhi.md`. The logical mesh stays free of GPU types. Lifetimes are JRV-0048.

## JRV-0048 — GPU resource lifetimes

- Status: Accepted
- Generational handles: PASS
- Stale-handle detection: PASS
- Explicit retirement: PASS
- GPU-safe destruction: PASS
- Resource dependency validation: PASS
- Mesh residency separation: PASS
- Depth resize lifecycle: PASS
- Explicit shutdown: PASS
- Headless server: PASS
- Boundary isolation: PASS
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0047
- Goal: Engine-owned GPU lifetimes. A logical mesh can exist with no GPU object. Handles do not alias after reuse. Physical free waits for queue completion.
- Acceptance Criteria: Generational handles. Stale use and double release fail. Resize retires the old depth target and flush brings retired resources to zero. Evicting a `GpuMesh` keeps the logical mesh. Shutdown is explicit. The null backend agrees. No streaming, render graph, or device-loss recovery.
- Deferred: wgpu retirement waits for queue completion inside `flush`. Safe for bootstrap. Later multi-frame, streaming, and async upload work replaces that stall with submission-serial or fence retirement. Not part of JRV-0049.
- Validation: `cargo test --manifest-path native/Cargo.toml --offline --workspace` on 2026-09-22. Self-test: four presents, `pipeline_count=1`, `buffer_count=6`, `mesh_count=2`, `indexed_draw_count=8`, `depth=Some((1600, 900))`, `alive_buffers=6`, `retired=0`. Zero-size left no depth resident. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/rendering/resources.md`, ADR-0024.

## JRV-0049 — RenderView and multi-view

- Status: Accepted
- Multi-view model: PASS
- Shared world: PASS
- Shared mesh residency: PASS
- Independent cameras: PASS
- Independent render origins: PASS
- Per-view reversed-Z depth: PASS
- Single acquire: PASS
- Single present: PASS
- Resource lifetime integration: PASS
- Large-world integrity: PASS
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0048, ADR-0022
- Goal: More than one view of one world. A view is not a world, a renderer, or a swapchain.
- Acceptance Criteria: Two views share logical meshes and GPU mesh residency, use different cameras and origins, and draw into one surface that is acquired and presented once. Viewports do not let the second view clear the first. A destroyed view id goes stale. Resize and zero size keep the ids. No public C view type.
- Validation: `cargo test --manifest-path native/Cargo.toml --offline --workspace` on 2026-09-22. Self-test: four presents, `view_count=2`, `target_count=1`, `mesh_count=2`, `buffer_count=8`, `indexed_draw_count=16`, `acquires=1`, `presents=1`, `alive_buffers=8`, `retired=0`, depth full target, viewports 800×900 side by side. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/rendering/views.md`, ADR-0025. Human look at the split frame is still open.

## JRV-0050 — Scene extraction

- Status: Accepted
- Authoritative world ownership: PASS
- Single extraction/frame: PASS
- Snapshot isolation: PASS
- Multi-view snapshot sharing: PASS
- High-precision pose preservation: PASS
- Per-view camera-relative conversion: PASS
- Shared mesh residency: PASS
- Headless-server extraction skip: PASS
- Renderer/world dependency boundary: PASS
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0049, ADR-0025
- Goal: The renderer draws a derived snapshot. It does not query the authoritative world.
- Acceptance Criteria: One extraction per presented frame feeds both views. A world edit does not change the snapshot already extracted. Instances reference logical meshes, not copied vertices. Hiding an instance drops the submission and keeps the mesh. The server does not extract. No public C render-instance type.
- Validation: `cargo test --manifest-path native/Cargo.toml --offline --workspace` on 2026-09-22. Self-test: four presents, `scene_instances=2`, `extractions_this_frame=1`, `mesh_count=2`, `buffer_count=8`, `view_count=2`, `acquires=1`, `presents=1`, `indexed_draw_count=16`, `alive_buffers=8`, `retired=0`. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/rendering/scene-extraction.md`, ADR-0026. Human look at the split frame is still open.

## Follow-up — Native entity identity

The number is JRV-0060, and the native registry is implemented and waiting for review. `ObjectId` remains the bootstrap slot. `EntityId` is the UUID. Do not invent a second id inside the outliner.

## JRV-0051 — First material / Material IR foundation

- Status: Accepted
- Material Graph: PASS
- Graph validation: PASS
- Material IR independent of WGSL: PASS
- Material compiler: PASS
- Master Material: PASS
- Material Instances: PASS
- Runtime parameter override: PASS
- No shader recompile on Tint change: PASS
- Pipeline sharing: PASS
- Mesh/material separation: PASS
- Material-slot binding: PASS
- Render snapshot integration: PASS
- Multi-view: PASS
- Large-world architecture: PASS
- Headless server: PASS
- Milestone: P5
- Subsystem: materials
- Dependencies: JRV-0050, ADR-0007
- Goal: A surface is a material graph lowered to Material IR, compiled once, and instanced. It is not a color, a shader string, or a pipeline stored on a mesh.
- Acceptance Criteria: One Surface/Unlit/Opaque master. Two instances override Tint without a second compile or a mesh re-upload. Submesh slots resolve through the render instance. The snapshot carries logical material ids only. The server does not compile. No public C material id.
- Validation: On 2026-09-22, `cargo build --manifest-path native/Cargo.toml --offline --workspace` and `cargo test --manifest-path native/Cargo.toml --offline --workspace` passed. Self-test: four presents, `material_master_count=1`, `material_instance_count=2`, `material_compile_count=1`, `material_pipeline_count=1`, `material_parameter_upload_count=2`, `buffer_count=10`, `mesh_count=2`, `scene_instances=2`, `extractions_this_frame=1`, `view_count=2`, `acquires=1`, `presents=1`, `indexed_draw_count=16`, `alive_buffers=10`, `retired=0`, depth full target, clean shutdown. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`. A tint change in the renderer test left compile count and pipeline count at 1 and mesh uploads at 2, and the parameter upload count went from 2 to 3.
- Documentation: `docs/materials/overview.md`, `material-graph.md`, `material-ir.md`, `master-materials.md`, `material-instances.md`. ADR-0007 clarified, not replaced. Human look at the split frame is still open.

## JRV-0052 — Texture and sampler

- Status: Accepted
- Human visually confirmed: PASS
- Texture sampling: PASS
- UV0 interpolation: PASS
- Repeat addressing: PASS
- Material texture binding: PASS
- Material tint: PASS
- Multi-view: PASS
- Shared residency: PASS
- Depth occlusion: PASS
- Milestone: P5
- Subsystem: materials
- Dependencies: JRV-0051, ADR-0027
- Goal: A logical Texture2D and sampler flow through the material graph, IR, and GPU residency. Color space is metadata. This is not an image importer.
- Acceptance Criteria: One master compiles once. Two instances bind different sRGB checkers and can change texture or sampler without a new shader, pipeline, or mesh upload. Both views share the upload. The server creates no GPU texture. A mesh without TexCoord0 is rejected.
- Validation: On 2026-09-22, `cargo build` and `cargo test --manifest-path native/Cargo.toml --offline --workspace` passed. Self-test: four presents, `material_compile_count=1`, `material_pipeline_count=1`, `texture_count=3`, `texture_upload_count=2`, `sampler_count=1`, `gpu_sampler_count=1`, `view_count=2`, `acquires=1`, `presents=1`, `indexed_draw_count=16`, `alive_buffers=10`, `retired=0`, clean shutdown. `pnpm lint`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`.
- Documentation: `docs/materials/textures.md`, ADR-0027. Human look confirmed.

## JRV-0053 — Standard metallic/roughness PBR

- Status: Accepted
- Milestone: P5
- Subsystem: materials
- Dependencies: JRV-0052, ADR-0027
- Goal: A canonical metallic/roughness surface. The graph writes physical properties. The BRDF is policy. Lights stay on JRV-0054.
- Acceptance Criteria: `StandardMetalRough` beside Unlit. One master, two instances, one compile, one pipeline. Factor and texture changes do not recompile. Base color and emissive are sRGB-to-linear. Metallic, roughness, AO, normal, and height are linear. Height is not a BRDF input. Normals use inverse-transpose and a +Y map. The server does not compile. The live image is base color plus emissive, not a claim of lit PBR.
- Validation: On 2026-09-22, `cargo test --manifest-path native/Cargo.toml --offline --workspace` passed. Self-test: four presents, swapchain `Bgra8UnormSrgb`, `pbr_master_count=1`, `pbr_instance_count=2`, `pbr_compile_count=1`, `material_compile_count=1`, `material_pipeline_count=1`, `material_parameter_upload_count=2`, `texture_count=9`, `texture_upload_count=5`, `sampler_count=1`, `gpu_sampler_count=1`, `buffer_count=10`, `mesh_count=2`, `scene_instances=2`, `view_count=2`, `extractions_this_frame=1`, `indexed_draw_count=16`, `alive_buffers=10`, `retired=0`, `acquires=1`, `presents=1`, clean shutdown. `pnpm lint`, `pnpm typecheck`, `pnpm test` (29), and `pnpm smoke` passed. Server smoke stays `graphics=none`. The later lit frame is JRV-0054. This ticket's preview was not claimed as lit PBR.
- Documentation: `docs/materials/pbr.md`, overview, graph, IR, instances, textures, rendering overview, scene extraction, lighting. ADR-0028. ADR-0007 and ADR-0027 clarified, not replaced.

## JRV-0008 — Embedded runtime viewport

- Status: The native view is JRV-0058, waiting for review. This founding ticket is not a second viewport and not a page-owned canvas. Not accepted on its own.
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0007
- Goal: The editor displays the live engine camera.
- Acceptance Criteria: A visible view matches the engine camera, not a fake preview.
- Validation: The JRV-0058 self-test presents that view. Do not also build a DOM canvas for this ticket.
- Documentation: `docs/editor/viewport.md`, `docs/architecture/editor-runtime.md`.

## JRV-0009 — Outliner and selection

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0007
- Goal: Hierarchy selection stays synchronized.
- Acceptance Criteria: Selecting in the outliner selects the same entity elsewhere, and the reverse.
- Validation: Not run.
- Documentation: `docs/editor/overview.md`.

## JRV-0010 — Transform inspector and undo

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0009, JRV-0006
- Goal: Transform edits undo and redo.
- Acceptance Criteria: An edit can be undone and redone as a transaction.
- Validation: Not run.
- Documentation: `docs/architecture/editor-runtime.md`.

## JRV-0011 — Gizmo transaction coalescing

- Status: Not started
- Milestone: P1
- Subsystem: editor
- Dependencies: JRV-0010
- Goal: A drag is one undo record.
- Acceptance Criteria: A continuous gizmo drag pushes a single undo entry.
- Validation: Not run.
- Documentation: `docs/architecture/editor-runtime.md`.

## JRV-0012 — Project schema v1

- Status: Done
- Milestone: P0
- Subsystem: assets
- Dependencies: none
- Goal: The CLI validates a project fixture.
- Acceptance Criteria: `jarvig.project/v1` accepts the fixture and rejects a broken document.
- Validation: `pnpm smoke` runs `jarvig project validate tests/fixtures/minimal.project.json`. Unit test covers a rejection. Passed 2026-09-22.
- Documentation: `docs/architecture/serialization.md`.

## JRV-0013 — Scene schema v1

- Status: Done
- Milestone: P0
- Subsystem: ecs
- Dependencies: JRV-0005, JRV-0006
- Goal: Scene snapshots round-trip against a golden file.
- Acceptance Criteria: Serialize matches `tests/golden/health-scene.json` and loads back.
- Validation: `tests/unit/foundation.test.ts`. Passed 2026-09-22.
- Documentation: `docs/architecture/serialization.md`.

## JRV-0014 — PIE snapshot or copy-on-write

- Status: Not started
- Milestone: P2
- Subsystem: editor
- Dependencies: JRV-0010, JRV-0013
- Goal: Stop discards runtime changes.
- Acceptance Criteria: After Play and mutations, Stop restores the exact authoring snapshot.
- Validation: Not run.
- Documentation: `docs/architecture/play-in-editor.md`.

## JRV-0015 — PIE pause, step, eject

- Status: Not started
- Milestone: P2
- Subsystem: editor
- Dependencies: JRV-0014
- Goal: Debug controls work in a sample.
- Acceptance Criteria: Pause, step, and eject operate on the simulation world without writing the authoring world.
- Validation: Not run.
- Documentation: `docs/architecture/play-in-editor.md`.

## JRV-0016 — 64-bit frame transform

- Status: Done
- Milestone: P0
- Subsystem: world
- Dependencies: JRV-0003
- Goal: Precision tests pass at an extreme distance.
- Acceptance Criteria: A sub-meter offset at 8e6 meters survives frame composition and is lost in a float32 absolute sum.
- Validation: `tests/unit/foundation.test.ts` hierarchical frames. Passed 2026-09-22. This is the math contract, not a rendered scene.
- Documentation: `docs/architecture/coordinate-frames.md`, ADR-0004.

## JRV-0017 — Camera-relative render transforms

- Status: In progress
- Milestone: P3
- Subsystem: render
- Dependencies: JRV-0016
- Goal: A far-origin jitter stress test passes in the renderer.
- Acceptance Criteria: A rendered or GPU-uploaded path keeps centimeter motion near a far camera. The null device does not count.
- Validation: `toCameraRelativeF32` is covered by the JRV-0016 test. No render stress scene exists.
- Documentation: `docs/architecture/coordinate-frames.md`, `docs/rendering/gpu-scene.md`.

## JRV-0018 — World-cell schema

- Status: Done
- Milestone: P0
- Subsystem: world
- Dependencies: JRV-0016
- Goal: Entity-to-cell assignment is deterministic.
- Acceptance Criteria: The same entities produce the same cells regardless of input order, including negative coordinates.
- Validation: `tests/unit/foundation.test.ts`. Passed 2026-09-22.
- Documentation: `docs/architecture/world-partition.md`.

## JRV-0019 — Async cell loader

- Status: Not started
- Milestone: P3
- Subsystem: world
- Dependencies: JRV-0018
- Goal: Loading a cell does not block the frame.
- Acceptance Criteria: A test path loads cell bytes without a synchronous frame stall.
- Validation: Not run.
- Documentation: `docs/architecture/streaming.md`.

## JRV-0020 — Streaming debug overlay

- Status: Not started
- Milestone: P3
- Subsystem: world
- Dependencies: JRV-0019
- Goal: State, cost, and reason are visible.
- Acceptance Criteria: An overlay or CLI dump shows why a cell is resident or pending.
- Validation: Not run.
- Documentation: `docs/architecture/streaming.md`.

## JRV-0021 — Derived-data keys

- Status: Done
- Milestone: P0
- Subsystem: assets
- Dependencies: none
- Goal: Source, settings, or tool changes invalidate the cache key.
- Acceptance Criteria: Identical inputs match. Each of those changes produces a different SHA-256. Key order does not.
- Validation: `tests/unit/foundation.test.ts`. Passed 2026-09-22. No on-disk cache yet.
- Documentation: `docs/assets/derived-data-cache.md`, ADR-0012.

## JRV-0022 — glTF import node

- Status: Accepted. Human lock-in 2026-09-25. The production fixture is `C:\tmp\townshop.glb`, asset `affe7d8d-e3e1-448a-a008-76c6e6266867`, not the earlier 50k–250k `curved-prop.glb` band. Selection of townshop is immediate through the stored triangle BVH. The imported model keeps its AssetId, renders with its own material, receives shadows, stays out of shadow-map redraws above the triangle cap, and the renderer never sees the source filename. JRV-0028 and JRV-0026 are accepted after it. JRV-0025 is human confirmed.
- Milestone: P4
- Subsystem: assets
- Dependencies: JRV-0021, JRV-0013
- Goal: Import one GLB/glTF 2.0 into a JARVIG mesh asset. The file does not become a level or a second scene format.
- Acceptance Criteria: The source file is kept. The asset has a persistent AssetId. The filesystem path is metadata, not identity. Derived data is JARVIG-owned positions, indices, normals, tangents, UV0, material slots, local bounds, and submesh ranges, so a later meshlet build reads this mesh and not the glTF. A malformed file fails cleanly with no half-created asset. An actor placed by an authoring command keeps its EntityUuid and stores AssetRef of that mesh. Built-in primitives stay. One external mesh, about 50k–250k triangles, with multiple material slots, curves, normals, a UV set, and more than one primitive, renders with an existing PBR material, casts and receives shadows, shows in a reflection, survives save and reopen, and runs in PIE and JARVIGGame.exe on Intel UHD. Lighting Lab is not rewritten to hold it. The renderer never sees the source filename.
- Validation: Import, save, reload, `JARVIGGame --server` (`entities=18`, `extractions=0`), one standalone frame, and `JARVIGEditor --self-test` (`frames=41`, `pick=PASS`) ran before the selection acceptance. The selection look itself is the human check on 2026-09-25.
- Documentation: `docs/assets/import-pipeline.md`.

## JRV-0023 — Cooker skeleton

- Status: Not started
- Milestone: P4
- Subsystem: assets
- Dependencies: JRV-0021, JRV-0012
- Goal: Client and server cook outputs exist.
- Acceptance Criteria: Two targets cook, and the server output has no graphics payload it does not need.
- Validation: Not run.
- Documentation: `docs/assets/cooking.md`.

## JRV-0024 — Render graph v1

- Status: Not started
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0008
- Goal: Pass dependencies and transient resources validate.
- Acceptance Criteria: A graph rejects an illegal pass order and reuses a transient resource safely.
- Validation: Not run.
- Documentation: `docs/rendering/overview.md`.

## JRV-0025 — GPU scene v1

- Status: Human confirmed 2026-09-25. The Lighting Lab status line showed inst 12, geom 12, meshlets 61702, about 3.8 MB, and an upload near 450 us while the ordinary shop stayed intact. Not a culling result. JRV-0024 remains unstarted.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0024, JRV-0017
- Goal: A compact instance database is uploaded.
- Acceptance Criteria: GPU or test double receives packed instances using camera-relative transforms.
- Validation: `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib two_instances_share` passed 2026-09-25. Two instances share one meshlet table. A camera at 1,000,000 m stores a 0.25 m relative translation. `JARVIGEditor --self-test` presented 41 frames on Intel UHD after the upload was added. Townshop descriptor counts are waiting on a human look in the open project.
- Documentation: `docs/rendering/gpu-scene.md`.

## JRV-0026 — GPU frustum culling

- Status: Accepted. Human confirmation 2026-09-25 on Intel UHD in the debug editor. The shop stayed the ordinary imported model. Show Meshlet Colors and Draw From Meshlets behaved. Submitted meshlets were about 60599/61702 with the shop filling the view, 19604/61702 at the edge, 2286/61702 as a sliver, and 0/61702 fully out of frame. This is the CPU sphere-versus-frustum test. There is no GPU dispatch. Occlusion is JRV-0027 and is not part of this acceptance.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0025
- Goal: GPU results match a CPU reference set.
- Acceptance Criteria: A fixture scene compares GPU-visible ids to the CPU set.
- Validation: `cargo test --offline --manifest-path native/Cargo.toml -p jarvig_core --lib a_side_cluster_is_rejected` passed 2026-09-25. On Intel UHD in the debug editor, townshop went from 60599/61702 submitted while filling the view, to 19604/61702 at the screen edge, to 2286/61702 as a sliver, to 0/61702 with the shop out of frame. Cull time on that CPU test was about 13–32 ms. The GPU id compare is not run.
- Documentation: `docs/rendering/visibility.md`.

## JRV-0027 — Hierarchical-Z occlusion

- Status: Accepted 2026-09-26 for conservative occlusion correctness, not for occlusion efficiency. Human review: the shaded meshlet path keeps the roof, cloth, beams, posts, banners, lantern, rear structure, and silhouette, and the ordinary indexed draw remains the fallback. The fixed front camera kept that silhouette through ordinary draw, meshlets, meshlets plus frustum, and meshlets plus frustum plus occlusion. State 4 was `61006/61702`, `fru 696`, `occ 0`, `cons 61006`, `tris 6054911`, cull 65.5 ms. Those `occ 0` frames are not a claim of townshop occlusion rate, a performance win, or production hierarchical-Z. The CPU sphere pyramid remains the reference. The chain is JRV-0022, JRV-0028, JRV-0025, JRV-0026, then this ticket. The meshlet partition stays. RFC-0002 stays unstarted. A missing canopy at some angles is not this ticket: it shows up with Draw From Meshlets off, and the file's only material is double-sided. The false facade rejection is visually cleared on Intel UHD: Draw From Meshlets with both culls on shows the awning, cloth, ropes, lantern, door, banners, and table. Occlusion rejected 0 clusters in that look. Submitted counts were 57779/61702 (fru 3923, cons 57776, 77.4 ms), 57211/61702 (fru 4491, cons 57207, 87.9 ms), and 55486/61702 (fru 6216, cons 55457, 71.7–88.4 ms). Almost every surviving cluster is conservative-visible because it is under 2 pixels on the 64 by 36 buffer, so this reference is drawing them on purpose and is not yet rejecting hidden townshop clusters. CPU time stays a reference cost. The ticket is accepted for non-overcull. `occ` 0 is recorded and is not an efficiency claim. RFC-0002, Einstein microgeometry, page streaming, and the GPU depth pyramid stay unstarted.
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0026
- Goal: A stable camera-motion test passes.
- Acceptance Criteria: Occlusion results are stable across a scripted camera move within a stated error.
- Validation: The facade regression failed before the correction and passed after it. Later Intel UHD views keep the canopy from above and from underneath on `colors off | draw original`, and the yellow meshlet view no longer deletes that roof. Every one of those frames has `occ 0`. A later Intel UHD set keeps the canopy in shaded and yellow views, with `occ` at 0 or 1. One front meshlet frame submitted all `61702/61702` with `fru 0` and `occ 0`. The fixed front camera is recorded. States 1 through 3 keep the canopy: ordinary draw, meshlets with both culls off, and meshlets with frustum on. State 4, meshlets with frustum and occlusion on, is `61006/61702`, `fru 696`, `occ 0`, `cons 61006`, `tris 6054911`, cull 65.5 ms, and the roof is still there. Occlusion rejected nothing in that frame, which is recorded and is not an efficiency claim. The ticket is accepted for non-overcull. A sphere that crosses the near plane stays submitted: `a_cluster_crossing_the_near_plane_stays_submitted`. `the_shared_standard_master_draws_both_sides` locks the shaded cull mode at `None`. Townshop `doubleSided` is read by `a_curved_two_slot`. A buried test cluster can still be occluded. Townshop at these distances does not reach that case.
- Documentation: `docs/rendering/visibility.md`.

## JRV-0028 — Meshlet preprocessing experiment

- Status: Accepted. Human visual confirmed 2026-09-25. Show Meshlet Colors covers the shop. Draw From Meshlets, with colors off, matches the ordinary imported material. Not a Nanite claim. JRV-0026 is accepted after it. JRV-0027 is accepted for conservative occlusion correctness.
- Milestone: P6
- Subsystem: render
- Dependencies: JRV-0022, RFC-0001
- Goal: Clusters, a debug view, and stats.
- Acceptance Criteria: An offline cook emits clusters and a debug view shows them. This is not a Nanite claim.
- Validation: Human visual confirmation 2026-09-25 on Intel UHD, plus the release townshop measure: 6,122,214 triangles, 3,419,738 vertices, 61,702 meshlets, average 99.22 triangles and 108.80 vertices, max 128/128, 50,155,678 bytes, build 1967.8 ms, sidecar write 216.6 ms, release decode 20.5 ms, debug editor load 701 ms, first debug upload 5942.7 ms, later toggles 0.00–0.03 ms. Exact cover including winding. `meshlet::` and `import_writes_an_asset_id` passed.
- Documentation: `docs/rendering/meshlets.md`.

## JRV-0029 — Dedicated server host

- Status: Done
- Milestone: P0
- Subsystem: net
- Dependencies: JRV-0003, JRV-0001
- Goal: The server boots a world with no graphics dependency.
- Acceptance Criteria: Server profile runs the world module, skips render, and its sources do not import `@jarvig/render`.
- Validation: `tests/integration/hosts.test.ts`, boundary lint, `pnpm smoke`. Passed 2026-09-22. There is no network protocol and no authored content world.
- Documentation: `docs/networking/overview.md`.

## JRV-0030 — AI session continuity

- Status: Done
- Milestone: P0
- Subsystem: platform
- Dependencies: none
- Goal: Agents resume from CURRENT.md without chat history.
- Acceptance Criteria: AGENTS.md defines the read order and the handoff block. CURRENT.md exists and is updated by this session.
- Validation: Files present. A second agent has not yet resumed from them. The ticket exit in the founding doc is the continuity mechanism, which is these files.
- Documentation: `AGENTS.md`, `docs/status/CURRENT.md`.

## JRV-0031 — Canonical material schema

- Status: Done
- Milestone: P4M
- Subsystem: materials
- Dependencies: none
- Goal: PBR inputs, shading model, and parameter metadata validate and round-trip.
- Acceptance Criteria: `jarvig.material/v1` accepts a baseline material and rejects an unknown shading model.
- Validation: `tests/unit/foundation.test.ts`. Passed 2026-09-22. No graph and no WGSL.
- Documentation: `docs/materials/overview.md`, `docs/materials/pbr.md`, ADR-0007.

## JRV-0032 — Texture semantic importer

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0031, JRV-0021
- Goal: sRGB, linear, normal, and channel mappings are explicit and tested.
- Acceptance Criteria: Import metadata records color space and channels. A wrong mapping fails a fixture.
- Validation: Not run. The schema can store the mapping. Nothing reads files.
- Documentation: `docs/materials/pbr-import.md`.

## JRV-0033 — PBR texture-set wizard

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0032
- Goal: Common names and packed maps import with a reviewable recipe.
- Acceptance Criteria: An auto recipe is shown and can be overridden. Packed channels are never guessed at runtime.
- Validation: Not run.
- Documentation: `docs/materials/pbr-import.md`.

## JRV-0034 — Material graph v1

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0031, JRV-0007
- Goal: Typed graph edits, preview, and error diagnostics.
- Acceptance Criteria: A graph edits, previews on the engine, and surfaces compile errors.
- Validation: Not run.
- Documentation: `docs/materials/material-graph.md`.

## JRV-0035 — Material IR and compiler v1

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0034
- Goal: A graph compiles to deterministic WGSL and reflection metadata.
- Acceptance Criteria: The same graph yields the same WGSL and reflection. The asset file is not that WGSL.
- Validation: Not run.
- Documentation: `docs/materials/material-ir.md`.

## JRV-0036 — Master and instance inheritance

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0035
- Goal: Parameter-only instance edits skip graph recompilation.
- Acceptance Criteria: Changing a parameter does not rebuild unrelated graph IR.
- Validation: Not run.
- Documentation: `docs/materials/master-materials.md`, `docs/materials/material-instances.md`.

## JRV-0037 — Material function assets

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0035, JRV-0021
- Goal: Shared graph logic versions and invalidates dependents.
- Acceptance Criteria: Editing a function changes derived keys for dependents only.
- Validation: Not run.
- Documentation: `docs/materials/material-functions.md`.

## JRV-0038 — Material library browser

- Status: Not started
- Milestone: P4M
- Subsystem: editor
- Dependencies: JRV-0036, JRV-0007
- Goal: Search, preview, drag-drop, and provenance.
- Acceptance Criteria: A library entry can be found, previewed with the real renderer, assigned to a slot, and shows license provenance.
- Validation: Not run.
- Documentation: `docs/materials/material-library.md`.

## JRV-0039 — Virtual texture prototype

- Status: Not started
- Milestone: P5
- Subsystem: render
- Dependencies: JRV-0032, JRV-0025
- Goal: A large PBR set stays inside a residency budget.
- Acceptance Criteria: A stress scene records resident pages under a configured budget and a deterministic fallback.
- Validation: Not run.
- Documentation: `docs/materials/virtual-textures.md`.

## JRV-0040 — Material performance diagnostics

- Status: Not started
- Milestone: P4M
- Subsystem: materials
- Dependencies: JRV-0035
- Goal: Variants, textures, passes, and resource counters are visible in the editor and the CLI.
- Acceptance Criteria: `jarvig material stats` and the editor show the same counters from a real compile.
- Validation: Not run. No counters are invented.
- Documentation: `docs/materials/material-library.md`, `docs/cli/overview.md`.
