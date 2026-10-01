# Research disclosure boundary

This document records a repository policy for separating JARVIG's public source from unpublished research. It is a project-governance note, not a patent filing and not legal advice.

## Public visibility is not an open-source grant

JARVIG is currently all rights reserved. The repository is publicly visible, but visibility alone does not grant permission to copy, modify, redistribute, or create derivative works from JARVIG source. See [../../LICENSES.md](../../LICENSES.md).

Third-party material remains governed by its own license and must be tracked in [THIRD_PARTY.md](THIRD_PARTY.md).

## What may be public

Public documentation may describe:

- subsystem purpose and interfaces;
- feature flags and fallback behavior;
- acceptance criteria;
- reproducible public benchmarks;
- high-level research goals;
- required attribution to prior work;
- the fact that an unpublished provider or algorithm exists.

Public documentation should avoid publishing unpublished implementation details solely to prove that private research exists.

## What may stay private

Until the human lead chooses otherwise, private/local research may retain implementation details such as:

- distribution or placement rules;
- topology-construction rules;
- subdivision or refinement strategies;
- cache/addressing schemes;
- heuristics and thresholds that are not already public contracts;
- unpublished source code, test fixtures, or derived-data formats specific to the private provider.

A public interface is not evidence that the private provider itself is public.

## Reporting private results

Private/local test results may be mentioned publicly only when they are labeled as private/internal results.

A private result must not be described as reproducible from the public `master` branch unless the matching implementation or a reproducible artifact has been published.

Before a result is presented as a public benchmark, record:

- exact revision or immutable artifact;
- hardware and output resolution;
- feature state and comparison baseline;
- scene/camera path;
- measured CPU/GPU/memory costs where available;
- acceptance evidence and known limitations.

## Prior work and JARVIG claims

JARVIG documentation must distinguish prior mathematics, algorithms, papers, APIs, and third-party assets from JARVIG-specific engineering.

For RFC-0002, the mathematical `einstein` concept and the 2023 hat monotile are prior mathematical work associated with David Smith, Joseph Samuel Myers, Craig S. Kaplan, and Chaim Goodman-Strauss. JARVIG does not claim invention of that mathematical result.

The JARVIG-specific work being documented and evaluated is the engine integration around that prior mathematics: using Einstein-hat-derived aperiodic structure as deterministic input to screen-space-adaptive generated microgeometry, preserving the accepted base hierarchy/material path, performing generation and upload asynchronously, rejecting stale work, publishing complete results at a frame boundary, and reusing deterministic results. The project documentation refers to that engine-side integration as **JARVIG Einstein Surface**.

This description is intentionally architectural. It does not publish the active private/local provider's exact construction, topology/subdivision rules, cache/addressing implementation, placement heuristics, or other unpublished implementation details.

JARVIG may describe and protect its own engine-specific integration, implementation, tooling, heuristics, data structures, pipelines, or other original work, but public wording must not erase the underlying attribution.

## Before publishing sensitive research

Before pushing source or documentation that would reveal an unpublished research implementation, stop and require a human decision on the disclosure boundary.

If patent protection is being considered, obtain qualified legal advice before intentionally publishing implementation details. This repository policy does not determine patentability, ownership, or filing deadlines.
