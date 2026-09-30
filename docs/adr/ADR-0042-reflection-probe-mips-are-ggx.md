# ADR-0042 — Reflection probe mips are a GGX prefilter

Status: Accepted
Date: 2026-09-23

## Context

JRV-0074 captured a 32² cubemap and filled the higher mips with a 2×2 box filter. A human confirmed the capture: the large metal shows scene radiance, and it moves with the camera and the surface. The same look also showed the box filter as enlarged rectangles. That is a filter limit, not a failed capture. JRV-0074 is accepted with that limit named. The rectangles are what JRV-0078 replaces.

## Decision

Mip 0 stays the sharp static capture. Mips 1 through `mip_count - 1` are a GGX importance-sample prefilter, Karis 2013. Mip `i` is filtered at roughness `i / (mip_count - 1)`. The material still selects `lod = roughness * (mip_count - 1)`. That is not `color * (1 - roughness)`.

Each texel's normal is the WebGPU / D3D cubemap direction for that face and texel center. Samples are weighted by `N·L` and read the cube with an explicit mip chosen from the sample's solid angle, clamped to mips already written. The prefilter runs once, during capture, into that probe's own cube. It does not recapture when the camera moves. It does not tone-map the cube. It does not change direct lights.

The bootstrap cube is still 32². A GGX mip cannot invent detail the capture never stored. Low roughness stays close to the sharp capture. High roughness becomes a lobe, not a rectangle.

## Alternatives Considered

- Leave the box filter and call the rectangles finished. Rejected. The human acceptance named them as a quality limit, and the lod select was already the GGX mip index.
- Raise the capture to 128² or 256² in the same change. Rejected. Resolution is a separate cost. This ticket only changes the filter.
- Prefilter on the CPU. Rejected. The cube is a renderer resource. The editor does not sample it.

## Consequences

`docs/rendering/environment-lighting.md` describes the filter. Direct point and spot highlights stay analytic and round. A soft or low-detail reflection is still the 32² capture. Indirect diffuse and shadows are not this filter.

## Supersedes

The ADR-0039 sentence that higher mips are a box filter.

## Superseded By
