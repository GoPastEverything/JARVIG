export type AssetResidency = 'unresolved' | 'loading' | 'resident' | 'evicted' | 'failed';

export interface AssetHandle {
  readonly id: string;
  readonly state: AssetResidency;
}

const transitions: Readonly<Record<AssetResidency, readonly AssetResidency[]>> = {
  unresolved: ['loading', 'failed'],
  loading: ['resident', 'failed'],
  resident: ['evicted'],
  evicted: ['loading', 'failed'],
  failed: ['loading'],
};

export function createAssetHandle(id: string, state: AssetResidency = 'unresolved'): AssetHandle {
  if (id.trim().length === 0) throw new Error('asset id is required');
  return { id, state };
}

export function transitionAsset(handle: AssetHandle, next: AssetResidency): AssetHandle {
  const allowed = transitions[handle.state];
  if (!allowed.includes(next)) {
    throw new Error(`illegal asset transition ${handle.state} -> ${next}`);
  }
  return { id: handle.id, state: next };
}
