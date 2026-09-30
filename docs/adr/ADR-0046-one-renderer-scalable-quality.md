# ADR-0046 — One renderer, scalable quality

Status: Accepted
Date: 2026-09-23

## Context

JARVIG is being built and checked on an Intel UHD laptop. That machine is the baseline validation platform. It is not a separate low-end renderer, and it is not the visual ceiling. A later Radeon, NVIDIA, or ray-tracing GPU must open the same `.jarviglevel` and mean the same thing by a material, a light, an exposure, and a probe.

Hardware ray tracing can make some queries cheaper or more accurate. It cannot be required for a shadow, a reflection, or indirect light to be correct.

## Decision

There is one JARVIG rendering model.

Same authored world, same materials, same lighting meaning, same level file. Implementations differ by an explicit quality budget, not by vendor name. Intel is not "Low". AMD is not "High". A measured device may choose a default budget. The editor may override it. The override is not a second scene.

Tiers, when a subsystem needs them, are:

- Baseline hardware raster
- Enhanced compute
- High-end compute
- Optional ray query / hardware ray tracing

Core lighting stays correct without the last tier. Ray tracing may later improve shadow visibility, reflection accuracy, GI probe tracing, contact detail, and translucency. Those paths accelerate the same systems. They do not replace them.

Every advanced subsystem exposes a budget. Examples, not a closed list: `shadow_resolution`, `shadow_cascade_count`, `shadow_filter_samples`, `reflection_probe_resolution`, `reflection_probe_updates_per_frame`, `gi_probe_spacing`, `gi_rays_per_probe`, `gi_updates_per_frame`, `reflection_ray_budget`.

The same rule will apply to a future cluster geometry system. A small GPU selects coarser clusters. A large GPU selects denser clusters of the same world. The numbers people quote for that are illustrative, not promises.

## Alternatives Considered

- A low-end renderer and a high-end renderer. Rejected. Two meanings for one level.
- Vendor branches (`if Intel`, `if NVIDIA`). Rejected. ADR-0030 already refuses to score a vendor name.
- Hardware ray tracing as the lighting foundation. Rejected. The validation machine has none, and a scene must not need to be relit to move GPUs.

## Consequences

`docs/rendering/shadows.md` records the first budgets: four cascades, a 1024² cascade texel, PCF/PCSS sample limits, and a shadow-pass budget. Dynamic GI is not started here. When it is, it is a probe or clipmap field with a ray budget, and a ray-query trace is an optional accelerator.

The default budget is Baseline on every adapter. JRV-0087 does not lower it because the device is integrated. A later measurement may choose Enhanced or High. Until that measurement exists, the editor override is the selection. `gi_updates_per_frame` and `reflection_ray_budget` are named and unused.

## Supersedes

Nothing in ADR-0030. This decision adds the quality-budget rule that adapter policy did not state.

## Superseded By
