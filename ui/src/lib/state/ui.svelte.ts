// Transient UI state: which popover is open, toasts.

export type Popover = 'profile' | 'wallet' | null;

interface Toast {
  id: number;
  text: string;
}

class UiState {
  popover = $state<Popover>(null);
  /** Modal dialogs (lazy-loaded). */
  dialog = $state<'import' | null>(null);
  toasts = $state<Toast[]>([]);
  private next = 1;

  toggle(p: Exclude<Popover, null>) {
    this.popover = this.popover === p ? null : p;
  }

  close() {
    this.popover = null;
  }

  toast(text: string) {
    const id = this.next++;
    this.toasts = [...this.toasts, { id, text }];
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), 3200);
  }
}

export const ui = new UiState();

/** Shown for actions whose backend lands in a later phase. */
export function comingSoon(what: string, phase: number) {
  ui.toast(`${what} arrives in Phase ${phase}. This build runs on sample data.`);
}
