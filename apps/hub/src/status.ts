import {
  FOUNDING_DOCUMENT,
  JARVIG_MILESTONE,
  JARVIG_NAME,
  JARVIG_PHASE,
  JARVIG_VERSION,
} from '@jarvig/core';

export function hubStatus(): string {
  return [
    'JARVIG_OK hub',
    `name=${JARVIG_NAME}`,
    `engine=${JARVIG_VERSION}`,
    `phase=${JARVIG_PHASE}`,
    `milestone=${JARVIG_MILESTONE}`,
    `founding-doc=${FOUNDING_DOCUMENT}`,
    'launch-editor=pnpm dev:editor',
  ].join('\n');
}
