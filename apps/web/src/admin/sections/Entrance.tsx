// Entrance page editor (ADM-PLT-003, ENT-UI-025): the operator-edited markdown for /no-invitation, /house-notes
// and the landing note. Saving and resetting need a fresh second factor (through `withReauth` in the api module).
// The preview uses the same restricted renderer as the Entrance, so what staff see is what visitors get.
import { Button, ConfirmDialog, Notice, TextArea } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import { EntranceMarkdown, setEntranceCopy } from '../../routes/entrance/entranceCopy';
import { HeroBlock } from './EntranceHero';
import { ActionStatus, Async, ReasonDialog, SectionHeader, useAction, useResource } from '../ui';

/** The server's limit (16 KiB of UTF-8). */
export const COPY_MAX_BYTES = 16 * 1024;
export const utf8Bytes = (s: string) => new TextEncoder().encode(s).length;

const BLOCKS: { key: api.EntranceCopyKey; title: MessageKey; hint: MessageKey; path: string }[] = [
  {
    key: 'no-invitation',
    title: 'admin.entrance.noInvitationTitle',
    hint: 'admin.entrance.noInvitationHint',
    path: '/no-invitation',
  },
  {
    key: 'house-notes',
    title: 'admin.entrance.houseNotesTitle',
    hint: 'admin.entrance.houseNotesHint',
    path: '/house-notes',
  },
  {
    key: 'landing-note',
    title: 'admin.entrance.landingNoteTitle',
    hint: 'admin.entrance.landingNoteHint',
    path: '/',
  },
];

export function EntranceSection() {
  const t = useT();
  const res = useResource('admin:entrance-copy', api.getEntranceCopy);
  return (
    <>
      <SectionHeader
        title={t('admin.entrance.title')}
        description={t('admin.entrance.description')}
      />
      <Async res={res} label={t('admin.entrance.loading')}>
        {(copy) => (
          <div className="ss-form" style={{ maxWidth: 760 }}>
            <HeroBlock
              alt={copy['hero-alt']}
              onAltSaved={(next) => {
                setEntranceCopy({ ...copy, 'hero-alt': next });
                void res.reload();
              }}
            />
            {BLOCKS.map((b) => (
              <CopyBlock
                key={b.key}
                block={b}
                saved={copy[b.key]}
                onSaved={(next) => {
                  setEntranceCopy({ ...copy, [b.key]: next });
                  void res.reload();
                }}
              />
            ))}
          </div>
        )}
      </Async>
    </>
  );
}

function CopyBlock({
  block,
  saved,
  onSaved,
}: {
  block: (typeof BLOCKS)[number];
  saved: string | undefined;
  onSaved: (next: string | undefined) => void;
}) {
  const t = useT();
  const [text, setText] = useState(saved ?? '');
  const [preview, setPreview] = useState(false);
  const [saving, setSaving] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();
  useEffect(() => setText(saved ?? ''), [saved]);

  const bytes = utf8Bytes(text);
  const tooBig = bytes > COPY_MAX_BYTES;
  const dirty = text.trim() !== (saved ?? '').trim();
  const headingId = `ent-copy-${block.key}`;
  return (
    <section className="ss-card" aria-labelledby={headingId}>
      <h2 id={headingId}>{t(block.title)}</h2>
      <p className="ss-sub">{t(block.hint)}</p>
      <p className="ss-sub">
        {saved ? t('admin.entrance.customised') : t('admin.entrance.usingDefault')}
      </p>
      <TextArea
        label={t('admin.entrance.markdownLabel')}
        value={text}
        rows={10}
        error={tooBig ? t('admin.entrance.tooLarge', { max: COPY_MAX_BYTES / 1024 }) : undefined}
        hint={t('admin.entrance.bytes', { used: bytes, max: COPY_MAX_BYTES })}
        onChange={(e) => {
          setText(e.target.value);
          setNotice(null);
        }}
      />
      {preview && (
        <div
          className="ent-prose"
          role="region"
          aria-label={t('admin.entrance.previewAria')}
          style={{ padding: 16 }}
        >
          <EntranceMarkdown source={text} />
        </div>
      )}
      <div className="adm-actions">
        <Button variant="secondary" onClick={() => setPreview(!preview)} aria-pressed={preview}>
          {preview ? t('admin.entrance.hidePreview') : t('admin.entrance.showPreview')}
        </Button>
        <Button disabled={!dirty || tooBig || text.trim() === ''} onClick={() => setSaving(true)}>
          {t('admin.entrance.save')}
        </Button>
        <Button variant="ghost" disabled={!saved} onClick={() => setResetting(true)}>
          {t('admin.entrance.reset')}
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />

      <ReasonDialog
        open={saving}
        title={t('admin.entrance.saveTitle', { page: t(block.title) })}
        description={t('admin.entrance.saveDescription')}
        confirmLabel={t('admin.entrance.save')}
        reasonOptional
        reasonLabel={t('admin.entrance.reasonLabel')}
        onCancel={() => setSaving(false)}
        onConfirm={async (reason) => {
          const next = text.trim();
          await api.putEntranceCopy(block.key, next, reason);
          setSaving(false);
          setNotice(t('admin.entrance.saved'));
          onSaved(next);
        }}
      />
      <ConfirmDialog
        open={resetting}
        danger
        title={t('admin.entrance.resetTitle', { page: t(block.title) })}
        description={t('admin.entrance.resetDescription')}
        confirmLabel={t('admin.entrance.reset')}
        loading={action.busy}
        onCancel={() => setResetting(false)}
        onConfirm={() => {
          void action
            .run(async () => {
              await api.deleteEntranceCopy(block.key);
              return true;
            })
            .then((ok) => {
              setResetting(false);
              if (ok) {
                setNotice(t('admin.entrance.wasReset'));
                setText('');
                onSaved(undefined);
              }
            });
        }}
      />
    </section>
  );
}
