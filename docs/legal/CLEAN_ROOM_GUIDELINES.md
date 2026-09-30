# Clean-room guidelines

Use this when a commercial engine is the behavioral reference.

1. One group may study public behavior and write a specification: inputs, outputs, failure modes, and tests. That specification does not include source, pseudo-code transcribed from source, or distinctive asset-format dumps.
2. A second implementation pass uses the specification and JARVIG's own architecture. It does not use the reference source as a second monitor.
3. If you have read restricted source, do not implement that subsystem. Say so in the session handoff and hand the work to someone who has not.
4. Tests describe JARVIG's contract. They do not embed expected bytes taken from another engine's files unless those bytes are from a format JARVIG has an independent right to implement (for example a published glTF sample).
5. Names of techniques (meshlets, virtual texturing, world partition) are not proprietary. Implementations and unpublished file layouts are.

PlayCanvas is MIT. A reviewed vendor or fork may include its source with the license notice. That is not a clean-room requirement. It is still a deliberate import, not a drive-by copy. See [ADR-0002](../adr/ADR-0002-playcanvas-foundation.md).
