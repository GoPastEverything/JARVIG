export const MATERIAL_SCHEMA_ID = 'jarvig.material/v1' as const;

/** Baseline production shading models. Advanced models are added by later tickets, not by free strings. */
export type ShadingModel = 'default-lit' | 'unlit' | 'masked' | 'translucent' | 'transmissive' | 'decal';

export type ParameterType = 'scalar' | 'vector' | 'texture' | 'enum' | 'boolean';

export type TextureSemantic =
  | 'baseColor'
  | 'normal'
  | 'roughness'
  | 'metallic'
  | 'ambientOcclusion'
  | 'emissive'
  | 'height'
  | 'opacity'
  | 'transmission'
  | 'clearcoat'
  | 'anisotropy'
  | 'subsurface';

export type ColorSpace = 'srgb' | 'linear' | 'data';

export interface TextureBinding {
  readonly colorSpace: ColorSpace;
  /** Explicit channel when the source is packed. Never inferred at runtime. */
  readonly channel?: string;
}

export interface MaterialParameter {
  readonly name: string;
  readonly type: ParameterType;
  readonly default: number | readonly number[] | string | boolean;
  readonly group?: string;
  readonly min?: number;
  readonly max?: number;
}

export interface MaterialAssetDocument {
  readonly schema: typeof MATERIAL_SCHEMA_ID;
  readonly id: string;
  readonly name: string;
  readonly shadingModel: ShadingModel;
  readonly parameters: readonly MaterialParameter[];
  readonly textureSemantics: Readonly<Partial<Record<TextureSemantic, TextureBinding>>>;
}

export type SchemaValidation<T> = { ok: true; value: T } | { ok: false; errors: readonly string[] };

const SHADING = new Set<ShadingModel>(['default-lit', 'unlit', 'masked', 'translucent', 'transmissive', 'decal']);
const PARAM_TYPES = new Set<ParameterType>(['scalar', 'vector', 'texture', 'enum', 'boolean']);
const SEMANTICS = new Set<TextureSemantic>([
  'baseColor',
  'normal',
  'roughness',
  'metallic',
  'ambientOcclusion',
  'emissive',
  'height',
  'opacity',
  'transmission',
  'clearcoat',
  'anisotropy',
  'subsurface',
]);
const COLOR_SPACES = new Set<ColorSpace>(['srgb', 'linear', 'data']);
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function validateMaterial(input: unknown): SchemaValidation<MaterialAssetDocument> {
  const errors: string[] = [];
  if (!isRecord(input)) return { ok: false, errors: ['material must be an object'] };
  if (input['schema'] !== MATERIAL_SCHEMA_ID) errors.push(`schema must be ${MATERIAL_SCHEMA_ID}`);
  const id = input['id'];
  if (typeof id !== 'string' || !UUID_RE.test(id)) errors.push('id must be a UUID');
  const name = input['name'];
  if (typeof name !== 'string' || name.trim().length === 0) errors.push('name is required');
  const shadingModel = input['shadingModel'];
  if (typeof shadingModel !== 'string' || !SHADING.has(shadingModel as ShadingModel)) {
    errors.push(`shadingModel must be one of ${[...SHADING].join(', ')}`);
  }
  const parameters = parseParameters(input['parameters'], errors);
  const textureSemantics = parseTextures(input['textureSemantics'], errors);
  if (
    errors.length > 0 ||
    typeof id !== 'string' ||
    typeof name !== 'string' ||
    typeof shadingModel !== 'string' ||
    !SHADING.has(shadingModel as ShadingModel)
  ) {
    return { ok: false, errors };
  }
  return {
    ok: true,
    value: {
      schema: MATERIAL_SCHEMA_ID,
      id,
      name,
      shadingModel: shadingModel as ShadingModel,
      parameters,
      textureSemantics,
    },
  };
}

export function serializeMaterial(material: MaterialAssetDocument): string {
  return `${JSON.stringify(material, null, 2)}\n`;
}

