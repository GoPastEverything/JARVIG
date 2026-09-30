export type Scalar = number | string | boolean;

export interface FieldMeta {
  readonly kind: 'number' | 'string' | 'boolean';
  readonly min?: number;
  readonly max?: number;
}

/** Versioned component contract. Editor and replication metadata live here, not in hidden prototype state. */
export interface ComponentSchema {
  readonly name: string;
  readonly version: number;
  readonly replicated: boolean;
  readonly fields: Readonly<Record<string, FieldMeta>>;
}

export class ComponentRegistry {
  private readonly schemas = new Map<string, ComponentSchema>();

  register(schema: ComponentSchema): void {
    if (this.schemas.has(schema.name)) {
      throw new Error(`component '${schema.name}' is already registered`);
    }
    if (!Number.isInteger(schema.version) || schema.version < 1) {
      throw new Error(`component '${schema.name}' version must be an integer >= 1`);
    }
    this.schemas.set(schema.name, schema);
  }

  get(name: string): ComponentSchema {
    const schema = this.schemas.get(name);
    if (!schema) throw new Error(`component '${name}' is not registered`);
    return schema;
  }

  has(name: string): boolean {
    return this.schemas.has(name);
  }

  list(): readonly ComponentSchema[] {
    return [...this.schemas.values()].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  }
}

export function validateComponentData(
  schema: ComponentSchema,
  data: unknown,
): Record<string, Scalar> {
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`component '${schema.name}' data must be an object`);
  }
  const record = data as Record<string, unknown>;
  const expected = Object.keys(schema.fields).sort();
  const actual = Object.keys(record).sort();
  if (expected.join('\0') !== actual.join('\0')) {
    throw new Error(
      `component '${schema.name}' fields must be exactly [${expected.join(', ')}], received [${actual.join(', ')}]`,
    );
  }
  const out: Record<string, Scalar> = {};
  for (const name of expected) {
    const field = schema.fields[name];
    const value = record[name];
    if (!field) throw new Error(`component '${schema.name}' is missing field metadata for ${name}`);
    if (field.kind === 'number') {
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        throw new Error(`component '${schema.name}'.${name} must be a finite number`);
      }
      if (field.min !== undefined && value < field.min) {
        throw new Error(`component '${schema.name}'.${name} is below min ${field.min}`);
      }
      if (field.max !== undefined && value > field.max) {
        throw new Error(`component '${schema.name}'.${name} is above max ${field.max}`);
      }
      out[name] = value;
    } else if (field.kind === 'string') {
      if (typeof value !== 'string') throw new Error(`component '${schema.name}'.${name} must be a string`);
      out[name] = value;
    } else if (typeof value !== 'boolean') {
      throw new Error(`component '${schema.name}'.${name} must be a boolean`);
    } else {
      out[name] = value;
    }
  }
  return out;
}
