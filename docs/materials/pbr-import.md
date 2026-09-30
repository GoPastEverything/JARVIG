# PBR texture-set import

The importer accepts common artist-library conventions without making the runtime depend on a vendor or a DCC. A wizard maps files and channels into canonical semantics and saves the mapping as import metadata.

```text
source files
  *_BaseColor / *_Albedo
  *_Normal
  *_Roughness or *_Gloss
  *_Metallic
  *_AO
  *_Height / *_Displacement
  packed map (ORM / RMA / MRA / custom)
    -> explicit channel-map recipe
    -> canonical surface inputs
    -> target cook
    -> virtual texture pages or compressed GPU textures
```

Rules:

- Auto-detection is allowed only when the detected convention is shown and can be overridden.
- Never silently reinterpret packed channels. ORM is one recipe, not the universal meaning.
- Gloss converts to roughness reproducibly. The recipe stores that conversion.
- Normal Y inversion is metadata.
- Source color space, channel meaning, physical scale, and texel density are preserved.
- UDIM is one logical texture.
- Derived outputs are platform-specific. The material document is not.

JRV-0032 is the semantic importer. JRV-0033 is the wizard. Neither is started. Provisional CLI: `jarvig material import ./Rock_Cliff_4K/ --recipe auto-review`.
