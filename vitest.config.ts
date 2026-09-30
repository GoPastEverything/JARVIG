import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const root = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    environment: 'node',
    root,
  },
  resolve: {
    alias: {
      '@jarvig/scene-schema': path.join(root, 'packages/scene-schema/src/index.ts'),
      '@jarvig/project-schema': path.join(root, 'packages/project-schema/src/index.ts'),
      '@jarvig/node-host': path.join(root, 'packages/node-host/src/index.ts'),
      '@jarvig/math': path.join(root, 'engine/math/src/index.ts'),
      '@jarvig/core': path.join(root, 'engine/core/src/index.ts'),
      '@jarvig/ecs': path.join(root, 'engine/ecs/src/index.ts'),
      '@jarvig/world': path.join(root, 'engine/world/src/index.ts'),
      '@jarvig/assets': path.join(root, 'engine/assets/src/index.ts'),
      '@jarvig/materials': path.join(root, 'engine/materials/src/index.ts'),
      '@jarvig/render': path.join(root, 'engine/render/src/index.ts'),
      '@jarvig/editor-shell': path.join(root, 'editor/shell/src/index.ts'),
      '@jarvig/hub': path.join(root, 'apps/hub/src/index.ts'),
      '@jarvig/editor': path.join(root, 'apps/editor/src/index.ts'),
      '@jarvig/client': path.join(root, 'hosts/client/src/index.ts'),
      '@jarvig/dedicated-server': path.join(root, 'hosts/dedicated-server/src/index.ts'),
      '@jarvig/cli': path.join(root, 'tools/cli/src/index.ts'),
    },
  },
});
