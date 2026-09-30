import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { createAssetHandle, derivedKey, sha256Hex, transitionAsset } from '@jarvig/assets';
import { Engine, JARVIG_VERSION, SimulationClock, defaultFeatureFlags } from '@jarvig/core';
import { ComponentRegistry, Scene, type ComponentSchema } from '@jarvig/ecs';
import { parseMaterial, validateMaterial } from '@jarvig/materials';
import { quatFromAxisAngle, quatIdentity, rotateVec, toCameraRelativeF32, vec3 } from '@jarvig/math';
import { validateProject } from '@jarvig/project-schema';
import { assignEntities, streamingPriority } from '@jarvig/world';
import { FrameGraph } from '@jarvig/world';
import { describe, expect, it } from 'vitest';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const Health: ComponentSchema = {
  name: 'Health',
  version: 1,
  replicated: true,
  fields: {
    current: { kind: 'number', min: 0 },
    max: { kind: 'number', min: 1 },
  },
};

describe('simulation clock', () => {
  it('consumes one step from an exact fixed delta', () => {
    const clock = new SimulationClock({ fixedHz: 60, maxStepsPerFrame: 8 });
    const step = clock.advance(1 / 60);
    expect(step.fixedSteps).toBe(1);
    expect(step.clamped).toBe(false);
    expect(step.alpha).toBeLessThan(1e-9);
  });

  it('accumulates variable render deltas', () => {
    const clock = new SimulationClock({ fixedHz: 60, maxStepsPerFrame: 8 });
    expect(clock.advance(0.5 / 60).fixedSteps).toBe(0);
    expect(clock.advance(0.5 / 60).fixedSteps).toBe(1);
    expect(clock.advance(1 / 60).fixedSteps).toBe(1);
  });

  it('drops excess time instead of spiraling', () => {
    const clock = new SimulationClock({ fixedHz: 60, maxStepsPerFrame: 8 });
    const burst = clock.advance(10);
    expect(burst.fixedSteps).toBe(8);
    expect(burst.clamped).toBe(true);
    expect(clock.advance(1 / 60).fixedSteps).toBe(1);
  });

  it('rejects a non-finite delta', () => {
    const clock = new SimulationClock();
    expect(() => clock.advance(Number.NaN)).toThrow(/finite/);
  });
});

describe('engine lifecycle', () => {
  it('boots, ticks, records telemetry, and shuts down', () => {
    const engine = Engine.create({ profile: 'test' });
    const tick = engine.tick(1 / 60);
    expect(tick.fixedSteps).toBe(1);
    expect(tick.renderExecuted).toBe(true);
    expect(engine.telemetry.snapshot()['frame.tick']?.count).toBe(1);
    expect(engine.flags).toEqual(defaultFeatureFlags);
    engine.shutdown();
    expect(() => engine.tick(1 / 60)).toThrow(/shut down/);
  });

  it('does not run render hooks on the server profile', () => {
    let fixed = 0;
    let prepared = 0;
    const engine = Engine.create({
      profile: 'server',
      modules: [
        {
          id: 'spy',
          initialize(ctx) {
            ctx.hooks.onFixedStep(() => {
              fixed += 1;
            });
            ctx.hooks.onRenderPrepare(() => {
              prepared += 1;
            });
          },
          shutdown() {},
        },
      ],
    });
    const tick = engine.tick(1 / 60);
    expect(fixed).toBe(1);
    expect(prepared).toBe(0);
    expect(tick.renderExecuted).toBe(false);
    expect(engine.telemetry.snapshot()['frame.renderSkipped']?.count).toBe(1);
    engine.shutdown();
  });

  it('rejects a missing module requirement and a cycle', () => {
    expect(() =>
      Engine.create({
        profile: 'test',
        modules: [{ id: 'child', requires: ['missing'], initialize() {}, shutdown() {} }],
      }),
    ).toThrow(/missing/);

    expect(() =>
      Engine.create({
        profile: 'test',
        modules: [
          { id: 'a', requires: ['b'], initialize() {}, shutdown() {} },
          { id: 'b', requires: ['a'], initialize() {}, shutdown() {} },
        ],
      }),
    ).toThrow(/cycle/);
  });

  it('keeps research flags off unless a host opts in', () => {
    const engine = Engine.create({ profile: 'test', featureFlags: { virtualGeometry: true } });
    expect(engine.flags.virtualGeometry).toBe(true);
    expect(engine.flags.proceduralMicrogeometry).toBe(false);
    expect(engine.flags.serverMeshing).toBe(false);
    engine.shutdown();
  });
});

