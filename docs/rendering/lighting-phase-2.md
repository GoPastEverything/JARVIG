# Lighting phase 2

JRV-0071 through JRV-0074, JRV-0077, and JRV-0078 are accepted. JRV-0075 is implemented and waiting on a human look at the checker under Indirect Diffuse Only. Do not start JRV-0076 from this page.

JARVIG's viewport is direct-light PBR plus one diffuse environment. A directional light, a point light, and a spot light feed the JRV-0053 BRDF. The bootstrap triangles are single-sided: one geometric normal, object +Z, tangent +X. When that normal points at a light, the face lights, including a direct specular highlight. When it turns away, `N·L` or `N·V` goes to zero and the direct term is black. A rough dielectric still receives the environment hemisphere. A pure metal does not. Emissive is added to the surface that owns it. It does not light its neighbors.

That is the picture in the editor: the checker triangle keeps a dim hemisphere when the direct term goes to zero, and the gold metal does not, because this ticket has no indirect specular lobe. The transform is current. The missing piece after this ticket is the reflection, not another ambient constant.

```text
direct light          present
specular highlight    present
emissive on itself    present
environment diffuse   present, JRV-0072 accepted
environment specular  present, JRV-0073, analytical sky, accepted
indirect bounce       present, JRV-0075, probe irradiance, not yet human-accepted
specular IBL          global sky is analytical; a local probe samples a captured cube
reflection probes     present, JRV-0074, one static sphere, not accepted
emissive into scene   absent
tone map / exposure   present, JRV-0071 accepted
shadows               present, JRV-0077, accepted
```

Do not hide the black face with a constant ambient term inside the direct-light shader. Do not flip the normal or drop the `N·V` test to fake a back side. Two-sided materials can be a later material choice. They are not bounced light.

## Tickets

| Ticket | What | State |
| --- | --- | --- |
| JRV-0071 | HDR accumulation, tone mapping, explicit exposure | Accepted. Human visual confirmed. |
| JRV-0077 | Direct-light shadows | Accepted. Human visual confirmed. |
| JRV-0072 | One environment light: normal-dependent diffuse irradiance | Accepted. Human visual confirmed. |
| JRV-0073 | Roughness-aware environment reflections of the analytical sky | Accepted. Human visual confirmed. |
| JRV-0074 | One local reflection probe, captured from the scene | Implemented. Not accepted. |
| JRV-0075 | An indirect diffuse model, not full real-time GI | Implemented. Probe irradiance. Not human-accepted. |
| JRV-0076 | Emissive policy: visible on the surface, not yet a light | Not started |

Order is 0071, then 0072, then 0073. Probes come after the reflection shading exists. Indirect diffuse is the first real bounce. Emissive-as-light stays a written rule before anyone treats a bright material as illumination.

Shadows are still required and are not one of these six tickets. Do not fold them into tone mapping.

## Rules

Direct lights stay. Environment, indirect diffuse, and specular IBL are separate inputs. A material consumes them. It does not own the scene lighting model.

Lighting math stays linear. sRGB is a texture format, not a shader guess. Tone mapping and the display encode happen at the output, not inside the BRDF.

Changing exposure is view state and must not revise `SceneWorld`. Changing the environment is a world lighting edit. Neither recompiles a material. Moving the editor camera must not revise `SceneWorld` and must not rewrite the environment packet.

JRV-0076's first rule: emissive changes the shaded surface only. Bloom may read it later. Emissive does not light other objects until a later ticket says so.

## What the first ticket has to show

JRV-0073 and JRV-0074 are accepted. Outside the probe radius the sky returns. The image is a capture from the probe origin, not a mirror. The rough mips are a GGX prefilter of that 32² capture, not a box filter and not diffuse bounce.

## What this phase is not

Not the asset database. Not the content browser. Not path tracing. Not screen-space reflections unless a later ticket says so. Not a license to blur direct and indirect light.