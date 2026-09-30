import type { EngineContext, EngineModule } from '@jarvig/core';
import { WORLD_MODULE_ID } from '@jarvig/world';
import { createNullRenderDevice, type RenderDevice } from './device.js';

export const RENDER_MODULE_ID = 'render';

export interface RenderModule extends EngineModule {
  readonly device: RenderDevice;
  readonly prepareCount: number;
  readonly executeCount: number;
}

export function createRenderModule(device: RenderDevice = createNullRenderDevice()): RenderModule {
  let prepareCount = 0;
  let executeCount = 0;
  return {
    id: RENDER_MODULE_ID,
    requires: [WORLD_MODULE_ID],
    device,
    get prepareCount() {
      return prepareCount;
    },
    get executeCount() {
      return executeCount;
    },
    initialize(ctx: EngineContext): void {
      ctx.hooks.onRenderPrepare(() => {
        prepareCount += 1;
      });
      ctx.hooks.onRenderExecute(() => {
        executeCount += 1;
      });
    },
    shutdown(): void {},
  };
}
