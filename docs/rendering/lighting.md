# Lighting

The shading baseline is physically based metallic/roughness: linear-light math, explicit color-space metadata, image-based lighting, physically meaningful normals, and a recorded tangent-space convention. Advanced models extend that baseline. They do not create unrelated pipelines.

Planned production path after the GBuffer or forward pass: direct lighting, shadows, then GI. Advanced GI is flag `advancedGi`, default off, and is [RFC-0006](../rfc/RFC-0006-advanced-global-illumination.md).

Each shading model needs a documented GBuffer or forward requirement, a lighting contract, a fallback, and a platform matrix. Unsupported models degrade on purpose. They must not silently compile a broken approximation.

## Direct lights (JRV-0054)

ADR-0029. The world owns the light. The material graph does not. [ADR-0028](../adr/ADR-0028-standard-metal-rough-surface.md) still owns the surface and the BRDF.

```text
SceneWorld lights
    -> one extract into RenderSceneSnapshot
    -> each RenderView builds a camera-relative packet
    -> group 2 storage buffer
    -> StandardMetalRough loop
    -> scene-linear radiance
    -> sRGB swapchain encode
```

| Kind | Frame | Intensity | Falloff |
| --- | --- | --- | --- |
| Directional | Orientation. Emits along -Z. | Lux | None. `L = -forward`. |
| Point | Position. | Candela | `cd / max(d, 0.01 m)²` |
| Spot | Position and -Z aim. | Candela | Inverse square, then a smooth cone |

Color is linear RGB. A positive range culls samples outside it and does not change candela inside it. `0` means no cutoff. Spot cones are half-angles with `inner <= outer <= π/2`. A bad cone is an error.

Disabled lights stay in the world and are left out of the snapshot. Both views read that same list. Their GPU positions differ because each view subtracts its own f64 origin and rotates into eye space. The bootstrap root is still one billion meters. The uploaded positions are meter-scale.

Group 0 is the object transform. Group 1 is the material. Group 2 binding 0 is the light count. Binding 1 is the read-only light storage buffer. One packet per view. The fragment loops `min(count, arrayLength)`. The renderer sizes the buffer for 64 lights. That cap is not a material permutation. Clustered or tiled lists can replace the loop later without a new surface or a new BRDF.

Unlit does not declare group 2 and does not read the buffer. Standard output is emissive plus, for each light, the JRV-0053 BRDF times that light's illuminance. Ambient occlusion does not scale direct light. There is no added ambient, no shadow, and no environment reflection. Values above 1 stay in the HDR scene target. Exposure and the tone curve are the output pass, not this BRDF. See [hdr.md](hdr.md).

Changing color, intensity, or the frame updates the packet. It does not recompile, rebuild a material pipeline, or reupload a mesh or texture. Destroying a view retires that view's buffers. The server may hold the logical lights. It does not extract them and it does not upload them.

The bootstrap scene is one dim white directional (0.35 lux, identity, so it shines toward -Z), one blue point (14 cd), and one warm spot (22 cd) aimed at the far triangle. Those numbers are for an untonemapped LDR target, not a calibrated exposure.

## A rotated face can go black

This is direct light only. There is no sky, image-based lighting, irradiance, reflection probe, or ambient term. Do not add a constant ambient color to hide a dark face.

The fragment kills the direct BRDF when `N·L <= 0` or `N·V <= 0`. The bootstrap triangles are single-sided: the geometric normal is object +Z and the tangent is +X. The directional light hits that +Z face. Turning the face so the normal points down puts every bootstrap direct light on the horizon or behind the normal, so the direct term becomes zero. Environment diffuse is added after that and still follows the world normal. It is not a second copy of the direct BRDF.

That result is recomputed every presented frame from the current extract:

- object translation and orientation become the camera-relative model matrix
- the normal is the inverse-transpose of that matrix's current 3×3, then the view rotation
- the tangent is the same 3×3, not the inverse-transpose, with the authored handedness
- light vectors are the current light frames, in the same eye space
- the camera pose is the view vector

A gizmo drag writes the authoritative transform before the next extract. Escape writes the original quaternion back, and the next frame's normal and lighting match the start. Translation does not change a directional `N·L`. Moving toward the point light follows `cd / distance²` and returns when the position returns. None of this reuploads the mesh, the material, or a texture. The instance uniform is rewritten only when its bytes change. Rotating an object does not rewrite the light packet. Moving the camera still does, as JRV-0064 required.

Environment diffuse is the accepted JRV-0072 hemisphere. Environment specular is the accepted JRV-0073 analytical sky, replaced inside one local probe by a captured cubemap. JRV-0071 is the HDR path both enter. JRV-0074 is that probe and is not accepted. JRV-0075 is indirect diffuse beyond this sky. JRV-0076 says emissive does not light other objects yet, except where a static probe happened to capture an emissive surface. Shadows are still missing. See [environment-lighting.md](environment-lighting.md) and [lighting-phase-2.md](lighting-phase-2.md). Do not start the next ticket from a gizmo bug.

Material editor previews, when they exist, use this same lighting path: selectable HDR environments, direct lights, and exposure. Not a special preview shader language. The light inspector is not built. The fields a later inspector edits are type, linear color, intensity with its unit, range, inner and outer cone, enabled, and the frame.
