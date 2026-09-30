# CryEngine boundary

CryEngine is a reference, not a donor code base.

The CRYENGINE Community Edition repository states that the files contributed there are MIT, and that the engine itself remains under Crytek's license. Do not merge CryEngine source into JARVIG because a community patch repo is MIT. Do not copy implementation from a CryEngine checkout, a leaked tree, or a decompile.

Allowed:

- Read public documentation and papers.
- Write clean-room notes about observed behavior, without transcripts of source.
- Discuss feature and workflow ideas (sandboxes, streaming, editor panels) in JARVIG's own design.

Forbidden:

- Copying or translating CryEngine source, headers, shaders, or serialized formats that are not independently specified.
- A "temporary" paste that will be rewritten later.
- An adapter that links proprietary CryEngine binaries into the core engine without a reviewed Zone B ADR and a license the human lead has accepted.

Zone B adapters, if they ever exist, stay behind the engine's own interfaces and are optional. The dedicated server and the editor must build with that adapter deleted.

See [CLEAN_ROOM_GUIDELINES.md](CLEAN_ROOM_GUIDELINES.md).
