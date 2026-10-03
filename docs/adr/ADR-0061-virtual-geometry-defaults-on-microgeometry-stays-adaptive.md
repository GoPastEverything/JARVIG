# ADR-0061 — Virtual geometry defaults on. Microgeometry stays adaptive.

Status: Accepted
Date: 2026-10-02

## Context

Imported static meshes already become canonical meshes and cached meshlets. Frustum culling and conservative occlusion were on in the editor. Draw From Meshlets and Cluster Hierarchy were off until someone checked them. Procedural microgeometry was a single checkbox, default off, and the research flag `proceduralMicrogeometry` still defaults off.

That split is the wrong product. A person who imports a static mesh should not have to know what a meshlet is. The same person should not get Einstein detail on glass, cards, UI, particles, or a surface the current view cannot resolve.

ADR-0008 keeps the research flags default off in the TypeScript prototype. This decision does not flip those flags and does not stamp RFC-0001's performance follow-ups or RFC-0002.

## Decision

For a compatible static mesh the native editor runtime is:

import, canonical mesh, meshlets, cluster hierarchy, derived-data cache, then per-view visibility.

`View > Draw From Meshlets`, `View > Cluster Hierarchy`, frustum culling, and hierarchical-Z occlusion start on. `View > Show Meshlet Colors` stays off. The research flag `virtualGeometry` in the TypeScript prototype stays off.

Occlusion for a view is `hierarchical_occlusion_cull`. The buffer is half the viewport, even, and clamped to 96–384 by 54–216. It is a CPU hierarchical-Z of cluster bounds, not a GPU depth pyramid and not a depth readback. An occluder writes the inside of its bounding circle. A cluster is dropped only when its expanded silhouette is fully in front of its near side. The JRV-0027 64 by 36 reference, with its 12 px cap, remains the regression test. The view path does not use that cap.

A parent mesh loads from a sidecar at or under 64 MB when Cluster Hierarchy is on. A missing sidecar, or a larger one, stays on the leaves until Cluster Hierarchy is turned off and on. That toggle may build. Opening a project does not start the parent build.

Microgeometry mode is Auto, On, or Off. The default is Auto. Auto asks for Einstein detail only when the surface is opaque, not skinned, not UI, not a particle, and not a thin sheet, and the existing 1 px error gate still passes. On uses that same error gate for the selected surface. Off does not. The mode does not match asset names. `PROCEDURAL_MICROGEOMETRY_DEFAULT` stays false. RFC-0002 stays unstamped.

Visibility is per `RenderView`. It does not change the entity, the collider, or gameplay. False-visible is allowed. False-hidden is not. Page streaming is not part of this decision.

## Consequences

The editor status line keeps candidate, frustum, occluded, conservative, selected parents, selected leaves, and submitted triangles, and it prints the hierarchical-Z size. Show Meshlet Colors still paints visible, conservative, frustum-rejected, and occluded clusters.

Draw From Meshlets still replaces one selected mesh. Other actors keep the indexed draw. Their clusters are still tested, and an actor whose enclosing sphere is fully outside the frustum or proven occluded contributes no cluster submission. A partial actor is not dropped as a whole.

No JRV ticket is accepted by this decision. No GPU frame of the hierarchical-Z path was presented with this change. The TypeScript research flags stay default off.
