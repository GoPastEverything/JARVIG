# ADR-0065 — An exact parametric solid is classified and draws no relief

Status: Accepted
Date: 2026-10-02

## Context

ADR-0064 puts a Block through the existing meshlet builder, the cluster hierarchy, and Einstein classification. It also said a close view may request patches, and that there is no planarity exception.

That prediction showed up on the 2 m cube. Meshlet Colors and the Einstein classification views were the right foundation: the cube stayed visible and its twelve clusters were classified. Normal shading then showed small squares and triangles sticking out of edges and corners that are mathematically flat.

The stage audit is:

```text
analytic box
  -> 12 derived triangles, 8 corners, 6 planar faces
  -> 12 meshlets, exact coverage, indices inside the box
  -> hierarchy parent spheres contain their children
  -> parent LOD does not substitute (12 triangles, floor 32)
  -> Einstein surface patch anchored on the face corner
  -> shaded draw
```

The derived triangles, the meshlet partition, and the hierarchy stay inside the analytic bounds. Both triangles of a face start at that face's first vertex, which is a corner of the box. Each meshlet can anchor a detail patch there. That patch is the first stage that can leave the box. The shaded detail draw is what shows it. The debug overlay paints the base mesh and does not. The private surface rule is not in this repository.

A gate on every planar triangle would also remove relief from imported flat walls. Those walls are why the surface path exists. The cube is different: it is an authored solid whose geometric error is zero.

## Decision

An exact parametric solid stays in the classifier. Auto still reports it as an opaque solid, so the clusters are not absent and they are not unclassified.

A visible, compatible, anchored cluster with `exact` set is `DetailReject::Exact`. That check is after visibility and before the 1 px gate, so a close view does not select it. Off-screen and occluded clusters keep those reasons. No patch is built in Auto or in forced On. Micro Off and Auto therefore draw the same 12-triangle box.

`cluster_requires_detail` is false for an exact cluster, so the coverage account does not turn Exact into an error. The reason count records `exact` on its own. The visibility balance counts those clusters with the below-threshold bucket so the account still closes, and the status prints `exact` separately. Reject Reason and Eligible color Exact on its own. Error view shows the ordinary surface color. State stays Not Eligible, because no patch was required.

The surface builder is unchanged. Imported meshes, including flat walls, still use the 2 cm feature and the 1 px gate, and a selected close view may still request their patches. The feature size, the error gate, and the meshlet and parent builders stay as they are. Nothing is shrunk or depth-biased to hide the corner.

Changing Size X, Y, or Z rebuilds the same kind of surface. The corner, face, winding, coverage, and parent-containment invariants hold for the new extents. The level still stores only `ParametricBlock` and the size.

## Alternatives Considered

- Clip each patch to its source triangle. Rejected for this slice. A flat cube would not be pixel-clean, and the imported-mesh builder would change with it.
- Shrink the cube or bias depth. Rejected. That hides the extra triangles.
- Turn Auto off for a block. Rejected. The previous failure was a cube that disappeared from the diagnostics. Exact is a decision the classifier records.
- Treat every planar triangle as exact. Rejected. Ghost City and the townshop use flat walls as real surfaces. This gate is the authored solid, not a curvature guess.
- Retune the meshlet grid so a face has one anchor in its interior. Rejected. ADR-0064 keeps the measured 12 clusters. The anchor would still request relief on an exact face.

## Consequences

Normal on a selected 2 m block matches the derived box at every camera angle, including corners against the sky and a grazing top face. Meshlet Colors, Draw From Meshlets, Cluster Hierarchy, and the Einstein views still see the twelve clusters. Eligible and Reject Reason color them as Exact. The status shows `exact 12` and `micro tris +0`. Plane, Wedge, Ramp, Cylinder, and Sphere stay unstarted until that Normal cube has been seen. No JRV ticket is accepted by this decision. RFC-0002 stays unstamped.

## Supersedes

The sentences in ADR-0064 that say a close view may request patches on the block and that there is no planarity exception. Those sentences applied to an exact parametric solid. ADR-0064 still owns derived clusters, the unchanged meshlet builder, and classification of every cluster. An imported mesh still uses the existing gate. Nothing in ADR-0062 or ADR-0063 is superseded.
