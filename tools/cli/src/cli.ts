import { readFileSync } from 'node:fs';
import {
  defaultFeatureFlags,
  FOUNDING_DOCUMENT,
  JARVIG_MILESTONE,
  JARVIG_NAME,
  JARVIG_PHASE,
  JARVIG_VERSION,
} from '@jarvig/core';
import { validateProject } from '@jarvig/project-schema';

export interface CliResult {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
}

export function runCli(args: readonly string[]): CliResult {
  const [command, subcommand, file] = args;
  if (command === undefined || command === 'info') return info();
  if (command === 'version') return ok(`${JARVIG_VERSION}\n`);
  if (command === 'project' && subcommand === 'validate') return validate(file);
  return fail(2, 'usage: jarvig info | version | project validate <file>\n');
}

function info(): CliResult {
  const flags = Object.entries(defaultFeatureFlags)
    .map(([name, enabled]) => `${name}=${enabled ? 'on' : 'off'}`)
    .join(' ');
  return ok(
    [
      `${JARVIG_NAME} ${JARVIG_VERSION}`,
      `phase: ${JARVIG_PHASE}`,
      `milestone: ${JARVIG_MILESTONE}`,
      `founding-doc: ${FOUNDING_DOCUMENT}`,
      'render-backend-target: webgpu',
      'hosts: hub, editor, client, dedicated-server',
      `research-flags: ${flags}`,
      '',
    ].join('\n'),
  );
}

function validate(file: string | undefined): CliResult {
  if (!file) return fail(2, 'usage: jarvig project validate <file>\n');
  let text: string;
  try {
    text = readFileSync(file, 'utf8');
  } catch (error) {
    return fail(1, `${error instanceof Error ? error.message : 'read failed'}\n`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text) as unknown;
  } catch (error) {
    return fail(1, `${error instanceof Error ? error.message : 'invalid JSON'}\n`);
  }
  const result = validateProject(parsed);
  if (!result.ok) return fail(1, `${result.errors.join('\n')}\n`);
  return ok(`JARVIG_OK project ${result.value.name}\n`);
}

function ok(stdout: string): CliResult {
  return { exitCode: 0, stdout, stderr: '' };
}

function fail(exitCode: number, stderr: string): CliResult {
  return { exitCode, stdout: '', stderr };
}
