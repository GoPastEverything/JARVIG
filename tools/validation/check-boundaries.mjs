import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const defaultRoot = path.resolve(here, '../..');

const packageRoots = [
  'engine/math',
  'engine/core',
  'engine/ecs',
  'engine/world',
  'engine/assets',
  'engine/materials',
  'engine/render',
  'editor/shell',
  'apps/hub',
  'apps/editor',
  'hosts/client',
  'hosts/dedicated-server',
  'tools/cli',
  'packages/scene-schema',
  'packages/project-schema',
  'packages/node-host',
  'samples/minimal',
];

const allowed = {
  'engine/math': [],
  'engine/core': [],
  'engine/ecs': ['@jarvig/scene-schema'],
  'engine/world': ['@jarvig/core', '@jarvig/math'],
  'engine/assets': [],
  'engine/materials': [],
  'engine/render': ['@jarvig/core', '@jarvig/world'],
  'editor/shell': ['@jarvig/core', '@jarvig/ecs', '@jarvig/render', '@jarvig/world'],
  'apps/hub': ['@jarvig/core', '@jarvig/node-host'],
  'apps/editor': ['@jarvig/core', '@jarvig/editor-shell', '@jarvig/node-host'],
  'hosts/client': ['@jarvig/core', '@jarvig/materials', '@jarvig/node-host', '@jarvig/render', '@jarvig/world'],
  'hosts/dedicated-server': ['@jarvig/assets', '@jarvig/core', '@jarvig/node-host', '@jarvig/world'],
  'tools/cli': ['@jarvig/core', '@jarvig/node-host', '@jarvig/project-schema'],
  'packages/scene-schema': [],
  'packages/project-schema': [],
  'packages/node-host': ['koffi'],
  'samples/minimal': ['@jarvig/client', '@jarvig/core', '@jarvig/node-host'],
};

const allowedSets = new Map(Object.entries(allowed).map(([root, deps]) => [root, new Set(deps)]));

const importPatterns = [
  /\bfrom\s+['"]([^'"]+)['"]/g,
  /\bimport\s+['"]([^'"]+)['"]/g,
  /\bimport\(\s*['"]([^'"]+)['"]\s*\)/g,
  /\brequire\(\s*['"]([^'"]+)['"]\s*\)/g,
];

export function packageOf(relativePath) {
  const norm = relativePath.replaceAll('\\', '/');
  const matches = packageRoots.filter((root) => norm === root || norm.startsWith(`${root}/`));
  matches.sort((a, b) => b.length - a.length);
  return matches[0] ?? null;
}

const browserGlobal = /\b(?:document|window|HTMLElement|localStorage|navigator)\b/;

export function evaluateSpecifier(packageRoot, fromFile, specifier) {
  const from = fromFile.replaceAll('\\', '/');
  if (packageRoot.startsWith('engine/') && (specifier.startsWith('node:') || specifier === 'playcanvas')) {
    return `${from} imports '${specifier}'. Engine packages must not depend on a host runtime or PlayCanvas.`;
  }
  if (specifier.startsWith('node:')) return null;
  if (specifier.startsWith('.')) {
    const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(from), specifier));
    const owner = packageOf(resolved);
    if (owner !== packageRoot) {
      return `${from} relative import escapes the package: ${specifier}`;
    }
    return null;
  }
  const deps = allowedSets.get(packageRoot);
  if (!deps) return `${from} is not in a governed package`;
  const imported = specifier.startsWith('@jarvig/') ? specifier.split('/').slice(0, 2).join('/') : specifier;
  if (!deps.has(imported)) {
    return `${from} imports '${specifier}', which is not allowed for ${packageRoot}`;
  }
  return null;
}

function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
}

function specifiersIn(source) {
  const found = [];
  const clean = stripComments(source);
  for (const pattern of importPatterns) {
    pattern.lastIndex = 0;
    let match = pattern.exec(clean);
    while (match) {
      const specifier = match[1];
      if (specifier) found.push(specifier);
      match = pattern.exec(clean);
    }
  }
  return found;
}

function walk(dir, files) {
  for (const entry of readdirSync(dir)) {
    if (entry === 'dist' || entry === 'node_modules') continue;
    const full = path.join(dir, entry);
    const info = statSync(full);
    if (info.isDirectory()) walk(full, files);
    else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) files.push(full);
  }
}

