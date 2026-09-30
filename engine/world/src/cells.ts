import { vec3, type Vec3 } from '@jarvig/math';

export interface Aabb {
  readonly min: Vec3;
  readonly max: Vec3;
}

/** Partition record. Pages and proxies are named now so later systems do not invent a second grid. */
export interface WorldCell {
  readonly id: string;
  readonly frameId: string;
  readonly bounds: Aabb;
  readonly entities: readonly string[];
  readonly dependencies: readonly string[];
  readonly residentCostEstimate: number;
}

export interface CellEntity {
  readonly id: string;
  readonly position: Vec3;
}

export function assignCell(position: Vec3, cellSize: number, frameId: string): WorldCell {
  if (!(cellSize > 0) || !Number.isFinite(cellSize)) {
    throw new Error('cellSize must be a positive finite number');
  }
  const ix = Math.floor(position.x / cellSize);
  const iy = Math.floor(position.y / cellSize);
  const iz = Math.floor(position.z / cellSize);
  return {
    id: `${frameId}:${ix}:${iy}:${iz}`,
    frameId,
    bounds: {
      min: vec3(ix * cellSize, iy * cellSize, iz * cellSize),
      max: vec3((ix + 1) * cellSize, (iy + 1) * cellSize, (iz + 1) * cellSize),
    },
    entities: [],
    dependencies: [],
    residentCostEstimate: 0,
  };
}

/** Deterministic entity-to-cell assignment. Input order does not change the result order. */
export function assignEntities(frameId: string, cellSize: number, entities: readonly CellEntity[]): WorldCell[] {
  const grouped = new Map<string, { cell: WorldCell; entities: string[] }>();
  for (const entity of entities) {
    const cell = assignCell(entity.position, cellSize, frameId);
    const existing = grouped.get(cell.id);
    if (existing) {
      existing.entities.push(entity.id);
    } else {
      grouped.set(cell.id, { cell, entities: [entity.id] });
    }
  }
  return [...grouped.values()]
    .sort((a, b) => (a.cell.id < b.cell.id ? -1 : a.cell.id > b.cell.id ? 1 : 0))
    .map(({ cell, entities: ids }) => ({
      ...cell,
      entities: [...ids].sort(),
    }));
}
