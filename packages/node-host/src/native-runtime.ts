import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const koffi = require('koffi') as {
  load: (file: string) => {
    func: (definition: string) => (...args: unknown[]) => unknown;
  };
  struct: (name: string, fields: Record<string, string>) => unknown;
};

export type NativeProfile = 'editor' | 'client' | 'server' | 'test';

export interface HeadlessNativeTick {
  readonly frame: number;
  readonly fixedSteps: number;
  readonly renderExecuted: boolean;
  readonly clamped: boolean;
}

export interface NativeRuntimeHandle {
  readonly profile: NativeProfile;
  readonly frame: number;
  tick(deltaSeconds: number): HeadlessNativeTick;
  shutdown(): void;
}

const PROFILES: Record<NativeProfile, number> = {
  editor: 1,
  client: 2,
  server: 3,
  test: 4,
};

interface TickOut {
  frame: number | bigint;
  fixed_steps: number;
  render_executed: number;
  clamped: number;
}

interface CoreLibrary {
  create(profile: number, fixedHz: number, maxSteps: number): unknown;
  tick(runtime: unknown, delta: number, out: TickOut): number;
  shutdown(runtime: unknown): number;
  destroy(runtime: unknown): void;
}

let library: CoreLibrary | undefined;

function repoRoot(): string {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
}

export function nativeCoreLibraryPath(): string {
  const file = process.platform === 'win32' ? 'jarvig_core.dll' : process.platform === 'darwin' ? 'libjarvig_core.dylib' : 'libjarvig_core.so';
  return path.join(repoRoot(), 'native', 'target', 'debug', file);
}

function ensureLibrary(): string {
  const dll = nativeCoreLibraryPath();
  if (!existsSync(dll)) {
    execFileSync('cargo', ['build', '-p', 'jarvig_core', '--manifest-path', path.join(repoRoot(), 'native', 'Cargo.toml')], {
      stdio: 'inherit',
    });
  }
  if (!existsSync(dll)) {
    throw new Error(`native core library was not produced at ${dll}`);
  }
  return dll;
}

function loadLibrary(): CoreLibrary {
  if (library) return library;
  const dll = ensureLibrary();
  const lib = koffi.load(dll);
  koffi.struct('JarvigRuntimeTick', {
    frame: 'uint64',
    fixed_steps: 'uint32',
    render_executed: 'int',
    clamped: 'int',
  });
  library = {
    create: lib.func('void *jarvig_runtime_create(uint32 profile, double fixed_hz, uint32 max_steps)') as CoreLibrary['create'],
    tick: lib.func('int32 jarvig_runtime_tick(void *runtime, double frame_delta_seconds, _Out_ JarvigRuntimeTick *out)') as CoreLibrary['tick'],
    shutdown: lib.func('int32 jarvig_runtime_shutdown(void *runtime)') as CoreLibrary['shutdown'],
    destroy: lib.func('void jarvig_runtime_destroy(void *runtime)') as CoreLibrary['destroy'],
  };
  return library;
}

/** Load the native headless runtime. This is a host binding, not an engine implementation. */
export function openNativeRuntime(profile: NativeProfile, fixedHz = 60, maxSteps = 8): NativeRuntimeHandle {
  const core = loadLibrary();
  const handle = core.create(PROFILES[profile], fixedHz, maxSteps);
  if (!handle) {
    throw new Error(`jarvig_runtime_create failed for profile ${profile}`);
  }
  let frame = 0;
  let closed = false;
  return {
    profile,
    get frame() {
      return frame;
    },
    tick(deltaSeconds: number): HeadlessNativeTick {
      if (closed) throw new Error('native runtime is shut down');
      const out: TickOut = { frame: 0, fixed_steps: 0, render_executed: 0, clamped: 0 };
      const code = core.tick(handle, deltaSeconds, out);
      if (code === -2) throw new Error('native runtime is shut down');
      if (code !== 0) throw new Error(`jarvig_runtime_tick failed (${code})`);
      frame = Number(out.frame);
      return {
        frame,
        fixedSteps: out.fixed_steps,
        renderExecuted: out.render_executed !== 0,
        clamped: out.clamped !== 0,
      };
    },
    shutdown(): void {
      if (closed) return;
      closed = true;
      core.shutdown(handle);
      core.destroy(handle);
    },
  };
}
