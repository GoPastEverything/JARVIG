import type { EngineContext, EngineModule } from '@jarvig/core';
import { FrameGraph } from './frames.js';

export const WORLD_MODULE_ID = 'world';

export interface WorldModule extends EngineModule {
  readonly frames: FrameGraph;
  readonly streamingUpdates: number;
}

export function createWorldModule(): WorldModule {
  const frames = new FrameGraph();
  let streamingUpdates = 0;
  return {
    id: WORLD_MODULE_ID,
    frames,
    get streamingUpdates() {
      return streamingUpdates;
    },
    initialize(ctx: EngineContext): void {
      ctx.hooks.onStreaming(() => {
        streamingUpdates += 1;
      });
    },
    shutdown(): void {},
  };
}
