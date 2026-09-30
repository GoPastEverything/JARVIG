import type { EngineModule } from './module.js';

export function sortModules(modules: readonly EngineModule[]): EngineModule[] {
  const byId = new Map<string, EngineModule>();
  for (const mod of modules) {
    if (byId.has(mod.id)) {
      throw new Error(`duplicate engine module '${mod.id}'`);
    }
    byId.set(mod.id, mod);
  }
  for (const mod of modules) {
    for (const requirement of mod.requires ?? []) {
      if (!byId.has(requirement)) {
        throw new Error(`module '${mod.id}' requires missing module '${requirement}'`);
      }
    }
  }

  const sorted: EngineModule[] = [];
  const state = new Map<string, 'visiting' | 'done'>();
  const visit = (id: string): void => {
    const mark = state.get(id);
    if (mark === 'done') return;
    if (mark === 'visiting') {
      throw new Error(`engine module cycle at '${id}'`);
    }
    state.set(id, 'visiting');
    const mod = byId.get(id);
    if (!mod) {
      throw new Error(`missing engine module '${id}'`);
    }
    for (const requirement of mod.requires ?? []) visit(requirement);
    state.set(id, 'done');
    sorted.push(mod);
  };
  for (const mod of modules) visit(mod.id);
  return sorted;
}
