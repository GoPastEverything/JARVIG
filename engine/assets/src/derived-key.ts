import { sha256Hex } from './sha256.js';

export interface DerivedKeyInput {
  readonly sourceBytes: Uint8Array;
  readonly importSettings: unknown;
  readonly importerVersion: string;
  readonly engineFormatVersion: string;
  readonly targetPlatform: string;
  readonly featureFlags: Readonly<Record<string, boolean | string | number>>;
}

/**
 * Content address of one derived product.
 * Any change to source bytes, import settings, tool version, engine format,
 * target platform, or feature flags must change the key.
 */
export function derivedKey(input: DerivedKeyInput): string {
  const payload = stableStringify({
    sourceBytesB64: bytesToBase64(input.sourceBytes),
    importSettings: input.importSettings,
    importerVersion: input.importerVersion,
    engineFormatVersion: input.engineFormatVersion,
    targetPlatform: input.targetPlatform,
    featureFlags: input.featureFlags,
  });
  return sha256Hex(new TextEncoder().encode(payload));
}

const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function bytesToBase64(bytes: Uint8Array): string {
  let out = '';
  for (let index = 0; index < bytes.length; index += 3) {
    const b0 = bytes[index] ?? 0;
    const b1 = index + 1 < bytes.length ? (bytes[index + 1] ?? 0) : 0;
    const b2 = index + 2 < bytes.length ? (bytes[index + 2] ?? 0) : 0;
    const triple = (b0 << 16) | (b1 << 8) | b2;
    out += BASE64[(triple >> 18) & 63] ?? '';
    out += BASE64[(triple >> 12) & 63] ?? '';
    out += index + 1 < bytes.length ? (BASE64[(triple >> 6) & 63] ?? '') : '=';
    out += index + 2 < bytes.length ? (BASE64[triple & 63] ?? '') : '=';
  }
  return out;
}

function stableStringify(value: unknown): string {
  if (value === undefined) throw new Error('derived key input cannot contain undefined');
  if (value === null || typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') {
    if (typeof value === 'number' && !Number.isFinite(value)) {
      throw new Error('derived key input cannot contain a non-finite number');
    }
    return JSON.stringify(value);
  }
  if (typeof value !== 'object') {
    throw new Error('derived key input contains an unsupported value');
  }
  if (Array.isArray(value)) {
    return `[${value.map((entry) => stableStringify(entry)).join(',')}]`;
  }
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  return `{${keys.map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`).join(',')}}`;
}
