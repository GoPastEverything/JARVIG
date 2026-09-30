export { nativeCoreLibraryPath, openNativeRuntime } from './native-runtime.js';
import { pathToFileURL } from 'node:url';

/** True when this module is the Node process entrypoint. Hosts only. The engine does not link this. */
export function isDirectRun(metaUrl: string, argv1: string | undefined): boolean {
  if (!argv1) return false;
  return metaUrl === pathToFileURL(argv1).href;
}