export function checkBoundaries(repoRoot = defaultRoot) {
  const violations = [];
  for (const packageRoot of packageRoots) {
    const absolute = path.join(repoRoot, packageRoot);
    const files = [];
    walk(absolute, files);
    for (const file of files) {
      const relative = path.relative(repoRoot, file).replaceAll('\\', '/');
      const source = readFileSync(file, 'utf8');
      if (packageRoot.startsWith('engine/') && browserGlobal.test(stripComments(source))) {
        violations.push(`${relative} references a browser or DOM global`);
      }
      for (const specifier of specifiersIn(source)) {
        const violation = evaluateSpecifier(packageRoot, relative, specifier);
        if (violation) violations.push(violation);
      }
    }
  }
  violations.push(...nativeBoundaryViolations(repoRoot));
  return { ok: violations.length === 0, violations };
}

function nativeBoundaryViolations(repoRoot) {
  const violations = [];
  const nativeRoot = path.join(repoRoot, 'native');
  const manifests = [];
  walkCargo(nativeRoot, manifests);
  for (const file of manifests) {
    const relative = path.relative(repoRoot, file).replaceAll('\\', '/');
    const crate = relative.split('/')[1];
    const text = readFileSync(file, 'utf8');
    if (crate !== 'jarvig_rhi_wgpu' && /^\s*wgpu\s*=/m.test(text)) {
      violations.push(`${relative} depends on wgpu. Only jarvig_rhi_wgpu may.`);
    }
    if (crate !== 'jarvig_platform' && /^\s*winit\s*=/m.test(text)) {
      violations.push(`${relative} depends on winit. Only jarvig_platform may.`);
    }
  }
  const sources = [];
  walkRust(nativeRoot, sources);
  for (const file of sources) {
    const relative = path.relative(repoRoot, file).replaceAll('\\', '/');
    const text = stripComments(readFileSync(file, 'utf8'));
    if (!relative.startsWith('native/jarvig_rhi_wgpu/') && (/\bwgpu::/.test(text) || /\buse wgpu\b/.test(text))) {
      violations.push(`${relative} names wgpu. Only the private backend may.`);
    }
    const crate = relative.split('/')[1];
    const engineCrates = new Set([
      'jarvig_engine',
      'jarvig_renderer',
      'jarvig_core',
      'jarvig_material',
      'jarvig_rhi',
      'jarvig_rhi_wgpu',
      'jarvig_platform',
      'jarvig_editor_host',
    ]);
    if (engineCrates.has(crate) && /\b(DockWorkspace|PanelRegistry|WorkspaceCommand|WorldOutlinerModel|OutlinerNodeId|SelectionService|SelectionItem|InspectorModel)\b/.test(text)) {
      violations.push(`${relative} names the editor dock, outliner, selection, or inspector. Those stay in jarvig_editor.`);
    }
  }
  for (const file of manifests) {
    const relative = path.relative(repoRoot, file).replaceAll('\\', '/');
    const crate = relative.split('/')[1];
    if (crate !== 'jarvig_editor' && crate !== 'jarvig_editor_host') {
      const text = readFileSync(file, 'utf8');
      if (/^\s*jarvig_editor\s*=/m.test(text)) {
        violations.push(`${relative} depends on jarvig_editor. The engine must not link the dock.`);
      }
    }
  }
  const serverPackage = path.join(repoRoot, 'hosts', 'dedicated-server', 'package.json');
  const serverText = readFileSync(serverPackage, 'utf8');
  if (/"wgpu"|"winit"/.test(serverText)) {
    violations.push('hosts/dedicated-server depends on a GPU or window package');
  }
  return violations;
}

function walkCargo(dir, files) {
  for (const entry of readdirSync(dir)) {
    if (entry === 'target') continue;
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) walkCargo(full, files);
    else if (entry === 'Cargo.toml') files.push(full);
  }
}

function walkRust(dir, files) {
  for (const entry of readdirSync(dir)) {
    if (entry === 'target') continue;
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) walkRust(full, files);
    else if (entry.endsWith('.rs')) files.push(full);
  }
}

function isDirect() {
  const entry = process.argv[1];
  return Boolean(entry) && import.meta.url === pathToFileURL(entry).href;
}

if (isDirect()) {
  const result = checkBoundaries();
  if (!result.ok) {
    for (const violation of result.violations) {
      process.stderr.write(`${violation}\n`);
    }
    process.exitCode = 1;
  }
}