export function parseMaterial(json: string): SchemaValidation<MaterialAssetDocument> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'invalid JSON';
    return { ok: false, errors: [message] };
  }
  return validateMaterial(parsed);
}

function parseParameters(input: unknown, errors: string[]): MaterialParameter[] {
  if (!Array.isArray(input)) {
    errors.push('parameters must be an array');
    return [];
  }
  const parameters: MaterialParameter[] = [];
  const names = new Set<string>();
  input.forEach((entry, index) => {
    if (!isRecord(entry)) {
      errors.push(`parameters[${index}] must be an object`);
      return;
    }
    const name = entry['name'];
    const type = entry['type'];
    const fallback = entry['default'];
    if (typeof name !== 'string' || name.trim().length === 0) {
      errors.push(`parameters[${index}].name is required`);
      return;
    }
    if (names.has(name)) errors.push(`duplicate parameter '${name}'`);
    names.add(name);
    if (typeof type !== 'string' || !PARAM_TYPES.has(type as ParameterType)) {
      errors.push(`parameters[${index}].type is invalid`);
      return;
    }
    const parameterType = type as ParameterType;
    if (!defaultMatches(parameterType, fallback)) {
      errors.push(`parameters[${index}].default does not match type ${parameterType}`);
      return;
    }
    const parameter: MaterialParameter = {
      name,
      type: parameterType,
      default: fallback as MaterialParameter['default'],
    };
    const group = entry['group'];
    if (group !== undefined) {
      if (typeof group !== 'string') errors.push(`parameters[${index}].group must be a string`);
      else parameters.push({ ...parameter, group });
    } else {
      parameters.push(parameter);
    }
    const min = entry['min'];
    const max = entry['max'];
    if (min !== undefined && typeof min !== 'number') errors.push(`parameters[${index}].min must be a number`);
    if (max !== undefined && typeof max !== 'number') errors.push(`parameters[${index}].max must be a number`);
    if (typeof min === 'number' || typeof max === 'number') {
      const last = parameters[parameters.length - 1];
      if (last && last.name === name) {
        parameters[parameters.length - 1] = {
          ...last,
          ...(typeof min === 'number' ? { min } : {}),
          ...(typeof max === 'number' ? { max } : {}),
        };
      }
    }
  });
  return parameters;
}

function defaultMatches(type: ParameterType, value: unknown): boolean {
  if (type === 'scalar') return typeof value === 'number' && Number.isFinite(value);
  if (type === 'boolean') return typeof value === 'boolean';
  if (type === 'enum' || type === 'texture') return typeof value === 'string';
  if (!Array.isArray(value) || value.length < 2 || value.length > 4) return false;
  return value.every((entry) => typeof entry === 'number' && Number.isFinite(entry));
}

function parseTextures(
  input: unknown,
  errors: string[],
): Partial<Record<TextureSemantic, TextureBinding>> {
  if (input === undefined) return {};
  if (!isRecord(input)) {
    errors.push('textureSemantics must be an object');
    return {};
  }
  const out: Partial<Record<TextureSemantic, TextureBinding>> = {};
  for (const [key, value] of Object.entries(input)) {
    if (!SEMANTICS.has(key as TextureSemantic)) {
      errors.push(`unknown texture semantic '${key}'`);
      continue;
    }
    if (!isRecord(value)) {
      errors.push(`textureSemantics.${key} must be an object`);
      continue;
    }
    const colorSpace = value['colorSpace'];
    if (typeof colorSpace !== 'string' || !COLOR_SPACES.has(colorSpace as ColorSpace)) {
      errors.push(`textureSemantics.${key}.colorSpace is invalid`);
      continue;
    }
    const binding: TextureBinding = { colorSpace: colorSpace as ColorSpace };
    const channel = value['channel'];
    if (channel !== undefined) {
      if (typeof channel !== 'string' || channel.length === 0) {
        errors.push(`textureSemantics.${key}.channel must be a non-empty string`);
        continue;
      }
      out[key as TextureSemantic] = { ...binding, channel };
    } else {
      out[key as TextureSemantic] = binding;
    }
  }
  return out;
}
