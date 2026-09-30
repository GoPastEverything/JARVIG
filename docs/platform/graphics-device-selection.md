# Graphics device selection

ADR: [ADR-0030](../adr/ADR-0030-graphics-adapter-policy.md).

The host passes a `GraphicsDeviceConfig`. `jarvig_rhi` scores adapters. `jarvig_rhi_wgpu` enumerates them and creates one device. The renderer does not see the list. A material does not see it. The world does not see it.

```text
--gpu / future editor setting
    -> GraphicsDeviceConfig
    -> enumerate, including surface compatibility
    -> required limits
    -> JARVIG score
    -> one device and queue
```

## Modes

| Mode | Choice |
| --- | --- |
| Auto | On a presenting native editor or player host, the high-performance policy. Not "the backend's first adapter." |
| HighPerformance | A compatible discrete GPU. If none can present and meet the limits, the best remaining hardware adapter, with that fallback in the reason. |
| LowPower | A compatible integrated GPU, otherwise the best remaining hardware adapter, again with the reason. |
| Specific | The fingerprint `vendor:device:api`. Missing or incompatible is an error. Another GPU is not substituted. |

Class order for high performance is discrete, integrated, unknown, virtual, then CPU. Low power swaps discrete and integrated. Vendor names are not scores. Equal devices break ties by texture limit, storage-buffer limit, then API order Dx12, Vulkan, Metal, OpenGL, then the numeric ids and the name.

Software is last. It is used only when no hardware candidate remains and software fallback is allowed. The reason starts with `SOFTWARE FALLBACK`. `--no-software` turns that off.

## Identity

`AdapterId` is this process's enumeration index. It is not saved.

A preference fingerprint is vendor id, device id, API, and optionally class and name. Driver updates can still change it. It is not a hardware UUID. A browser host must not be required to produce one. WebGPU often hides the chip. The mode names stay the same.

## Requirements

A presenting device must support the window surface, a 2048 texture edge, one storage buffer per shader stage, and the backend's default device limits. Mesh shaders, ray tracing, and bindless resources are not required.

An adapter that cannot present is listed and rejected. It is not selected and then discovered at the first frame.

## What this laptop can see

The current development machine is a Lenovo laptop. It has Intel UHD and the Microsoft Basic Render Driver. It does not contain a discrete GPU. The RX 6800 XT is a different desktop and is not expected here.

High performance therefore falls back to the best compatible integrated adapter and says so. Low power selects that same integrated adapter as its preferred class. Both results are correct. Do not investigate a missing AMD device on this machine, and do not add a vendor export to invent one.

The policy still cannot select a discrete GPU that Windows and the backend do not enumerate. On any machine where only an integrated adapter and a software renderer are listed, high performance says so and uses the integrated adapter. That is a fallback, not a silent default. The synthetic tests still prefer a discrete adapter when the list contains one.

## Headless

The dedicated server does not create a wgpu instance, does not enumerate adapters, and does not read this config. `graphics=none`.

## Later

Device-loss recovery can retry the saved config and fingerprint. It is not implemented. The editor can show mode, the surveyed list, the selected name, the API, the limits, and the reason. That panel is not built here. Multi-GPU rendering is not this policy.
