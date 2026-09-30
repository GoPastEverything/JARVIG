export type RenderBackendName = 'webgpu' | 'webgl2' | 'null';

/** GPU device boundary. Phase 0 ships the null device so hosts boot without a GPU. */
export interface RenderDevice {
  readonly backend: RenderBackendName;
  readonly label: string;
}

export function createNullRenderDevice(): RenderDevice {
  return { backend: 'null', label: 'null-device' };
}
