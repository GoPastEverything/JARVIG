import { createAssetHandle, type AssetHandle } from '@jarvig/assets';
import { Engine } from '@jarvig/core';
import { openNativeRuntime } from '@jarvig/node-host';
import { createWorldModule, type WorldModule } from '@jarvig/world';

export interface ServerSession {
  readonly engine: Engine;
  readonly world: WorldModule;
  readonly asset: AssetHandle;
  shutdown(): void;
}

export function bootServer(): ServerSession {
  const world = createWorldModule();
  const asset = createAssetHandle('world:default');
  const engine = Engine.create({
    profile: 'server',
    modules: [world],
  });
  if (engine.hasModule('render')) {
    throw new Error('dedicated server loaded a render module');
  }
  const tick = engine.tick(1 / 60);
  if (tick.renderExecuted) {
    throw new Error('dedicated server executed a render stage');
  }
  const native = openNativeRuntime('server');
  const nativeTick = native.tick(1 / 60);
  if (nativeTick.renderExecuted) {
    native.shutdown();
    throw new Error('native server runtime executed a render stage');
  }
  return {
    engine,
    world,
    asset,
    shutdown(): void {
      native.shutdown();
      engine.shutdown();
    },
  };
}

export function formatServerStatus(session: ServerSession): string {
  return [
    'JARVIG_OK dedicated-server',
    `profile=${session.engine.profile}`,
    `modules=${session.engine.moduleList().join(',')}`,
    'graphics=none',
    `asset=${session.asset.id}:${session.asset.state}`,
  ].join(' ');
}
