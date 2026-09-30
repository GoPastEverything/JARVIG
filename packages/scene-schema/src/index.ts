export const SCENE_SCHEMA_ID = 'jarvig.scene/v1' as const;

export type Scalar = number | string | boolean;

export interface SceneComponentRecord {
  readonly name: string;
  readonly version: number;
  readonly data: Readonly<Record<string, Scalar>>;
}

export interface SceneEntityRecord {
  readonly id: string;
  readonly parentId: string | null;
  readonly components: readonly SceneComponentRecord[];
}

export interface SceneSnapshot {
  readonly schema: typeof SCENE_SCHEMA_ID;
  readonly entities: readonly SceneEntityRecord[];
}

export type SchemaValidation<T> = { ok: true; value: T } | { ok: false; errors: readonly string[] };

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isScalar(value: unknown): value is Scalar {
  return typeof value === 'number' || typeof value === 'string' || typeof value === 'boolean';
}

export function validateScene(input: unknown): SchemaValidation<SceneSnapshot> {
  const errors: string[] = [];
  if (!isRecord(input)) return { ok: false, errors: ['scene must be an object'] };
  if (input['schema'] !== SCENE_SCHEMA_ID) {
    errors.push(`schema must be ${SCENE_SCHEMA_ID}`);
  }
  const entities = input['entities'];
  if (!Array.isArray(entities)) {
    errors.push('entities must be an array');
    return { ok: false, errors };
  }
  const parsed: SceneEntityRecord[] = [];
  entities.forEach((entity, index) => {
    if (!isRecord(entity)) {
      errors.push(`entities[${index}] must be an object`);
      return;
    }
    const id = entity['id'];
    if (typeof id !== 'string' || !UUID_RE.test(id)) {
      errors.push(`entities[${index}].id must be a UUID`);
    }
    const parentId = entity['parentId'];
    if (parentId !== null && (typeof parentId !== 'string' || !UUID_RE.test(parentId))) {
      errors.push(`entities[${index}].parentId must be a UUID or null`);
    }
    const components = entity['components'];
    if (!Array.isArray(components)) {
      errors.push(`entities[${index}].components must be an array`);
      return;
    }
    const parsedComponents: SceneComponentRecord[] = [];
    components.forEach((component, componentIndex) => {
      if (!isRecord(component)) {
        errors.push(`entities[${index}].components[${componentIndex}] must be an object`);
        return;
      }
      const name = component['name'];
      const version = component['version'];
      const data = component['data'];
      if (typeof name !== 'string' || name.length === 0) {
        errors.push(`entities[${index}].components[${componentIndex}].name is required`);
      }
      if (typeof version !== 'number' || !Number.isInteger(version) || version < 1) {
        errors.push(`entities[${index}].components[${componentIndex}].version must be an integer >= 1`);
      }
      if (!isRecord(data)) {
        errors.push(`entities[${index}].components[${componentIndex}].data must be an object`);
        return;
      }
      const scalarData: Record<string, Scalar> = {};
      for (const [key, value] of Object.entries(data)) {
        if (!isScalar(value) || (typeof value === 'number' && !Number.isFinite(value))) {
          errors.push(`entities[${index}].components[${componentIndex}].data.${key} must be a finite scalar`);
        } else {
          scalarData[key] = value;
        }
      }
      if (typeof name === 'string' && typeof version === 'number') {
        parsedComponents.push({ name, version, data: scalarData });
      }
    });
    if (typeof id === 'string' && (parentId === null || typeof parentId === 'string')) {
      parsed.push({ id, parentId, components: parsedComponents });
    }
  });
  if (errors.length > 0) return { ok: false, errors };
  return { ok: true, value: { schema: SCENE_SCHEMA_ID, entities: parsed } };
}
