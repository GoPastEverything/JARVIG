export const PROJECT_SCHEMA_ID = 'jarvig.project/v1' as const;

export type ProjectTarget = 'webgpu-web' | 'windows-desktop' | 'dedicated-server';
export type RenderBackendSetting = 'webgpu' | 'webgl2';

export interface ProjectSettings {
  readonly fixedHz: number;
  readonly renderBackend: RenderBackendSetting;
  readonly largeWorldCoordinates: boolean;
}

export interface ProjectDocument {
  readonly $schema: typeof PROJECT_SCHEMA_ID;
  readonly name: string;
  readonly engine: string;
  readonly projectId: string;
  readonly defaultWorld: string;
  readonly targets: readonly ProjectTarget[];
  readonly settings: ProjectSettings;
}

export type SchemaValidation<T> = { ok: true; value: T } | { ok: false; errors: readonly string[] };

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const TARGETS = new Set<ProjectTarget>(['webgpu-web', 'windows-desktop', 'dedicated-server']);
const BACKENDS = new Set<RenderBackendSetting>(['webgpu', 'webgl2']);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function validateProject(input: unknown): SchemaValidation<ProjectDocument> {
  const errors: string[] = [];
  if (!isRecord(input)) return { ok: false, errors: ['project must be an object'] };
  if (input['$schema'] !== PROJECT_SCHEMA_ID) errors.push(`$schema must be ${PROJECT_SCHEMA_ID}`);
  const name = input['name'];
  if (typeof name !== 'string' || name.trim().length === 0) errors.push('name is required');
  const engine = input['engine'];
  if (typeof engine !== 'string' || engine.trim().length === 0) errors.push('engine is required');
  const projectId = input['projectId'];
  if (typeof projectId !== 'string' || !UUID_RE.test(projectId)) errors.push('projectId must be a UUID');
  const defaultWorld = input['defaultWorld'];
  if (typeof defaultWorld !== 'string' || defaultWorld.trim().length === 0) {
    errors.push('defaultWorld is required');
  }
  const targetsIn = input['targets'];
  const targets: ProjectTarget[] = [];
  if (!Array.isArray(targetsIn) || targetsIn.length === 0) {
    errors.push('targets must be a non-empty array');
  } else {
    for (const target of targetsIn) {
      if (typeof target !== 'string' || !TARGETS.has(target as ProjectTarget)) {
        errors.push(`unknown target '${String(target)}'`);
      } else {
        targets.push(target as ProjectTarget);
      }
    }
  }
  const settingsIn = input['settings'];
  if (!isRecord(settingsIn)) {
    errors.push('settings is required');
    return { ok: false, errors };
  }
  const fixedHz = settingsIn['fixedHz'];
  const renderBackend = settingsIn['renderBackend'];
  const largeWorldCoordinates = settingsIn['largeWorldCoordinates'];
  if (typeof fixedHz !== 'number' || !Number.isFinite(fixedHz) || fixedHz <= 0) {
    errors.push('settings.fixedHz must be a positive number');
  }
  if (typeof renderBackend !== 'string' || !BACKENDS.has(renderBackend as RenderBackendSetting)) {
    errors.push("settings.renderBackend must be 'webgpu' or 'webgl2'");
  }
  if (typeof largeWorldCoordinates !== 'boolean') {
    errors.push('settings.largeWorldCoordinates must be a boolean');
  }
  if (errors.length > 0 || typeof name !== 'string' || typeof engine !== 'string') {
    return { ok: false, errors };
  }
  if (
    typeof projectId !== 'string' ||
    typeof defaultWorld !== 'string' ||
    typeof fixedHz !== 'number' ||
    typeof renderBackend !== 'string' ||
    typeof largeWorldCoordinates !== 'boolean'
  ) {
    return { ok: false, errors };
  }
  return {
    ok: true,
    value: {
      $schema: PROJECT_SCHEMA_ID,
      name,
      engine,
      projectId,
      defaultWorld,
      targets,
      settings: {
        fixedHz,
        renderBackend: renderBackend as RenderBackendSetting,
        largeWorldCoordinates,
      },
    },
  };
}
