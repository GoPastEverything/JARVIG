# Lighting

A Blank project lights the level with the environment on World Settings. Intensity starts at 0.20, with two hemisphere colors. That is not a directional-light actor. **Create** does not add a light. The only create command in the menu is Block.

Select **World Settings** in the outliner. The inspector edits the environment fields through the same property command as any other entity. Name and the other editable fields commit as one undo entry when you leave the field.

## A level that already has lights

Lighting Lab is the sample that already contains a directional light, point lights, a spot light, a reflection probe, and the environment:

```powershell
cargo run --offline --manifest-path native/Cargo.toml -p jarvig_editor --bin JARVIGEditor -- --project samples/lighting-lab/LightingLab.jarvigproject
```

Select a light or the probe in the outliner. The viewport click path hits meshes, so a light is selected from the list. The inspector edits that light's fields. The quaternion on a transform stays read-only. The move gizmo can still place the entity.

The picture is one shaded view. Emissive, direct lights, environment diffuse, and environment specular accumulate, then one pass applies exposure and a tone curve. A local reflection probe, when the shaded point is inside it, replaces the specular environment sample with its captured cubemap. That capture is not shadowed. **View > Recapture Reflection Probes** asks for a new capture. The probe is not recaptured every frame.

**View > Exposure +**, **Exposure -**, and **Reset Exposure** change the display exposure. They do not rewrite the light entities.

## Lighting Debug

**View > Lighting Debug** isolates terms that are already in the level. Checking Directional Light, Point Light, or Spot Light shows or hides that term. It does not spawn a new light. Directional Only, Point Only, and Spot Only are the same kind of isolation. Global Environment and Reflection Probe isolate those terms. Full Lighting restores the combined picture.

Material rows in that menu show base color, normal, roughness, ambient occlusion, or metallic as a diagnostic. They do not edit the material asset. Material and texture drops from the content browser still do not assign a slot.

## Shadows

Direct-light visibility multiplies the matching direct term. Shadow maps are limited. A mesh with more than 250,000 triangles can remain in the level and can receive shadows, and it is left out of the shadow-map redraw. The bootstrap directional coverage is a modest box around the view, and punctual lights have a modest far plane. A missing shadow on a huge mesh is that limit, not a second lighting model.

## What you leave alone while building

**View > Pipeline** and **View > Visualization** are diagnostic. Some visualization names belong to cluster debug and to an unstamped procedural-detail experiment. They recolor or replace the shaded picture. **View > Reset Rendering Debug** returns Pipeline to Normal and Visualization to None.

Procedural detail defaults off. Exact blocks are drawn as the solid you authored. This manual does not describe the private surface rule. Leave the diagnostic views off while you judge a graybox.
