# HDR scene color

JRV-0071. Lighting stays linear. The swapchain is the display, not the light accumulator.

```text
emissive + direct light + environment diffuse
        |
        v
RGBA16Float scene color
        |
        v
exposure, per view, 2^stops
        |
        v
fitted tone curve, once
        |
        v
linear display RGB
        |
        v
sRGB swapchain encode
        |
        v
editor gizmo
```

The format is `Rgba16Float`. It is a JARVIG texture format. wgpu names stay in `jarvig_rhi_wgpu`. The target matches the configured drawable: client, surface, HDR, and depth are the same pixels. A 0×0 view drops it and does not allocate an empty target. Resize retires the old target before creating the new one. Shutdown destroys the output bind groups, pipeline, and the HDR texture.

## Exposure

`exposure_ev` lives on the render view. Zero is the baseline. Plus one stop doubles the linear scene color before the curve. Minus one halves it. The editor range is -16 to +16. View > Exposure +, Exposure -, and Reset Exposure change it. The status line shows `Exposure: +0.0 EV`. That does not revise `SceneWorld`, the selection, materials, meshes, or textures, and it does not rebuild the tone-map pipeline.

The Perspective camera is still editor state. Exposure is not a scene entity.

## Tone curve

The output pass uses Krzysztof Narkowicz's fitted curve:

```text
y = x * (2.51x + 0.03) / (x * (2.43x + 0.59) + 0.14)
```

This is not Academy ACES. Negatives become 0 before the curve. The result is clamped to 0..1 only as the display range of an already compressed value. Alpha is 1 and is not passed through the curve. The curve is not in the material shader.

The shader returns linear display RGB. `Bgra8UnormSrgb` performs the transfer. There is no extra `pow(1/2.2)`.

A smooth gradient on that 8-bit display quantizes into isolines. On the Lighting Lab floor those isolines are the spot and point falloff. On the far triangle they ring the specular hotspot. The floor is roughness 0.92. The saved directional light emits along local -Z and does not light the floor.

The first dither was interleaved-gradient noise. Without temporal AA that noise is a diagonal weave, and the review still saw broad diagonal bands. The output pass now uses triangular noise from two hashes, one display code wide, in the sRGB encoding, then converts back to linear. View > Presentation Dither turns it off. View > Presentation: 8-bit Steps forces the quantize. View > Presentation: Before Tone Curve skips the curve. Lighting Debug > No Shadows and Contact Shadows are the other isolations. None of them are saved. Temporal AA is not started.

## What this does not add

Environment diffuse and environment specular both enter this target before exposure. See [environment-lighting.md](environment-lighting.md). There is still no probe, no object bounce, no bloom, and no fake ambient. The specular term is an analytical reflection of the sky, not a reflection of the other triangle. Direct light units are unchanged. Emissive above 1 is stored in the HDR target and tone-mapped. It still does not light another object.

The bootstrap point and spot are bright. At 0 EV their cores can still reach display white. That is what exposure is for. Do not retune the lights to hide it.

See [post-processing.md](post-processing.md) and [ADR-0035](../adr/ADR-0035-hdr-scene-color-and-output-pass.md).
