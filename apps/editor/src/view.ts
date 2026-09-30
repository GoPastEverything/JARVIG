import type { DockLayout, EditorCommand, ViewportPresentation } from '@jarvig/editor-shell';

export function renderEditorDocument(dock: DockLayout, viewport: ViewportPresentation): string {
  const panels = dock.panels
    .filter((panel) => panel.visible)
    .map((panel) => {
      const active = panel.id === dock.activePanelId ? ' active' : '';
      const body =
        panel.id === 'viewport'
          ? viewportBody(viewport)
          : `<p class="placeholder">This panel is editor view state. It does not own an engine system.</p>`;
      return `<section class="panel region-${panel.region}${active}" data-panel="${panel.id}" data-region="${panel.region}">
        <header>${escapeHtml(panel.title)}</header>
        ${body}
      </section>`;
    })
    .join('');
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>JARVIG Editor</title>
  <style>
    :root { color-scheme: dark; }
    body { margin: 0; height: 100vh; display: grid; grid-template-rows: auto 1fr; background: #14181f; color: #e7edf5; font: 13px/1.4 Segoe UI, sans-serif; }
    header.bar { display: flex; gap: 8px; align-items: center; padding: 8px 10px; background: #0e1218; border-bottom: 1px solid #2a3342; }
    header.bar b { margin-right: 8px; }
    header.bar span { color: #9aabc0; }
    button { background: #1c2633; color: inherit; border: 1px solid #3a4a60; border-radius: 4px; padding: 4px 8px; }
    .dock { display: grid; grid-template-columns: 220px minmax(0, 1fr) 240px; grid-template-rows: minmax(0, 1fr) 150px; min-height: 0; }
    .region-left { grid-column: 1; grid-row: 1; }
    .region-center { grid-column: 2; grid-row: 1; }
    .region-right { grid-column: 3; grid-row: 1; }
    .region-bottom { grid-column: 1 / -1; grid-row: 2; }
    .panel { min-width: 0; min-height: 0; border: 1px solid #2a3342; display: flex; flex-direction: column; background: #1a212b; }
    .panel header { padding: 6px 8px; background: #222b38; border-bottom: 1px solid #2a3342; }
    .panel.active { outline: 1px solid #3d7ec9; }
    .viewport-body, .placeholder { margin: 0; padding: 12px; }
    .viewport-body { font-family: Consolas, monospace; white-space: pre-wrap; }
    .note { color: #9aabc0; }
  </style>
</head>
<body>
  <header class="bar">
    <b>JARVIG</b>
    <span>Temporary panel host. Not the native editor executable.</span>
    ${button('engine.tick', 'Tick engine')}
    ${button('layout.reset', 'Reset layout')}
    ${button('layout.toggle', 'Toggle outliner', 'outliner')}
    ${button('layout.toggle', 'Toggle inspector', 'inspector')}
    ${button('layout.focus', 'Focus viewport', 'viewport')}
  </header>
  <main class="dock">
    ${panels}
  </main>
</body>
</html>
`;
}

function viewportBody(viewport: ViewportPresentation): string {
  return `<pre class="viewport-body" data-viewport="engine">JARVIG_OK editor
profile=${escapeHtml(viewport.profile)}
modules=${escapeHtml(viewport.modules.join(','))}
viewport=bound
camera=${escapeHtml(viewport.cameraEntityId)}
backend=${escapeHtml(viewport.backend)}
frame=${viewport.frame}
nativeFrame=${viewport.nativeFrame ?? 'none'}

<span class="note">This pane consumes the engine view. It does not own a renderer.
nativeFrame is the headless native runtime. Pixels are a later RHI present, not this page.</span></pre>`;
}

function button(type: EditorCommand['type'], label: string, panelId?: string): string {
  const hidden = panelId ? `<input type="hidden" name="panelId" value="${panelId}">` : '';
  return `<form method="post" action="/api/command">${hidden}<input type="hidden" name="type" value="${type}"><button type="submit">${label}</button></form>`;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}
