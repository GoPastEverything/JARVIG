import { Engine } from '@jarvig/core';
import { Scene, type EntityId } from '@jarvig/ecs';
import { createRenderModule, type RenderBackendName, type RenderModule } from '@jarvig/render';
import { createWorldModule, type WorldModule } from '@jarvig/world';

export interface EditorViewport {
  readonly bound: true;
  readonly cameraEntityId: EntityId;
  readonly backend: RenderBackendName;
}

/** Editor host session. The viewport is bound to a live engine scene, not a fake preview. */
export interface EditorSession {
  readonly engine: Engine;
  readonly scene: Scene;
  readonly world: WorldModule;
  readonly render: RenderModule;
  readonly viewport: EditorViewport;
  shutdown(): void;
}

export function createEditorSession(): EditorSession {
  const world = createWorldModule();
  const render = createRenderModule();
  const scene = new Scene();
  const cameraEntityId = scene.createEntity();
  const engine = Engine.create({
    profile: 'editor',
    modules: [world, render],
  });
  engine.tick(1 / 60);
  return {
    engine,
    scene,
    world,
    render,
    viewport: {
      bound: true,
      cameraEntityId,
      backend: render.device.backend,
    },
    shutdown(): void {
      engine.shutdown();
    },
  };
}
