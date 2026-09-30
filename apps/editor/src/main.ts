import { isDirectRun } from '@jarvig/node-host';
import { bootEditorOnce, serveEditor } from './host.js';

export function main(argv: readonly string[]): void {
  if (argv.includes('--once')) {
    process.stdout.write(`${bootEditorOnce()}\n`);
    return;
  }
  const portArg = argv.find((arg) => arg.startsWith('--port='));
  const port = portArg ? Number(portArg.slice('--port='.length)) : 4780;
  if (!Number.isInteger(port) || port < 0) {
    throw new Error('editor port must be a non-negative integer');
  }
  serveEditor(port, { signals: true });
}

if (isDirectRun(import.meta.url, process.argv[1])) {
  main(process.argv.slice(2));
}
