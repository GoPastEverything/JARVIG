import type { EditorSession } from './session.js';
import {
  cloneDock,
  createDefaultDock,
  docksEqual,
  isEditorPanelId,
  panelById,
  type DockLayout,
  type EditorPanelId,
} from './dock.js';

export type EditorCommand =
  | { readonly type: 'layout.reset' }
  | { readonly type: 'layout.toggle'; readonly panelId: EditorPanelId }
  | { readonly type: 'layout.focus'; readonly panelId: EditorPanelId }
  | { readonly type: 'engine.tick' };

export interface CommandReceipt {
  readonly type: EditorCommand['type'];
  readonly frame: number;
  readonly undoable: boolean;
}

/**
 * Editor commands. Layout is view state. `engine.tick` calls the engine.
 * It does not implement a private editor simulation.
 */
export class CommandBus {
  private readonly undoStack: DockLayout[] = [];
  private readonly redoStack: DockLayout[] = [];

  constructor(
    private readonly session: EditorSession,
    private dock: DockLayout,
  ) {}

  get layout(): DockLayout {
    return this.dock;
  }

  execute(command: EditorCommand): CommandReceipt {
    if (command.type === 'engine.tick') {
      const tick = this.session.engine.tick(1 / 60);
      return { type: command.type, frame: tick.frame, undoable: false };
    }
    const before = cloneDock(this.dock);
    this.applyLayout(command);
    if (!docksEqual(before, this.dock)) {
      this.undoStack.push(before);
      this.redoStack.length = 0;
    }
    return { type: command.type, frame: this.session.engine.clock.frame, undoable: true };
  }

  undo(): boolean {
    const previous = this.undoStack.pop();
    if (!previous) return false;
    this.redoStack.push(cloneDock(this.dock));
    this.dock = previous;
    return true;
  }

  redo(): boolean {
    const next = this.redoStack.pop();
    if (!next) return false;
    this.undoStack.push(cloneDock(this.dock));
    this.dock = next;
    return true;
  }

  private applyLayout(command: Exclude<EditorCommand, { type: 'engine.tick' }>): void {
    if (command.type === 'layout.reset') {
      this.dock = createDefaultDock();
      return;
    }
    const panel = panelById(this.dock, command.panelId);
    if (command.type === 'layout.focus') {
      if (!panel.visible) throw new Error(`cannot focus hidden panel '${panel.id}'`);
      this.dock.activePanelId = panel.id;
      return;
    }
    if (!panel.closable) throw new Error(`panel '${panel.id}' cannot be closed`);
    panel.visible = !panel.visible;
    if (!panel.visible && this.dock.activePanelId === panel.id) {
      this.dock.activePanelId = 'viewport';
    }
  }
}

export function parseEditorCommand(type: string, panelId: string | undefined): EditorCommand {
  if (type === 'engine.tick' || type === 'layout.reset') return { type };
  if ((type === 'layout.toggle' || type === 'layout.focus') && panelId !== undefined && isEditorPanelId(panelId)) {
    return { type, panelId };
  }
  throw new Error(`unknown editor command '${type}'`);
}
