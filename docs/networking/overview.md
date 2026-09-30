# Networking and replication

Not implemented. `engine/net/` is reserved. ADR: [ADR-0013](../adr/ADR-0013-multiplayer-ready-world-identity.md).

Multiplayer is not a late feature. Authority, prediction, ids, and relevance have to be shaped while the slice is small. The protocol itself waits until the world ids exist, which they now do (entity UUIDs, frame ids, cell ids).

```text
Client input
  -> local prediction for owned predictable systems
  -> server validates and simulates authority
  -> relevance filter
  -> snapshot or delta replication
  -> interpolation / reconciliation
```

Replication fields declare a mode: `authority-only`, `owner-predicted`, `snapshot`, `reliable-event`, or `never-replicate`. Component schema already has a `replicated` boolean. The finer modes are not on the schema yet. Add them as schema metadata, not as a parallel net-only component list.

The server does not trust client position, damage, inventory, or entitlement claims.

Server meshing is research only: [../rfc/RFC-0005-server-meshing.md](../rfc/RFC-0005-server-meshing.md). Flag `serverMeshing` defaults off. Future cell ownership must use world partition.

JRV-0029's host exists: the dedicated server boots the world module with no render import. It does not speak a network protocol.
