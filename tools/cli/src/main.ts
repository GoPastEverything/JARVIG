#!/usr/bin/env node
import { isDirectRun } from '@jarvig/node-host';
import { runCli } from './cli.js';

export function main(args: readonly string[]): number {
  const result = runCli(args);
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  return result.exitCode;
}

if (isDirectRun(import.meta.url, process.argv[1])) {
  process.exitCode = main(process.argv.slice(2));
}
