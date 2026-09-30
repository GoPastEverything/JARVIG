# Virtual textures

```text
Material visibility
  -> required material instances
  -> required texture / UDIM pages
  -> GPU feedback + predictive streaming
  -> virtual resource manager
       resident mip/page set
       memory budgets
       fallback mip/tile
       eviction priority
```

Two mechanisms, both later:

- **Streaming virtual textures.** Disk-cooked pages requested on demand. JRV-0039. Exit: a large PBR set stays inside a configured residency budget.
- **Runtime virtual surfaces.** GPU-generated pages for expensive layered or procedural surfaces, only where measurement shows a benefit.

Material instances reference logical textures. Residency stays inside the resource manager. UDIM and tiled sets are one logical texture with independently streamable tiles. High-resolution sources stay out of runtime packages. Cooks emit platform mips, pages, and compression (BCn, ASTC, ETC, Basis, KTX2 families are the founding examples). The authored material stays platform-agnostic.

Debug requirements when this exists: per-texture residency, mip bias, anisotropy, and page-fault counters.

Phase 0 has none of the page machinery. The texture semantic on the material document is the logical binding this system will serve.
