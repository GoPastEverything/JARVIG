import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import { EditorShell, parseEditorCommand } from '@jarvig/editor-shell';
import { openNativeRuntime } from '@jarvig/node-host';
import { renderEditorDocument } from './view.js';

function createHostedShell(): EditorShell {
  return EditorShell.create(openNativeRuntime('editor'));
}

export function formatEditorStatus(shell: EditorShell): string {
  const viewport = shell.viewport();
  const panels = shell.dock.panels
    .filter((panel) => panel.visible)
    .map((panel) => panel.id)
    .join(',');
  return [
    'JARVIG_OK editor',
    `profile=${viewport.profile}`,
    `modules=${viewport.modules.join(',')}`,
    'viewport=bound',
    `camera=${viewport.cameraEntityId}`,
    `backend=${viewport.backend}`,
    `panels=${panels}`,
    `nativeFrame=${viewport.nativeFrame ?? 'none'}`,
  ].join(' ');
}

export function bootEditorOnce(): string {
  const shell = createHostedShell();
  const line = formatEditorStatus(shell);
  shell.shutdown();
  return line;
}

export function serveEditor(port = 4780, options?: { readonly signals?: boolean }): Server {
  const shell = createHostedShell();
  const server = createServer((request, response) => {
    void handle(shell, request, response);
  });
  server.listen(port, '127.0.0.1', () => {
    const address = server.address();
    const bound = typeof address === 'object' && address !== null ? address.port : port;
    process.stdout.write(`${formatEditorStatus(shell)}\nlistening=http://127.0.0.1:${bound}\n`);
  });
  const stop = (): void => {
    server.close();
    try {
      shell.shutdown();
    } catch {
      // A second stop is ignored. The engine rejects a second shutdown.
    }
  };
  if (options?.signals) {
    process.once('SIGINT', () => {
      stop();
      process.exit(0);
    });
    process.once('SIGTERM', () => {
      stop();
      process.exit(0);
    });
  }
  return server;
}

async function handle(shell: EditorShell, request: IncomingMessage, response: ServerResponse): Promise<void> {
  const url = request.url ?? '/';
  if (url.startsWith('/health')) {
    response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
    response.end('ok\n');
    return;
  }
  if (url.startsWith('/api/state')) {
    const viewport = shell.viewport();
    response.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });
    response.end(
      JSON.stringify({
        panels: shell.dock.panels.map((panel) => ({ id: panel.id, visible: panel.visible, region: panel.region })),
        activePanelId: shell.dock.activePanelId,
        cameraEntityId: viewport.cameraEntityId,
        frame: viewport.frame,
        nativeFrame: viewport.nativeFrame,
        nativeRenderExecuted: viewport.nativeRenderExecuted,
        backend: viewport.backend,
      }),
    );
    return;
  }
  if (request.method === 'POST' && url.startsWith('/api/command')) {
    try {
      const body = await readBody(request);
      const parsed = parseCommandBody(body, request.headers['content-type']);
      shell.execute(parsed);
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end(renderEditorDocument(shell.dock, shell.viewport()));
    } catch (error) {
      const message = error instanceof Error ? error.message : 'command failed';
      response.writeHead(400, { 'content-type': 'text/plain; charset=utf-8' });
      response.end(`${message}\n`);
    }
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end(renderEditorDocument(shell.dock, shell.viewport()));
}

function parseCommandBody(body: string, contentType: string | undefined): ReturnType<typeof parseEditorCommand> {
  if (contentType?.includes('application/json')) {
    const json = JSON.parse(body) as { type?: string; panelId?: string };
    return parseEditorCommand(json.type ?? '', json.panelId);
  }
  const params = new URLSearchParams(body);
  return parseEditorCommand(params.get('type') ?? '', params.get('panelId') ?? undefined);
}

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    request.on('data', (chunk: Buffer) => chunks.push(chunk));
    request.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    request.on('error', reject);
  });
}
