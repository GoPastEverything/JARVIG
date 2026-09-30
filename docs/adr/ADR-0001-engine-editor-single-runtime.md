# ADR-0001 — One engine, multiple hosts

Status: Accepted
Date: 2026-09-22

## Context

The founding specification requires the editor to contain the real engine. A separate editor scene format would diverge from the shipped game and from the dedicated server.

## Decision

JARVIG has one engine. The Hub, Editor, game client, dedicated server, and CLI are hosts. The editor viewport and Play-In-Editor run that engine. The server uses the same world and simulation architecture and omits editor and renderer systems. Game code does not call editor APIs.

## Alternatives Considered

- A standalone editor that exports a baked level the game loads. Rejected. It splits the world contract.
- Two engines that share a file format. Rejected. Behavior would drift even if files matched.

## Consequences

Hosts compose engine modules. Engine packages cannot import host packages. PIE copies or snapshots the authoring world instead of toggling a mode bit on it.

## Supersedes

Nothing. This is the founding topology in `designdoc.html` sections 1 and 4.

## Superseded By

Nothing.
