import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { Field, Problem, Root } from '../dev/parts';
import type { ToolProps } from '../registry';
import type { Note, NotesState, NotesStore } from './notesStore';
import { useNotes } from './useNotes';

export const MIN_PASSPHRASE = 10;
const SAVE_DELAY_MS = 800;

function download(name: string, text: string, type = 'application/json') {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

const titleOf = (n: Note) => n.text.split('\n', 1)[0]?.trim().slice(0, 60) || '';

/** The encrypted scratchpad: notes are encrypted in this browser; the server holds ciphertext it cannot read. */
export default function NotesTool(_props: ToolProps) {
  const t = useT();
  const { state, store } = useNotes();

  return (
    <Root label={t('tools.notes.name')}>
      <p className="ud-muted">{t('tools.notes.privacy')}</p>
      {state.problem ? <Problem>{t(`tools.notes.problem.${state.problem}`)}</Problem> : null}
      {!store || state.status === 'loading' ? (
        <p role="status" className="ud-muted">
          {t('tools.notes.loading')}
        </p>
      ) : state.status === 'error' ? (
        <button type="button" className="ud-btn" onClick={() => void store.init()}>
          {t('tools.notes.retry')}
        </button>
      ) : state.status === 'no-key' ? (
        <Setup state={state} store={store} />
      ) : (
        <Editor state={state} store={store} />
      )}
    </Root>
  );
}

function Setup({ state, store }: { state: NotesState; store: NotesStore }) {
  const t = useT();
  const [pass, setPass] = useState('');
  const [again, setAgain] = useState('');
  const [busy, setBusy] = useState(false);
  const [restorePass, setRestorePass] = useState('');
  const [confirmReset, setConfirmReset] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const hasRemote = state.remoteCount > 0;
  const weak = pass.length > 0 && pass.length < MIN_PASSPHRASE;
  const mismatch = again.length > 0 && again !== pass;

  return (
    <>
      {hasRemote ? (
        <p className="ud-warn" role="note">
          {t('tools.notes.otherBrowser', { n: state.remoteCount })}
        </p>
      ) : null}

      <section aria-labelledby="ud-notes-restore">
        <h4 id="ud-notes-restore">{t('tools.notes.restoreTitle')}</h4>
        <div className="ud-row">
          <Field label={t('tools.notes.recoveryFile')}>
            <input ref={file} type="file" accept=".json,application/json" />
          </Field>
          <Field label={t('tools.notes.passphrase')}>
            <input
              type="password"
              autoComplete="off"
              value={restorePass}
              onChange={(e) => setRestorePass(e.target.value)}
            />
          </Field>
          <button
            type="button"
            className="ud-btn"
            disabled={busy || restorePass === ''}
            onClick={() => {
              const f = file.current?.files?.[0];
              if (!f) return;
              setBusy(true);
              void f
                .text()
                .then((text) => store.restore(text, restorePass))
                .finally(() => setBusy(false));
            }}
          >
            {t('tools.notes.restore')}
          </button>
        </div>
      </section>

      <section aria-labelledby="ud-notes-new">
        <h4 id="ud-notes-new">{hasRemote ? t('tools.notes.startOverTitle') : t('tools.notes.setupTitle')}</h4>
        {hasRemote ? (
          <>
            <p className="ud-muted">{t('tools.notes.startOverBody')}</p>
            {confirmReset ? (
              <div className="ud-row">
                <button type="button" className="ud-btn" onClick={() => void store.startOver()}>
                  {t('tools.notes.startOverConfirm')}
                </button>
                <button type="button" className="ud-btn" onClick={() => setConfirmReset(false)}>
                  {t('tools.notes.cancel')}
                </button>
              </div>
            ) : (
              <button type="button" className="ud-btn" onClick={() => setConfirmReset(true)}>
                {t('tools.notes.startOver')}
              </button>
            )}
          </>
        ) : (
          <>
            <p className="ud-warn" role="note">
              {t('tools.notes.lostKey')}
            </p>
            <div className="ud-row">
              <Field label={t('tools.notes.passphrase')}>
                <input
                  type="password"
                  autoComplete="new-password"
                  value={pass}
                  aria-invalid={weak}
                  onChange={(e) => setPass(e.target.value)}
                />
              </Field>
              <Field label={t('tools.notes.passphraseAgain')}>
                <input
                  type="password"
                  autoComplete="new-password"
                  value={again}
                  aria-invalid={mismatch}
                  onChange={(e) => setAgain(e.target.value)}
                />
              </Field>
            </div>
            {weak ? <Problem>{t('tools.notes.passphraseShort', { n: MIN_PASSPHRASE })}</Problem> : null}
            {mismatch ? <Problem>{t('tools.notes.passphraseMismatch')}</Problem> : null}
            <button
              type="button"
              className="ud-btn ud-btn-primary"
              disabled={busy || pass.length < MIN_PASSPHRASE || pass !== again}
              onClick={() => {
                setBusy(true);
                void store
                  .setup(pass)
                  .then((bundle) => {
                    // Offer the recovery file at once: it is the only way into these notes from another browser.
                    if (bundle) download('sideroom-notes-recovery.json', store.exportFile() ?? '');
                  })
                  .finally(() => setBusy(false));
              }}
            >
              {busy ? t('tools.notes.creating') : t('tools.notes.create')}
            </button>
          </>
        )}
      </section>
    </>
  );
}

function Editor({ state, store }: { state: NotesState; store: NotesStore }) {
  const t = useT();
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [saved, setSaved] = useState(true);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const draftFor = useRef<string | null>(null);
  const [lockConfirm, setLockConfirm] = useState(false);

  const current = state.notes.find((n) => n.id === selected) ?? null;

  // Pick the newest note when nothing is selected or the selected one is gone.
  useEffect(() => {
    if (current === null && state.notes[0]) {
      setSelected(state.notes[0].id);
    }
  }, [current, state.notes]);

  // Load the text when the selection changes (not on every save of the same note).
  useEffect(() => {
    if (current && draftFor.current !== current.id) {
      draftFor.current = current.id;
      setDraft(current.text);
      setSaved(true);
    }
  }, [current]);

  const flush = (id: string, text: string) => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    void store.save(id, text).then(() => setSaved(true));
  };

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const change = (text: string) => {
    setDraft(text);
    setSaved(false);
    if (!current) return;
    if (timer.current) clearTimeout(timer.current);
    const id = current.id;
    timer.current = setTimeout(() => flush(id, text), SAVE_DELAY_MS);
  };

  const select = (id: string) => {
    if (current && !saved) flush(current.id, draft);
    draftFor.current = null;
    setSelected(id);
  };

  return (
    <>
      <div className="ud-row">
        <button
          type="button"
          className="ud-btn ud-btn-primary"
          onClick={() => {
            if (current && !saved) flush(current.id, draft);
            void store.create('').then((id) => {
              draftFor.current = null;
              if (id) setSelected(id);
            });
          }}
        >
          {t('tools.notes.newNote')}
        </button>
        <span className="ud-muted" role="status" aria-live="polite">
          {saved ? t('tools.notes.saved') : t('tools.notes.saving')}
        </span>
      </div>
      <div className="ud-notes-layout">
        <ul className="ud-notes-list" aria-label={t('tools.notes.list')}>
          {state.notes.length === 0 ? <li className="ud-muted">{t('tools.notes.empty')}</li> : null}
          {state.notes.map((n) => (
            <li key={n.id}>
              <button
                type="button"
                className="ud-btn ud-notes-item"
                aria-current={n.id === selected}
                onClick={() => select(n.id)}
              >
                {n.unreadable
                  ? t('tools.notes.unreadable')
                  : (n.id === selected ? titleOf({ ...n, text: draft }) : titleOf(n)) || t('tools.notes.untitled')}
              </button>
            </li>
          ))}
        </ul>
        <div>
          {current ? (
            current.unreadable ? (
              <p className="ud-muted">{t('tools.notes.unreadableBody')}</p>
            ) : (
              <Field label={t('tools.notes.text')}>
                <textarea
                  value={draft}
                  spellCheck
                  onChange={(e) => change(e.target.value)}
                  onBlur={() => {
                    if (!saved) flush(current.id, draft);
                  }}
                />
              </Field>
            )
          ) : null}
          {current ? (
            <div className="ud-row">
              <button type="button" className="ud-btn" onClick={() => void store.remove(current.id)}>
                {t('tools.notes.delete')}
              </button>
            </div>
          ) : null}
        </div>
      </div>
      <div className="ud-row">
        <button
          type="button"
          className="ud-btn"
          onClick={() => download('sideroom-notes-recovery.json', store.exportFile() ?? '')}
        >
          {t('tools.notes.downloadRecovery')}
        </button>
        <button
          type="button"
          className="ud-btn"
          disabled={state.notes.length === 0}
          onClick={() =>
            download(
              'sideroom-notes.txt',
              state.notes.filter((n) => !n.unreadable).map((n) => n.text).join('\n\n----\n\n'),
              'text/plain',
            )
          }
        >
          {t('tools.notes.downloadText')}
        </button>
        {lockConfirm ? (
          <>
            <button type="button" className="ud-btn" onClick={() => void store.lock()}>
              {t('tools.notes.lockConfirm')}
            </button>
            <button type="button" className="ud-btn" onClick={() => setLockConfirm(false)}>
              {t('tools.notes.cancel')}
            </button>
          </>
        ) : (
          <button type="button" className="ud-btn" onClick={() => setLockConfirm(true)}>
            {t('tools.notes.lock')}
          </button>
        )}
      </div>
      {lockConfirm ? <p className="ud-warn">{t('tools.notes.lockBody')}</p> : null}
      <p className="ud-muted">{t('tools.notes.lostKey')}</p>
    </>
  );
}
