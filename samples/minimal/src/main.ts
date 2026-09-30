import { bootClient, formatClientStatus } from '@jarvig/client';
import { isDirectRun } from '@jarvig/node-host';

export function runMinimalSample(): string {
  const session = bootClient();
  const client = formatClientStatus(session);
  session.shutdown();
  return `JARVIG_OK sample-minimal\n${client}\n`;
}

if (isDirectRun(import.meta.url, process.argv[1])) {
  process.stdout.write(runMinimalSample());
}
