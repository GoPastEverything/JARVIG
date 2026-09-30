import { CommandBus, type CommandReceipt, type EditorCommand } from './commands.js';
import { createDefaultDock, type DockLayout } from './dock.js';
import type { HeadlessNativeRuntime } from './native-runtime.js';
import { createEditorSession, type EditorSession } from './session.js';

export interface ViewportPresentation {
  readonly bound: true;
  readonly cameraEntityId: string;
  readonly backend: string;
  readonly profile: string;
  readonly frame: number;
  readonly modules: readonly string[];
  /** Frame counter from the native runtime, when the host attached one. Not a GPU frame. */
  readonly nativeFrame: number | null;
  readonly nativeRenderExecuted: boolean | null;
}

/** Editor host shell. Panels call the engine. They do not own a renderer or an importer. */
export class EditorShell {
  readonly session: EditorSession;
  readonly commands: CommandBus;
  private readonly native: HeadlessNativeRuntime | undefined;
  private nativeFrame: number | null;
  private nativeRenderExecuted: boolean | null;

  private constructor(
    session: EditorSession,
    commands: CommandBus,
    native: HeadlessNativeRuntime | undefined,
    nativeFrame: number | null,
    nativeRenderExecuted: boolean | null,
  ) {
    this.session = session;
    this.commands = commands;
    this.native = native;
    this.nativeFrame = nativeFrame;
    this.nativeRenderExecuted = nativeRenderExecuted;
  }

  static create(native?: HeadlessNativeRuntime): EditorShell {
    const session = createEditorSession();
    try {
      const first = native?.tick(1 / 60);
      return new EditorShell(
        session,
        new CommandBus(session, createDefaultDock()),
        native,
        first?.frame ?? null,
        first?.renderExecuted ?? null,
      );
    } catch (error) {
      session.shutdown();
      throw error;
    }
  }

  get dock(): DockLayout {
    return this.commands.layout;
  }

  execute(command: EditorCommand): CommandReceipt {
    const receipt = this.commands.execute(command);
    if (command.type === 'engine.tick' && this.native) {
      const tick = this.native.tick(1 / 60);
      this.nativeFrame = tick.frame;
      this.nativeRenderExecuted = tick.renderExecuted;
    }
    return receipt;
  }

  /** Read-only view of the engine camera. Pixel presentation is a later engine renderer call. */
  viewport(): ViewportPresentation {
    return {
      bound: true,
      cameraEntityId: this.session.viewport.cameraEntityId,
      backend: this.session.viewport.backend,
      profile: this.session.engine.profile,
      frame: this.session.engine.clock.frame,
      modules: this.session.engine.moduleList(),
      nativeFrame: this.nativeFrame,
      nativeRenderExecuted: this.nativeRenderExecuted,
    };
  }

  shutdown(): void {
    this.native?.shutdown();
    this.session.shutdown();
  }
}
