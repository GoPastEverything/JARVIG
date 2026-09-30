import { isDirectRun } from '@jarvig/node-host';
import { bootClient, formatClientStatus } from './boot.js';

export function runClient(argv: readonly string[]): string {
  const session = bootClient();
  const status = formatClientStatus(session);
  if (argv.includes('--run')) {
    process.stdout.write(`${status}\n`);
    const timer = setInterval(() => {
      session.engine.tick(1 / 60);
    }, 1000 / 60);
    const stop = (): void => {
      clearInterval(timer);
      session.shutdown();
      process.exit(0);
    };
    process.once('SIGINT', stop);
    process.once('SIGTERM', stop);
    return status;
  }
  session.shutdown();
  return status;
}

if (isDirectRun(import.meta.url, process.argv[1])) {
  const argv = process.argv.slice(2);
  const status = runClient(argv);
  if (!argv.includes('--run')) process.stdout.write(`${status}\n`);
}
