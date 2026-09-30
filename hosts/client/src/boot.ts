import { Engine } from '@jarvig/core';
import { MATERIAL_SCHEMA_ID } from '@jarvig/materials';
import { createRenderModule, type RenderModule } from '@jarvig/render';
import { createWorldModule, type WorldModule } from '@jarvig/world';

export interface ClientSession {
  readonly engine: Engine;
  readonly world: WorldModule;
  readonly render: RenderModule;
  readonly materialSchema: typeof MATERIAL_SCHEMA_ID;
  shutdown(): void;
}

export function bootClient(): ClientSession {
  const world = createWorldModule();
  const render = createRenderModule();
  const engine = Engine.create({
    profile: 'client',
    modules: [world, render],
  });
  engine.tick(1 / 60);
  return {
    engine,
    world,
    render,
    materialSchema: MATERIAL_SCHEMA_ID,
    shutdown(): void {
      engine.shutdown();
    },
  };
}

export function formatClientStatus(session: ClientSession): string {
  return [
    'JARVIG_OK client',
    `profile=${session.engine.profile}`,
    `modules=${session.engine.moduleList().join(',')}`,
    `backend=${session.render.device.backend}`,
    `materialSchema=${session.materialSchema}`,
  ].join(' ');
}
