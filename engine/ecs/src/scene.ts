import {
  SCENE_SCHEMA_ID,
  validateScene,
  type Scalar,
  type SceneSnapshot,
} from '@jarvig/scene-schema';
import { ComponentRegistry, validateComponentData, type ComponentSchema } from './schema.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export type EntityId = string;

interface StoredComponent {
  readonly version: number;
  readonly data: Record<string, Scalar>;
}

interface StoredEntity {
  readonly id: EntityId;
  parentId: EntityId | null;
  readonly components: Map<string, StoredComponent>;
}

export function createEntityId(): EntityId {
  return crypto.randomUUID();
}

function assertUuid(id: string, label: string): void {
  if (!UUID_RE.test(id)) throw new Error(`${label} must be a UUID`);
}

/**
 * Stable entity identity and hierarchy, separate from hot component storage.
 * Phase 0 storage is a map. Archetype/SoA storage may replace it later without
 * changing the schema or snapshot contract.
 */
export class Scene {
  private readonly entities = new Map<EntityId, StoredEntity>();

  constructor(readonly registry: ComponentRegistry = new ComponentRegistry()) {}

  createEntity(id: EntityId = createEntityId(), parentId: EntityId | null = null): EntityId {
    assertUuid(id, 'entity id');
    if (this.entities.has(id)) throw new Error(`entity '${id}' already exists`);
    if (parentId !== null) {
      assertUuid(parentId, 'parent id');
      if (!this.entities.has(parentId)) throw new Error(`parent '${parentId}' does not exist`);
    }
    this.entities.set(id, { id, parentId, components: new Map() });
    return id;
  }

  has(id: EntityId): boolean {
    return this.entities.has(id);
  }

  setParent(id: EntityId, parentId: EntityId | null): void {
    const entity = this.require(id);
    if (parentId !== null) {
      assertUuid(parentId, 'parent id');
      if (!this.entities.has(parentId)) throw new Error(`parent '${parentId}' does not exist`);
      let cursor: EntityId | null = parentId;
      while (cursor !== null) {
        if (cursor === id) throw new Error(`parent assignment would cycle at '${id}'`);
        cursor = this.require(cursor).parentId;
      }
    }
    entity.parentId = parentId;
  }

  parentOf(id: EntityId): EntityId | null {
    return this.require(id).parentId;
  }

  addComponent(id: EntityId, schema: ComponentSchema, data: unknown): void {
    const registered = this.registry.get(schema.name);
    if (registered.version !== schema.version) {
      throw new Error(
        `component '${schema.name}' version ${schema.version} does not match registered version ${registered.version}`,
      );
    }
    const entity = this.require(id);
    if (entity.components.has(registered.name)) {
      throw new Error(`entity '${id}' already has component '${registered.name}'`);
    }
    entity.components.set(registered.name, {
      version: registered.version,
      data: validateComponentData(registered, data),
    });
  }

  getComponent(id: EntityId, name: string): Readonly<Record<string, Scalar>> {
    const component = this.require(id).components.get(name);
    if (!component) throw new Error(`entity '${id}' has no component '${name}'`);
    return component.data;
  }

  duplicate(id: EntityId): EntityId {
    const source = this.require(id);
    const copyId = this.createEntity(createEntityId(), source.parentId);
    for (const [name, component] of source.components) {
      const schema = this.registry.get(name);
      this.addComponent(copyId, schema, { ...component.data });
    }
    return copyId;
  }

  serialize(): SceneSnapshot {
    const entities = [...this.entities.values()]
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
      .map((entity) => ({
        id: entity.id,
        parentId: entity.parentId,
        components: [...entity.components.entries()]
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([name, component]) => ({
            name,
            version: component.version,
            data: sortScalars(component.data),
          })),
      }));
    return { schema: SCENE_SCHEMA_ID, entities };
  }

  static fromSnapshot(input: unknown, registry: ComponentRegistry): Scene {
    const validated = validateScene(input);
    if (!validated.ok) {
      throw new Error(`invalid scene snapshot: ${validated.errors.join('; ')}`);
    }
    const scene = new Scene(registry);
    for (const entity of validated.value.entities) {
      scene.createEntity(entity.id, null);
    }
    for (const entity of validated.value.entities) {
      if (entity.parentId !== null) scene.setParent(entity.id, entity.parentId);
      for (const component of entity.components) {
        const schema = registry.get(component.name);
        if (schema.version !== component.version) {
          throw new Error(
            `component '${component.name}' snapshot version ${component.version} does not match registered version ${schema.version}; migration required`,
          );
        }
        scene.addComponent(entity.id, schema, component.data);
      }
    }
    return scene;
  }

  private require(id: EntityId): StoredEntity {
    const entity = this.entities.get(id);
    if (!entity) throw new Error(`entity '${id}' does not exist`);
    return entity;
  }
}

function sortScalars(data: Readonly<Record<string, Scalar>>): Record<string, Scalar> {
  const out: Record<string, Scalar> = {};
  for (const key of Object.keys(data).sort()) {
    const value = data[key];
    if (value === undefined) throw new Error(`missing scalar ${key}`);
    out[key] = value;
  }
  return out;
}
