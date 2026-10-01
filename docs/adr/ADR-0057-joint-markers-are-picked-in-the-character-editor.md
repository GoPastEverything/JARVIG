# ADR-0057 — Joint markers are picked in the Character Editor

Status: Accepted
Date: 2026-10-01

## Context

ADR-0056 seated each socket on the ball that joins a part to its parent, and it said a viewport click on a joint-debug pivot selects that joint. Show Joints draws that marker for every joint. On Base and Base Male the fingers are a cluster of markers, and a click within 14 px of any of them selected that joint before the mesh. The human said the points are acceptable, and that a point is selected only when they are choosing it.

The level editor is the default workspace. The Character Editor is the workspace where a socket is chosen. ADR-0056 still owns the socket position and the default workspace. This decision changes only the pick.

## Decision

A joint-debug marker is a pick target only in the Character Editor. In that workspace, a marker under the cursor selects that joint before the mesh. The level editor selects an actor by its mesh. Show Joints may still draw the markers there. The gizmo handle of the current selection is tested before either pick, so a press on an axis or a ring starts a drag.

This partially supersedes only this sentence of ADR-0056: "A viewport click on a joint-debug pivot selects that joint."

## Consequences

Socket positions stay where ADR-0056 put them. JRV-0092 visual acceptance stays open. JRV-0090, the kinematic pawn, stays unstarted. Play In Editor is the existing free-fly pawn. This decision does not add gravity, collision, or a player-start actor.
