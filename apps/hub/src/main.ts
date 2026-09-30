import { isDirectRun } from '@jarvig/node-host';
import { hubStatus } from './status.js';

if (isDirectRun(import.meta.url, process.argv[1])) {
  process.stdout.write(`${hubStatus()}\n`);
}
