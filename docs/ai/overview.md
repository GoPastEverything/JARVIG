# AI and navigation

Not implemented. `engine/ai/` is reserved.

- Decision and perception are separate from navigation.
- Nav chunks partition and stream with world cells.
- Local and dynamic nav supports moving interiors.
- Behavior tree, utility, or GOAP can share an action and task layer. None is chosen.
- Editor overlays must show perception, decisions, blackboard, paths, and cost.

Navigation is a worker-lane candidate. It must not block the frame on a cache miss; streaming rules apply. Server workers may run AI batches only where cell ownership allows. Server meshing is not that ownership model yet. See [../networking/overview.md](../networking/overview.md).
