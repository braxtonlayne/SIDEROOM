import { Button, Modal, TextArea, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { newNonce, sendInteraction } from './interactions';
import {
  startInteractionModals,
  useInteractionModal,
  type ActiveModal,
  type ModalComponent,
} from './modalStore';
import { useReportOutcome } from './useReportOutcome';
import './apps.css';

/** Text inputs and selects of a modal that carry a value, in order. */
function fields(m: ActiveModal): ModalComponent[] {
  return m.components.flatMap((r) =>
    (r.components ?? []).filter((c) => (c.type === 4 || c.type === 3) && c.custom_id),
  );
}

/** What a modal submission looks like on the wire; exported for tests. */
export function submissionData(m: ActiveModal, values: Record<string, string | string[]>) {
  return {
    custom_id: m.custom_id,
    components: m.components.map((row) => ({
      type: 1,
      components: (row.components ?? [])
        .filter((c) => (c.type === 4 || c.type === 3) && c.custom_id)
        .map((c) =>
          c.type === 3
            ? { type: 3, custom_id: c.custom_id, values: [values[c.custom_id!] ?? []].flat() }
            : { type: 4, custom_id: c.custom_id, value: String(values[c.custom_id!] ?? '') },
        ),
    })),
  };
}

/** Problems with the current values, by custom id. */
export function modalProblems(
  m: ActiveModal,
  values: Record<string, string | string[]>,
): Record<string, 'required' | 'short' | 'long'> {
  const out: Record<string, 'required' | 'short' | 'long'> = {};
  for (const c of fields(m)) {
    const v = values[c.custom_id!];
    const text = Array.isArray(v) ? v.join('') : (v ?? '');
    const required = c.required !== false;
    if (!text.trim()) {
      if (required) out[c.custom_id!] = 'required';
      continue;
    }
    const len = [...text].length;
    if (c.type === 4 && c.min_length !== undefined && len < c.min_length)
      out[c.custom_id!] = 'short';
    if (c.type === 4 && c.max_length !== undefined && len > c.max_length)
      out[c.custom_id!] = 'long';
  }
  return out;
}

/**
 * The form an app opens with a modal response (DEV-BOT-010): short and paragraph text inputs and
 * single-choice selects. Mounted once in the Lounge shell.
 */
export function InteractionModalHost() {
  useEffect(() => startInteractionModals(), []);
  const modal = useInteractionModal((s) => s.modal);
  const close = useInteractionModal((s) => s.close);
  if (!modal) return null;
  return <ModalBody key={modal.custom_id + (modal.nonce ?? '')} modal={modal} onClose={close} />;
}

function ModalBody({ modal, onClose }: { modal: ActiveModal; onClose: () => void }) {
  const t = useT();
  const report = useReportOutcome();
  const [values, setValues] = useState<Record<string, string | string[]>>(() => {
    const init: Record<string, string | string[]> = {};
    for (const c of fields(modal)) {
      init[c.custom_id!] =
        c.type === 3
          ? (c.options ?? []).filter((o) => o.default).map((o) => o.value)
          : (c.value ?? '');
    }
    return init;
  });
  const [busy, setBusy] = useState(false);
  const [touched, setTouched] = useState(false);
  const problems = modalProblems(modal, values);

  const submit = () => {
    setTouched(true);
    if (busy || Object.keys(problems).length > 0) return;
    setBusy(true);
    void sendInteraction({
      type: 5,
      application_id: modal.application_id,
      channel_id: modal.channel_id,
      ...(modal.guild_id ? { guild_id: modal.guild_id } : {}),
      nonce: newNonce(),
      data: submissionData(modal, values),
    }).then((o) => {
      setBusy(false);
      report(o);
      if (o.ok || o.reason !== 'rejected') onClose();
    });
  };

  const problemText = (id: string, c: ModalComponent) => {
    const p = touched ? problems[id] : undefined;
    if (p === 'required') return t('lounge.apps.form.errRequired');
    if (p === 'short') return t('lounge.apps.form.errMinLength', { min: c.min_length ?? 0 });
    if (p === 'long') return t('lounge.apps.form.errMaxLength', { max: c.max_length ?? 0 });
    return undefined;
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={modal.title}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.apps.modal.cancel')}
          </Button>
          <Button onClick={submit} loading={busy} disabled={busy}>
            {t('lounge.apps.modal.submit')}
          </Button>
        </>
      }
    >
      <form
        className="lng-appform"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        {fields(modal).map((c) => {
          const id = c.custom_id!;
          const err = problemText(id, c);
          const label = `${c.label ?? id}${c.required === false ? '' : ' *'}`;
          if (c.type === 3) {
            const current = [values[id] ?? []].flat();
            return (
              <label key={id} className="lng-appform-field">
                <span className="lng-appform-label">{label}</span>
                <select
                  className="lng-appform-select"
                  aria-invalid={err ? true : undefined}
                  value={current[0] ?? ''}
                  onChange={(e) =>
                    setValues((v) => ({ ...v, [id]: e.target.value ? [e.target.value] : [] }))
                  }
                >
                  <option value="">{c.placeholder ?? t('lounge.apps.form.pick')}</option>
                  {(c.options ?? []).slice(0, 25).map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
                {err && (
                  <span role="alert" className="lng-appform-error">
                    {err}
                  </span>
                )}
              </label>
            );
          }
          const common = {
            label,
            error: err,
            placeholder: c.placeholder,
            value: String(values[id] ?? ''),
            minLength: c.min_length,
            maxLength: c.max_length,
          };
          return (
            <div key={id} className="lng-appform-field">
              {c.style === 2 ? (
                <TextArea
                  {...common}
                  rows={4}
                  onChange={(e) => setValues((v) => ({ ...v, [id]: e.target.value }))}
                />
              ) : (
                <TextInput
                  {...common}
                  onChange={(e) => setValues((v) => ({ ...v, [id]: e.target.value }))}
                />
              )}
            </div>
          );
        })}
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </form>
    </Modal>
  );
}