describe('hierarchical frames', () => {
  it('keeps a centimeter offset at a far origin that float32 cannot', () => {
    const frames = new FrameGraph();
    frames.add({
      frameId: 'planet',
      parentFrameId: null,
      position: vec3(8_000_000, 0, 0),
      rotation: quatIdentity(),
    });
    frames.add({
      frameId: 'ship',
      parentFrameId: 'planet',
      position: vec3(0, 0, 0),
      rotation: quatIdentity(),
    });
    const world = frames.rootPosition('ship', vec3(0.25, 0, 0));
    expect(world.x).toBeCloseTo(8_000_000.25, 6);
    expect(Math.fround(world.x) - Math.fround(8_000_000)).toBe(0);
    const relative = toCameraRelativeF32(world, vec3(8_000_000, 0, 0));
    expect(relative.x).toBeCloseTo(0.25, 5);
    expect(relative.x).not.toBe(0);
  });

  it('rotates a child offset by the parent frame', () => {
    const frames = new FrameGraph();
    frames.add({
      frameId: 'root',
      parentFrameId: null,
      position: vec3(0, 0, 0),
      rotation: quatFromAxisAngle(vec3(0, 1, 0), Math.PI / 2),
    });
    const world = frames.rootPosition('root', vec3(1, 0, 0));
    expect(world.x).toBeCloseTo(0, 6);
    expect(world.z).toBeCloseTo(-1, 6);
    expect(rotateVec(quatFromAxisAngle(vec3(0, 1, 0), Math.PI / 2), vec3(1, 0, 0)).z).toBeCloseTo(-1, 6);
  });
});

describe('world cells and streaming priority', () => {
  it('assigns the same cell regardless of input order', () => {
    const first = { id: 'a', position: vec3(1, 1, 1) };
    const second = { id: 'b', position: vec3(129, -1, 10) };
    const forward = assignEntities('planet', 128, [first, second]);
    const backward = assignEntities('planet', 128, [second, first]);
    expect(forward).toEqual(backward);
    expect(forward.map((cell) => cell.id)).toEqual(['planet:0:0:0', 'planet:1:-1:0']);
    expect(forward[0]?.entities).toEqual(['a']);
    expect(forward[1]?.entities).toEqual(['b']);
  });

  it('prefers nearer and more important cells', () => {
    const near = streamingPriority({ distance: 10, viewAlignment: 0, gameplayImportance: 0, memoryPressure: 0 });
    const far = streamingPriority({ distance: 1000, viewAlignment: 0, gameplayImportance: 0, memoryPressure: 0 });
    const important = streamingPriority({
      distance: 1000,
      viewAlignment: 1,
      gameplayImportance: 1,
      memoryPressure: 0,
    });
    const pressured = streamingPriority({ distance: 10, viewAlignment: 0, gameplayImportance: 0, memoryPressure: 1 });
    expect(near).toBeGreaterThan(far);
    expect(important).toBeGreaterThan(far);
    expect(pressured).toBeLessThan(near);
  });
});

describe('scene identity', () => {
  it('round-trips a golden scene and gives duplicates a new id', () => {
    const registry = new ComponentRegistry();
    registry.register(Health);
    const parent = '22222222-2222-4222-8222-222222222222';
    const child = '11111111-1111-4111-8111-111111111111';
    const scene = new Scene(registry);
    scene.createEntity(parent);
    scene.createEntity(child, parent);
    scene.addComponent(child, Health, { current: 80, max: 100 });
    const golden = JSON.parse(readFileSync(path.join(root, 'tests/golden/health-scene.json'), 'utf8')) as unknown;
    expect(scene.serialize()).toEqual(golden);

    const loaded = Scene.fromSnapshot(golden, registry);
    expect(loaded.getComponent(child, 'Health')).toEqual({ current: 80, max: 100 });
    expect(loaded.parentOf(child)).toBe(parent);
    expect(registry.get('Health').replicated).toBe(true);

    const copy = loaded.duplicate(child);
    expect(copy).not.toBe(child);
    expect(loaded.getComponent(copy, 'Health')).toEqual({ current: 80, max: 100 });
    expect(loaded.parentOf(copy)).toBe(parent);
    expect(loaded.serialize().entities.map((entity) => entity.id)).toContain(child);
  });

  it('refuses a snapshot whose component version has no migration', () => {
    const registry = new ComponentRegistry();
    registry.register({ ...Health, version: 2 });
    const snapshot = {
      schema: 'jarvig.scene/v1',
      entities: [
        {
          id: '11111111-1111-4111-8111-111111111111',
          parentId: null,
          components: [{ name: 'Health', version: 1, data: { current: 1, max: 1 } }],
        },
      ],
    };
    expect(() => Scene.fromSnapshot(snapshot, registry)).toThrow(/migration required/);
  });
});

