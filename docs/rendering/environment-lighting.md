# Environment lighting

JRV-0072. One scene environment adds diffuse irradiance. It is not a constant ambient term, not a reflection, and not object-to-object bounce.

```text
emissive + direct lights + environment diffuse + environment specular
        |
        v
RGBA16Float scene color
        |
        v
exposure, tone map, sRGB encode, editor gizmo
```

The direct BRDF is unchanged. `N·L <= 0` or `N·V <= 0` still contributes no direct light. The environment term is added after that loop, so a dielectric can stay lit when every direct light is behind the normal.

## What it is

`SceneWorld` owns one `EnvironmentLight`. At most one. It is world lighting state, like the directional, point, and spot lights, and unlike exposure. Editing it revises the world. It is not a `LightKind`, not one of the 64 direct-light records, not a material parameter, and not a `RenderView` setting.

The bootstrap source is programmatic. There is no environment texture and no asset import.

| Field | Value | Meaning |
| --- | --- | --- |
| Upper hemisphere | `(0.55, 0.68, 0.86)` | Cool sky. Linear radiance. |
| Lower hemisphere | `(0.22, 0.16, 0.11)` | Darker, slightly warm ground. Linear radiance. |
| Intensity | `0.20` | Scales both hemispheres. Direct lights stay dominant. |
| Enabled | on | Off restores the JRV-0071 direct-only result. |

World +Y splits the hemispheres. There is no position and no rotation yet. The same light is shared by every view. Moving the camera does not change the packet.

## Diffuse response

For a Lambertian surface the cosine-weighted integral of a uniform hemisphere is `radiance * π`. Lambert divides by `π`, so those cancel. The blend is exact when the normal points up, down, or at the horizon:

```text
sky = clamp(worldNormal.y, -1, 1) * 0.5 + 0.5
incident = mix(lower, upper, sky)
environment = baseColor * (1 - metallic) * ao * intensity * incident
```

`worldNormal` is the shaded normal rotated back out of view space. A yaw that keeps `normal.y` does not change this sky. A pitch does. Equal upper and lower colors are orientation-invariant, which is the uniform-sky case, not a flat ambient add.

On a two-sided material the normal is the visible side. Looking at the reverse of an upward face sees the lower hemisphere, because that side points down. A one-sided material never shades that reverse face. It is culled. See [face-culling.md](face-culling.md).

## Specular reflection

JRV-0073, accepted. The same `EnvironmentLight` also feeds a reflection. Metals have no diffuse environment term, so this is what keeps a metal from going flat when the direct highlight leaves. Dielectrics keep the diffuse term and add a smaller reflection at F0 `0.04`.

The global lookup is still `jarvig_hemisphere(direction)`. It is not a cubemap. The material surface does not change.

```text
R = reflect(-V, N)
alpha = roughness²
prefiltered = mix(hemisphere(R), hemisphere(N), alpha)
specular = prefiltered * intensity * (F0 * scale + bias)
```

`alpha` is the same GGX width the direct BRDF uses. Roughness 0 stays on the mirror ray. Roughness 1 looks at the hemisphere the normal faces. That is not `color * (1 - roughness)`.

`scale` and `bias` are Brian Karis's analytical environment BRDF. Grazing `N·V` raises the reflectance. `F0` is the same mix as the direct term: `mix(0.04, baseColor, metallic)`.

Ambient occlusion does not scale this term. There is no specular-occlusion model yet. Direct specular is still unaffected by AO.

The reflection uses the visible-side normal, including the two-sided flip, and the world view direction. Rotating the object or moving the camera changes it. Neither recompiles a material. Camera motion does not revise `SceneWorld` and does not rewrite the environment packet. Roughness is the existing material parameter.

What is still absent from this global term: a reflection of the other triangle, screen-space reflections, and emissive light bouncing onto a neighbor. The local probe below is the first path that can carry nearby geometry.

## Local reflection probe

JRV-0074 is accepted. One `SphereReflectionProbe` sits in the scene frame at local `(0, 0.2, -3.5)` with radius 8 m. It is world lighting data. The authoring row is an entity under ADR-0040. The cubemap is not.

The renderer captures the opaque scene from that origin into an `Rgba16Float` cubemap. The self-test uses 32². A saved level keeps the resolution it names. The Lighting Lab file is 64². View can select 32, 64, 128, or 256. A new capture, when the level did not name one, uses 64 on an integrated GPU and 128 on a discrete or virtual GPU. That choice is the device class, not a vendor id. The same material shader runs, with the probe weight forced off so the cube cannot sample itself. Direct lights, the global environment, and emissive surfaces are in the capture. The editor gizmo is drawn later, onto the swapchain, and is not. Empty texels are the hemisphere color of the face direction times environment intensity.

A geometric specular AA term raises roughness from the screen derivatives of the shaded normal, and caps that rise at 0.02, so a smooth 0.045 mirror stays on mip 0. It runs once per fragment and feeds both the direct BRDF and the probe lod. It is not a second roughness control and it does not change authored intensity.

