import { bootEditorOnce } from '@jarvig/editor';
import { bootClient } from '@jarvig/client';
import { bootServer } from '@jarvig/dedicated-server';
import { createEditorSession } from '@jarvig/editor-shell';
import { hubStatus } from '@jarvig/hub';
import { runCli } from '@jarvig/cli';
import { createRenderModule } from '@jarvig/render';
import { Engine } from '@jarvig/core';
import { describe, expect, it } from 'vitest';
import { checkBoundaries, evaluateSpecifier } from '../../tools/validation/check-boundaries.mjs';

describe('hosts', () => {
  it('editor host boots the real engine and binds a viewport camera', () => {
    const session = createEditorSession();
    expect(session.viewport.bound).toBe(true);
    expect(session.scene.has(session.viewport.cameraEntityId)).toBe(true);
    expect(session.engine.profile).toBe('editor');
    expect(session.engine.hasModule('render')).toBe(true);
    expect(session.engine.hasModule('world')).toBe(true);
    expect(session.render.prepareCount).toBe(1);
    expect(session.render.executeCount).toBe(1);
    expect(session.world.streamingUpdates).toBe(1);
    session.shutdown();
    expect(bootEditorOnce()).toContain('JARVIG_OK editor');
  });

  it('client host boots with the null WebGPU-facing device', () => {
    const session = bootClient();
    expect(session.engine.profile).toBe('client');
    expect(session.render.device.backend).toBe('null');
    expect(session.render.prepareCount).toBe(1);
    expect(session.materialSchema).toBe('jarvig.material/v1');
    session.shutdown();
  });

  it('dedicated server boots a world without graphics', () => {
    const session = bootServer();
    expect(session.engine.profile).toBe('server');
    expect(session.engine.hasModule('render')).toBe(false);
    expect(session.engine.hasModule('world')).toBe(true);
    expect(session.asset.state).toBe('unresolved');
    expect(session.world.streamingUpdates).toBe(1);
    session.shutdown();
  });

  it('hub and cli report the same phase', () => {
    expect(hubStatus()).toContain('JARVIG_OK hub');
    expect(hubStatus()).toContain('phase=P0');
    const info = runCli(['info']);
    expect(info.exitCode).toBe(0);
    expect(info.stdout).toContain('JARVIG 0.0.1');
    expect(info.stdout).toContain('phase: P0');
    expect(info.stdout).toContain('virtualGeometry=off');
    expect(info.stdout).toContain('serverMeshing=off');
    const project = runCli(['project', 'validate', 'tests/fixtures/minimal.project.json']);
    expect(project.exitCode).toBe(0);
    expect(project.stdout).toContain('JARVIG_OK project Minimal');
    expect(runCli(['project', 'validate']).exitCode).toBe(2);
  });

  it('refuses to build a render module without a world module', () => {
    expect(() => Engine.create({ profile: 'client', modules: [createRenderModule()] })).toThrow(/world/);
  });
});

describe('dependency boundaries', () => {
  it('accepts the repository and rejects an engine import of the editor', () => {
    const result = checkBoundaries();
    expect(result.violations).toEqual([]);
    expect(result.ok).toBe(true);
    expect(
      evaluateSpecifier('engine/core', 'engine/core/src/engine.ts', '@jarvig/editor-shell'),
    ).toMatch(/not allowed/);
    expect(
      evaluateSpecifier('hosts/dedicated-server', 'hosts/dedicated-server/src/boot.ts', '@jarvig/render'),
    ).toMatch(/not allowed/);
    expect(
      evaluateSpecifier('engine/world', 'engine/world/src/frames.ts', '../../render/src/device.ts'),
    ).toMatch(/escapes/);
    expect(
      evaluateSpecifier('engine/assets', 'engine/assets/src/derived-key.ts', 'node:crypto'),
    ).toMatch(/must not depend/);
  });
});