describe('sha256 without a host runtime', () => {
  it('matches Node crypto for the standard abc vector and a derived-key payload', () => {
    const abc = new TextEncoder().encode('abc');
    expect(sha256Hex(abc)).toBe('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
    const payload = 'jarvig-derived-key';
    expect(sha256Hex(new TextEncoder().encode(payload))).toBe(createHash('sha256').update(payload).digest('hex'));
  });
});

describe('derived data keys', () => {
  const base = {
    sourceBytes: new Uint8Array([1, 2, 3, 4]),
    importSettings: { scale: 1, tags: ['a', 'b'] },
    importerVersion: '0.0.1',
    engineFormatVersion: '1',
    targetPlatform: 'webgpu-web',
    featureFlags: { virtualGeometry: false },
  };

  it('is stable and changes when source, settings, or tool version change', () => {
    const first = derivedKey(base);
    const reordered = derivedKey({ ...base, importSettings: { tags: ['a', 'b'], scale: 1 } });
    expect(reordered).toBe(first);
    expect(derivedKey({ ...base, sourceBytes: new Uint8Array([1, 2, 3, 5]) })).not.toBe(first);
    expect(derivedKey({ ...base, importSettings: { scale: 2, tags: ['a', 'b'] } })).not.toBe(first);
    expect(derivedKey({ ...base, importerVersion: '0.0.2' })).not.toBe(first);
    expect(derivedKey({ ...base, targetPlatform: 'dedicated-server' })).not.toBe(first);
    expect(derivedKey({ ...base, featureFlags: { virtualGeometry: true } })).not.toBe(first);
  });
});

describe('asset handles', () => {
  it('moves through residency and rejects an illegal jump', () => {
    let handle = createAssetHandle('mesh:crate');
    expect(handle.state).toBe('unresolved');
    handle = transitionAsset(handle, 'loading');
    handle = transitionAsset(handle, 'resident');
    handle = transitionAsset(handle, 'evicted');
    expect(handle.state).toBe('evicted');
    expect(() => transitionAsset(handle, 'resident')).toThrow(/illegal asset transition/);
  });
});

describe('material and project schemas', () => {
  const material = {
    schema: 'jarvig.material/v1',
    id: '33333333-3333-4333-8333-333333333333',
    name: 'RockCliff',
    shadingModel: 'default-lit',
    parameters: [
      { name: 'roughness', type: 'scalar', default: 0.72, min: 0, max: 1, group: 'Surface' },
      { name: 'albedo', type: 'texture', default: 'tex:albedo' },
    ],
    textureSemantics: {
      baseColor: { colorSpace: 'srgb' },
      normal: { colorSpace: 'linear' },
      roughness: { colorSpace: 'data', channel: 'g' },
    },
  };

  it('round-trips a canonical material and rejects an unknown shading model', () => {
    const validated = validateMaterial(material);
    expect(validated.ok).toBe(true);
    if (!validated.ok) return;
    const parsed = parseMaterial(JSON.stringify(validated.value));
    expect(parsed).toEqual(validated);
    const broken = validateMaterial({ ...material, shadingModel: 'nanite' });
    expect(broken.ok).toBe(false);
  });

  it('validates the minimal project fixture', () => {
    const fixture = JSON.parse(readFileSync(path.join(root, 'tests/fixtures/minimal.project.json'), 'utf8')) as unknown;
    const result = validateProject(fixture);
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value.settings.largeWorldCoordinates).toBe(true);
    const bad = validateProject({ ...result, settings: undefined });
    expect(bad.ok).toBe(false);
  });
});

describe('version pin', () => {
  it('matches the root package version', () => {
    const pkg = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8')) as { version: string };
    expect(pkg.version).toBe(JARVIG_VERSION);
  });
});
