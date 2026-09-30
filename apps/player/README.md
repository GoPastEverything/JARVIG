# Player

Reserved for the native game player. Not a Node host and not a browser wrapper.

A shipped game boots platform, engine, renderer, world, then the game module. It must not require Electron, Node, or a DOM. See [docs/platform/hosts.md](../../docs/platform/hosts.md).

The current client prototype is `hosts/client`. It boots the TypeScript engine and is not the shipping player.
