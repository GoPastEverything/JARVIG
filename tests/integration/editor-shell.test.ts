import { EditorShell, type HeadlessNativeRuntime } from '@jarvig/editor-shell';
import { serveEditor } from '@jarvig/editor';
import type { AddressInfo } from 'node:net';
import { describe, expect, it } from 'vitest';

describe('editor shell', () => {
  it('drives the dock by commands and ticks the engine', () => {
    const shell = EditorShell.create();
    const camera = shell.viewport().cameraEntityId;
    expect(shell.session.scene.has(camera)).toBe(true);
    expect(shell.dock.panels.find((panel) => panel.id === 'viewport')?.region).toBe('center');
    expect(shell.viewport().frame).toBe(1);

    shell.execute({ type: 'layout.toggle', panelId: 'outliner' });
    expect(shell.dock.panels.find((panel) => panel.id === 'outliner')?.visible).toBe(false);
    expect(shell.commands.undo()).toBe(true);
    expect(shell.dock.panels.find((panel) => panel.id === 'outliner')?.visible).toBe(true);

    expect(() => shell.execute({ type: 'layout.toggle', panelId: 'viewport' })).toThrow(/cannot be closed/);
    const frame = shell.viewport().frame;
    shell.execute({ type: 'engine.tick' });
    expect(shell.viewport().frame).toBe(frame + 1);
    expect(shell.commands.undo()).toBe(false);
    expect(shell.viewport().frame).toBe(frame + 1);
    shell.shutdown();
  });

  it('ticks an injected native runtime beside the prototype', () => {
    let frames = 0;
    let shut = false;
    const native: HeadlessNativeRuntime = {
      profile: 'editor',
      get frame() {
        return frames;
      },
      tick() {
        frames += 1;
        return { frame: frames, fixedSteps: 1, renderExecuted: true, clamped: false };
      },
      shutdown() {
        shut = true;
      },
    };
    const shell = EditorShell.create(native);
    expect(shell.viewport().nativeFrame).toBe(1);
    expect(shell.viewport().frame).toBe(1);
    shell.execute({ type: 'engine.tick' });
    expect(shell.viewport().nativeFrame).toBe(2);
    expect(shell.viewport().frame).toBe(2);
    shell.shutdown();
    expect(shut).toBe(true);
  });

  it('serves a dock page whose commands call the same shell', async () => {
    const server = serveEditor(0);
    await new Promise<void>((resolve) => server.once('listening', () => resolve()));
    const address = server.address() as AddressInfo;
    const base = `http://127.0.0.1:${address.port}`;
    const html = await fetch(base).then((response) => response.text());
    expect(html).toContain('data-panel="viewport"');
    expect(html).toContain('data-region="center"');
    expect(html).toContain('viewport=bound');
    expect(html).toContain('Temporary panel host');

    const toggled = await fetch(`${base}/api/command`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ type: 'layout.toggle', panelId: 'inspector' }),
    });
    expect(toggled.status).toBe(200);
    const after = (await fetch(`${base}/api/state`).then((response) => response.json())) as {
      panels: { id: string; visible: boolean }[];
      frame: number;
    };
    expect(after.panels.find((panel) => panel.id === 'inspector')?.visible).toBe(false);
    expect(after.panels.find((panel) => panel.id === 'viewport')?.visible).toBe(true);

    await fetch(`${base}/api/command`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ type: 'engine.tick' }),
    });
    const ticked = (await fetch(`${base}/api/state`).then((response) => response.json())) as {
      frame: number;
      nativeFrame: number;
      nativeRenderExecuted: boolean;
    };
    expect(ticked.frame).toBe(after.frame + 1);
    expect(ticked.nativeFrame).toBe(ticked.frame);
    expect(ticked.nativeRenderExecuted).toBe(true);

    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  });
});