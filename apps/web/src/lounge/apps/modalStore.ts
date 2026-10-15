/**
 * The modal an app asked this user to fill in (`INTERACTION_MODAL_CREATE`, DEV-BOT-010). One at a time:
 * a newer one replaces an older one, as on Discord.
 */
import { create } from 'zustand';
import { onDispatch } from '../../store/events';

/** A component inside a modal action row. Text inputs are type 4, string selects type 3. */
export interface ModalComponent {
  type: number;
  custom_id?: string;
  label?: string;
  style?: number;
  placeholder?: string;
  value?: string;
  required?: boolean;
  min_length?: number;
  max_length?: number;
  options?: { label: string; value: string; description?: string; default?: boolean }[];
  min_values?: number;
  max_values?: number;
}

export interface ModalRow {
  type: 1;
  components?: ModalComponent[];
}

export interface ActiveModal {
  /** The interaction nonce, only used to tell modals apart. */
  nonce: string | null;
  application_id: string;
  channel_id: string;
  guild_id: string | null;
  custom_id: string;
  title: string;
  components: ModalRow[];
}

interface ModalState {
  modal: ActiveModal | null;
  open: (m: ActiveModal) => void;
  close: () => void;
}

export const useInteractionModal = create<ModalState>((set) => ({
  modal: null,
  open: (modal) => set({ modal }),
  close: () => set({ modal: null }),
}));

/** Turn a gateway payload into a modal, or null when it is not usable. */
export function modalFromEvent(d: unknown): ActiveModal | null {
  const x = d as Partial<Record<string, unknown>> | null;
  if (!x || typeof x !== 'object') return null;
  const str = (v: unknown) => (typeof v === 'string' ? v : '');
  const customId = str(x.custom_id);
  const app = str(x.application_id);
  const channel = str(x.channel_id);
  if (!customId || !app || !channel || !Array.isArray(x.components)) return null;
  return {
    nonce: typeof x.nonce === 'string' ? x.nonce : null,
    application_id: app,
    channel_id: channel,
    guild_id: typeof x.guild_id === 'string' ? x.guild_id : null,
    custom_id: customId,
    title: str(x.title) || customId,
    components: (x.components as ModalRow[]).filter((r) => r && r.type === 1),
  };
}

let started = false;
/** Listen for modals for the lifetime of the page (idempotent). */
export function startInteractionModals(): void {
  if (started) return;
  started = true;
  onDispatch((event, data) => {
    if (event !== 'INTERACTION_MODAL_CREATE') return;
    const m = modalFromEvent(data);
    if (m) useInteractionModal.getState().open(m);
  });
}
