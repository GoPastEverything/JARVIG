export type NativeProfile = 'editor' | 'client' | 'server' | 'test';

export interface HeadlessNativeTick {
  readonly frame: number;
  readonly fixedSteps: number;
  readonly renderExecuted: boolean;
  readonly clamped: boolean;
}

/**
 * The headless native runtime behind the C ABI.
 * The editor does not implement this. A host loads `jarvig_core` and passes it in.
 * It is not a renderer and it does not present a frame.
 */
export interface HeadlessNativeRuntime {
  readonly profile: NativeProfile;
  readonly frame: number;
  tick(deltaSeconds: number): HeadlessNativeTick;
  shutdown(): void;
}
