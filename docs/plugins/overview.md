# Plugins

Not implemented. `packages/plugin-sdk/` is the reserved package name from the founding layout and does not exist yet.

Rules already in force:

- Downloaded project plugins are untrusted relative to the Hub and the update system.
- Collaboration never auto-executes arbitrary remote code.
- Importers validate sizes and types and run in workers or sandboxes where practical.
- Permissions are an explicit list. A missing permission fails closed.
- Editor and runtime entries are separate. The runtime entry must not import editor modules, or the boundary checker will reject it once it lives in a governed package.

Binary update signing has to be designed before public distribution. That work is not started and needs a human decision. Do not invent a signing scheme in a feature PR.
