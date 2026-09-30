export type EditorPanelId = 'viewport' | 'outliner' | 'inspector' | 'content';
export type DockRegion = 'left' | 'center' | 'right' | 'bottom';

export interface DockPanel {
  readonly id: EditorPanelId;
  readonly title: string;
  readonly region: DockRegion;
  visible: boolean;
  readonly closable: boolean;
}

export interface DockLayout {
  panels: DockPanel[];
  activePanelId: EditorPanelId;
}

const DEFAULTS: readonly Omit<DockPanel, 'visible'>[] = [
  { id: 'outliner', title: 'Outliner', region: 'left', closable: true },
  { id: 'viewport', title: 'Viewport', region: 'center', closable: false },
  { id: 'inspector', title: 'Inspector', region: 'right', closable: true },
  { id: 'content', title: 'Content', region: 'bottom', closable: true },
];

export function createDefaultDock(): DockLayout {
  return {
    activePanelId: 'viewport',
    panels: DEFAULTS.map((panel) => ({ ...panel, visible: true })),
  };
}

export function panelById(dock: DockLayout, id: EditorPanelId): DockPanel {
  const panel = dock.panels.find((item) => item.id === id);
  if (!panel) throw new Error(`unknown editor panel '${id}'`);
  return panel;
}

export function isEditorPanelId(value: string): value is EditorPanelId {
  return value === 'viewport' || value === 'outliner' || value === 'inspector' || value === 'content';
}

export function cloneDock(dock: DockLayout): DockLayout {
  return {
    activePanelId: dock.activePanelId,
    panels: dock.panels.map((panel) => ({ ...panel })),
  };
}

export function docksEqual(a: DockLayout, b: DockLayout): boolean {
  if (a.activePanelId !== b.activePanelId || a.panels.length !== b.panels.length) return false;
  return a.panels.every((panel, index) => {
    const other = b.panels[index];
    return other !== undefined && panel.id === other.id && panel.visible === other.visible;
  });
}
