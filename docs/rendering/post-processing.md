# Post processing

The only post process today is the JRV-0071 output pass: exposure, then one tone curve, then the sRGB swapchain. It is a fullscreen triangle owned by the renderer. It is not a scene mesh.

Bloom is not implemented. A bright highlight rolls off in the curve. It does not bleed into neighboring pixels. A later ticket can add bloom between the HDR target and the tone curve. Do not add it to make the current lights look softer.

Editor gizmos are not a post process. They are drawn after the tone curve, in display color, so exposure does not recolor the axes. ADR-0035.
