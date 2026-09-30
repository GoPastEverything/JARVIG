import { isDirectRun } from '@jarvig/node-host';
import { bootServer, formatServerStatus } from './boot.js';

export function runServer(): string {
  const session = bootServer();
  const status = formatServerStatus(session);
  session.shutdown();
  return status;
}

if (isDirectRun(import.meta.url, process.argv[1])) {
  process.stdout.write(`${runServer()}\n`);
}
