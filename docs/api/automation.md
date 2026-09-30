# Automation

Tools outside the process will need to drive the engine: a build farm, a test harness, a remote editor. That boundary is an adapter. It is not the engine API.

```text
Remote tool
    |
    |  some wire, not chosen
    v
Automation host
    |
    v
Engine command or runtime API
```

The wire might later be pipes, JSON-RPC, or something else. This doctrine does not pick. It forbids two mistakes:

- Making the JSON document the API that in-process callers also have to speak.
- Teaching the wire a private import path that the CLI does not use.

The automation host is a consumer, like the editor and the CLI. It submits `CookProject` or `ImportAsset`. It does not link a private cooker.

In-process tests keep calling the engine directly. They do not need a socket.

Nothing in this layer is implemented.
