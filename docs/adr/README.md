# Architecture decision records

An ADR is accepted architecture. It outranks `designdoc.html` only for the decision it names, and only when it says what it supersedes.

Status values: Proposed, Accepted, Superseded, Rejected.

Do not edit an Accepted ADR in place to change the decision. Write a new ADR and set Superseded By on the old one.

| ID | Title | Status |
| --- | --- | --- |
| [0001](ADR-0001-engine-editor-single-runtime.md) | One engine, multiple hosts | Accepted |
| [0002](ADR-0002-playcanvas-foundation.md) | PlayCanvas upstream and license policy | Partially superseded by 0017 |
| [0006](ADR-0006-webgpu-first-renderer.md) | WebGPU as the first graphics backend | Partially superseded by 0017 |
| [0003](ADR-0003-no-bsp-world-model.md) | No BSP world model | Accepted |
| [0004](ADR-0004-hierarchical-coordinate-frames.md) | Hierarchical coordinate frames | Accepted |
| [0005](ADR-0005-cryengine-license-boundary.md) | CryEngine license boundary | Accepted |
| [0007](ADR-0007-material-graph-ir.md) | Materials compile through an IR | Accepted |
| [0008](ADR-0008-research-feature-flags.md) | Research stays behind flags | Accepted |
| [0009](ADR-0009-ecs-identity-and-data.md) | ECS identity separated from storage | Accepted |
| [0010](ADR-0010-documentation-authority.md) | Documentation authority order | Accepted |
| [0011](ADR-0011-cli-parity.md) | CLI parity for critical editor operations | Accepted |
| [0012](ADR-0012-derived-data-cache.md) | Content-addressed derived data | Accepted |
| [0013](ADR-0013-multiplayer-ready-world-identity.md) | World identity is multiplayer-ready | Accepted |
| [0014](ADR-0014-codename-jarvig.md) | Codename JARVIG | Accepted |
| [0015](ADR-0015-dependency-direction.md) | Dependency direction | Accepted |
| [0016](ADR-0016-repository-layout.md) | Repository layout | Accepted |
| [0017](ADR-0017-native-engine-core-and-multi-host-runtime.md) | Native engine core and multi-host runtime | Accepted |
| [0018](ADR-0018-engine-owned-capabilities.md) | Engine-owned capabilities | Accepted |
| [0019](ADR-0019-rhi-hides-backend-types.md) | RHI hides backend types | Accepted |
| [0020](ADR-0020-render-space-and-clip-convention.md) | Render space and clip convention | Accepted, depth mapping partially superseded by 0021 |
| [0021](ADR-0021-reversed-z-infinite-far.md) | Reversed-Z and an infinite far plane | Accepted |
| [0022](ADR-0022-api-layers-and-one-type-registry.md) | API layers and one type registry | Accepted |
| [0023](ADR-0023-abi-struct-negotiation-and-calling-convention.md) | ABI struct negotiation and calling convention | Accepted |
| [0024](ADR-0024-gpu-resource-lifetime.md) | GPU resource lifetime | Accepted |
| [0025](ADR-0025-render-view-is-not-a-world.md) | A render view is not a world | Accepted |
| [0026](ADR-0026-render-snapshot-is-not-a-world.md) | A render snapshot is not a world | Accepted |
| [0027](ADR-0027-texture-color-space.md) | Texture color space is metadata, not a shader guess | Accepted |
| [0028](ADR-0028-standard-metal-rough-surface.md) | Standard metallic/roughness surface | Accepted |
| [0029](ADR-0029-direct-light-conventions.md) | Direct light direction, units, and view packets | Accepted |
| [0030](ADR-0030-graphics-adapter-policy.md) | Graphics adapter policy belongs to JARVIG | Accepted |
| [0031](ADR-0031-native-editor-shell.md) | The native editor shell is not the engine | Accepted |
| [0032](ADR-0032-editor-dock-workspace.md) | The dock tree is editor state, not a window tree | Accepted |
| [0033](ADR-0033-editor-camera-is-not-a-scene-entity.md) | The editor camera is not a scene entity | Accepted |
| [0034](ADR-0034-gizmo-is-editor-overlay.md) | The transform gizmo is editor overlay state | Accepted |
| [0035](ADR-0035-hdr-scene-color-and-output-pass.md) | HDR scene color is not the swapchain | Accepted |
| [0036](ADR-0036-environment-diffuse-is-scene-lighting.md) | Environment diffuse is scene lighting | Accepted |
| [0037](ADR-0037-opaque-culls-back-faces.md) | Opaque materials cull back faces | Accepted |
| [0038](ADR-0038-analytical-environment-specular.md) | Environment specular is split-sum until a cubemap exists | Accepted |
| [0039](ADR-0039-local-reflection-probe-is-scene-capture.md) | A local reflection probe is captured scene radiance | Accepted |
| [0040](ADR-0040-authoring-entity-is-not-a-subsystem-id.md) | An authorable world object has one EntityUuid | Accepted, duplicate/delete sentence superseded by 0041 |
| [0041](ADR-0041-authoring-lifecycle-owns-subsystem-records.md) | Duplicating or deleting an entity owns its subsystem record | Accepted |
| [0042](ADR-0042-reflection-probe-mips-are-ggx.md) | Reflection probe mips are a GGX prefilter | Accepted |
| [0043](ADR-0043-direct-light-shadows-are-visibility.md) | Direct-light shadows are a visibility term | Accepted |
| [0044](ADR-0044-indirect-diffuse-is-probe-irradiance.md) | Indirect diffuse is probe irradiance | Accepted |
| [0045](ADR-0045-project-and-level-documents.md) | A project and a level are documents, not GPU state | Accepted |
| [0046](ADR-0046-one-renderer-scalable-quality.md) | One renderer, scalable quality, optional ray tracing | Accepted |
| [0047](ADR-0047-cascaded-direct-shadows.md) | Direct shadows are cascaded visibility | Accepted |
| [0048](ADR-0048-game-framework-above-the-world.md) | A game framework sits above the world | Accepted |
| [0049](ADR-0049-component-membership-is-not-a-second-registry.md) | Component membership is not a second registry | Accepted |
| [0050](ADR-0050-authored-world-is-source-runtime-world-is-instance.md) | The authored world is source data; the runtime world is an instance | Accepted |
| [0051](ADR-0051-imported-mesh-is-an-asset.md) | An imported mesh is an asset, not a level | Accepted |
| [0052](ADR-0052-in-process-job-queue.md) | An in-process job queue is not a second engine | Accepted |

RFCs are not in this list. See [../rfc/README.md](../rfc/README.md).
