# ADR-0030 — Graphics adapter policy belongs to JARVIG

Status: Accepted
Date: 2026-09-22

## Context

Asking wgpu for a high-performance adapter still returned the integrated GPU, and the log did not say why. A desktop engine cannot treat that as a product decision. The same host also has to request a low-power GPU, or one specific device, without the renderer or a shader knowing which chip it is.

## Decision

JARVIG enumerates adapters, records them as `GraphicsAdapterDesc`, and selects with `GraphicsDeviceConfig`. wgpu creates the device. It does not define the policy. No wgpu enum crosses `jarvig_rhi_wgpu`.

```text
host config
    -> enumerate adapters that can present
    -> drop any adapter below the required limits
    -> score by device class
    -> one device and queue
```

Modes are Auto, HighPerformance, LowPower, and Specific.

Auto on a presenting native editor or player host is the high-performance policy. It is not "whatever the backend returns."

High performance prefers a compatible discrete GPU. Low power prefers a compatible integrated GPU. If that class is missing, the other hardware class is used and the reason says so. Software is used only when no hardware candidate remains and `allow_software_fallback` is set. That choice is logged as a software fallback. Specific mode never substitutes a different adapter. A missing or incompatible request is an error.

Device class is the score. Vendor names are not. Ties break by limits, then API order Dx12, Vulkan, Metal, OpenGL, then vendor id, device id, and name. That order is stability, not a claim that one vendor is faster.

A runtime `AdapterId` dies with the process. A saved preference is the fingerprint: vendor id, device id, API, optional class, and optional name. That is not a hardware UUID.

Required limits today are a presentable surface, a 2048 texture edge, one storage buffer per stage, and the backend's default device limits. Mesh shaders and ray tracing are not required.

The selected config and fingerprint stay on the attach result so a later device-loss path can retry the same choice. This decision does not recover from device loss.

The dedicated server does not enumerate adapters. A browser host, when it exists, uses the same preference names, but it must not require a native fingerprint. WebGPU may hide the chip.

## Alternatives Considered

- Keep `PowerPreference::HighPerformance` as the policy. Rejected. It already selected the integrated GPU and did not explain itself.
- Hardcode a discrete product name for the development machine. Rejected. The rule has to work when that chip is absent, and it must not encode a vendor.
- Let the renderer pick the GPU. Rejected. The host supplies config. The RHI applies policy. The renderer sees one device.

## Consequences

One process uses one graphics device. Multi-GPU rendering, cross-adapter resources, and hot-plug migration are not this decision. The editor can later show the same mode list, the surveyed adapters, and the reason. It does not get a second selector.

If Windows or the backend does not enumerate a discrete GPU, high performance falls back and says there was no compatible discrete adapter. JARVIG does not invent a device the platform did not list.

## Supersedes

Nothing. ADR-0019 still hides backend types.
