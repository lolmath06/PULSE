import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from '@/services/tauri';

interface DialogWindow {
  isAlwaysOnTop(): Promise<boolean>;
  setAlwaysOnTop(value: boolean): Promise<void>;
  setFocus(): Promise<void>;
}

/** One native window lease shared by nested/replaced DOM dialogs. */
export class DialogWindowPriority {
  private users = 0;
  private previous: boolean | undefined;
  private pending = Promise.resolve();

  constructor(private readonly window: DialogWindow) {}

  retain(): () => void {
    this.users += 1;
    this.sync();
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.users -= 1;
      this.sync();
    };
  }

  /** Also lets tests wait for the real asynchronous native-window sequence. */
  settled(): Promise<void> {
    return this.pending;
  }

  private sync() {
    this.pending = this.pending
      .then(async () => {
        if (this.users > 0 && this.previous === undefined) {
          const previous = await this.window.isAlwaysOnTop();
          // A dialog can unmount while the native getter is still pending.
          if (this.users === 0) return;
          this.previous = previous;
          await this.window.setAlwaysOnTop(true);
          // Both main and overlays now occupy the native "above" layer;
          // focusing main raises it above PULSE's existing overlay windows.
          if (this.users > 0) await this.window.setFocus();
        } else if (this.users === 0 && this.previous !== undefined) {
          await this.window.setAlwaysOnTop(this.previous);
          this.previous = undefined;
        }
      })
      .catch((error: unknown) => console.error('PULSE: dialog window priority failed', error));
  }
}

let priority: DialogWindowPriority | undefined;

/** DOM z-index cannot cross native overlay windows. Restore priority on unmount. */
export function useDialogWindow() {
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const window = getCurrentWindow();
    if (window.label !== 'main') return;
    priority ??= new DialogWindowPriority(window);
    return priority.retain();
  }, []);
}