The probe sample also raises its lod when one pixel covers more than one cube texel, and that extra rise stops at two mips. A close mirror still reads mip 0. A distant or edge-on reflection does not sparkle the capture. Blocky texels on the smooth sphere at 64² are that mip magnified. They are not a cubemap seam and not a prefilter bug. View > Probe Resolution 128 and 256 is the comparison. View > Renderer Quality names the same sizes as baseline, enhanced, and high. The choice is not the adapter class. A saved level keeps the resolution it already has.

Mip 0 is that capture and stays sharp. The smooth-metal floor (perceptual roughness 0.045) reads mip 0 exactly, so a close mirror shows the capture texels and not a coarser mip blended in. Rougher values spread across the rest of the chain. Mip `i` is prefiltered at the roughness that maps back to `i`, with a GGX Hammersley integral weighted by `N·L`. Narrow lobes use up to 160 samples at 64². Larger faces use fewer samples per texel so a 256² rebuild stays a one-time cost on the baseline GPU. The runtime blend between those mips is explicit. Sampling is still a `texture_cube` direction, so a face edge is the hardware cube sample. A blocky close-up at 64² is the capture size. 128 and 256 are the comparison. Direct-light specular is a separate term and is not this cube. JRV-0078.

The first cube under Static, On Demand, On Transform, or On Lighting is captured in that frame. Every later dirty rebuild, and every Time Sliced capture, renders one cubemap face per frame into a side cube. The cube being sampled stays until the new one is complete. Camera movement does not mark a probe dirty and does not redraw shadow maps. Object, light, and environment edits mark probes only when the selected policy says so. This is not a recapture of every probe every frame. Specular radiance, irradiance, and direct shadow maps stay three separate systems.

```text
t = saturate(1 - distance / radius)
weight = t * t * (3 - 2 * t)
radiance = mix(global_prefiltered, local_sample * probe_intensity, weight)
specular = radiance * (F0 * scale + bias)
```

Weight is 0 outside the sphere, when the probe is disabled, or when its intensity is not positive. The global term remains. Diffuse environment light is still only the hemisphere. The GPU center is probe origin minus view origin, in f64, then f32.

Under the default Static policy, moving a card changes which probe it samples on the next frame without a material compile, and the cube image stays the one from the first capture until a recapture or a resolution change. On Transform rebuilds after that move, one face per frame. The shaded point is not the capture point, so this is not a mirror. The far card at local z `-5` is inside. Local z `-30` is outside.

Metallic `1` drives this term to zero. That is the missing diffuse lobe, not a reflection. A pure metal can still look dark away from direct light until JRV-0073. The far bootstrap triangle is that metal. Its emissive stays additive and does not light the near triangle.

AO is the surface value the graph already writes (`lerp(1, sampledAO, OcclusionStrength)`). It scales environment diffuse only. `evaluate_direct` still ignores AO. Direct lights are not darkened.

## GPU

Group 2 binding 0 and 1 stay the direct-light header and storage. Binding 2 is one 32-byte uniform, shared by every view:

```text
upper.xyz = upper radiance, upper.w = intensity
lower.xyz = lower radiance, lower.w = 1 when enabled, else 0
```

Bindings 3, 4, and 5 are the probe uniform, the cubemap, and its sampler. The uniform is per view because the center is view-relative. The cubemap is one renderer resource.

`environment_packet_upload_count` increments when the environment bytes change. An edit does not recompile a material, reupload a mesh or a scene texture, or rebuild the tone-map pipeline. Probe capture resources are separate from `texture_upload_count`. Unlit masters do not declare group 2.

## What this is not

- Not `color += vec3(0.1)` and not `baseColor * ambientConstant`.
- Not a promise that 64² holds a sharp close-up of a mirror. 128 and 256 are selectable and keep the same lighting meaning. The self-test cube is still 32².
- Not box projection, SSR, or a recapture of every probe every frame.
- Not one triangle lighting another through diffuse bounce. JRV-0075. A reflection of opaque color is not that bounce.
- Not emissive lighting a neighbor. Emissive still lights only itself.
- Not shadowed by JRV-0077. Direct-light visibility multiplies only the matching direct term. Probe capture binds the shadow maps with every record disabled, so this cube does not darken when a card blocks a light.
- The same capture also feeds JRV-0075. After the GGX mips, a cosine convolution writes a second cube, the irradiance. Full lighting uses that for diffuse inside the probe and fades this analytical hemisphere by the probe weight, because the capture already contains the sky. Environment Diffuse Only still shows the full hemisphere. Indirect Diffuse Only shows only the irradiance. The gold card is metal, so it does not take the diffuse bounce. The checker does. A normal that points away from the other card does not receive that card. The irradiance is built once with the capture. Moving the camera or a triangle does not rebuild it.

View > Environment Light toggles the world light. The engine does not depend on that menu. The status line shows `Env: on` or `Env: off`, and `Probe: on` or `Probe: off`. There is no probe gizmo.

See [lighting-phase-2.md](lighting-phase-2.md), [hdr.md](hdr.md), [../materials/pbr.md](../materials/pbr.md), and [ADR-0036](../adr/ADR-0036-environment-diffuse-is-scene-lighting.md).
