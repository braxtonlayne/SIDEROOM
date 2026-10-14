// Variants (other cuts such as "Director's Cut") and trailers of a title (CIN-ADM-001).
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { useAdminViewer } from '../ui';
import * as api from './api';
import { Banner, Section, intOrNull, useRun } from './hooks';

interface Props {
  detail: api.AdminTitleDetail;
  reload: () => Promise<void>;
}

export function ExtrasPanel({ detail, reload }: Props) {
  return (
    <div className="adm-cin-extras">
      <VariantsBlock detail={detail} reload={reload} />
      <TrailersBlock detail={detail} reload={reload} />
    </div>
  );
}

function VariantsBlock({ detail, reload }: Props) {
  const t = useT();
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const { busy, error, notice, run } = useRun();
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const [remove, setRemove] = useState<api.Variant | null>(null);
  const items = [...detail.variants].sort((a, b) => a.position - b.position);
  return (
    <Section title={t('admin.extrasPanel.variants.title')}>
      <p className="ss-desc">{t('admin.extrasPanel.variants.intro')}</p>
      <Banner error={error} notice={notice} />
      {items.length === 0 && editing !== 'new' && (
        <EmptyState title={t('admin.extrasPanel.variants.empty')} />
      )}
      <ul className="ss-list" aria-label={t('admin.extrasPanel.variants.title')}>
        {items.map((v) =>
          editing === v.id ? (
            <li key={v.id} className="ss-row">
              <VariantForm
                initial={v}
                onCancel={() => setEditing(null)}
                onSave={async (b) => {
                  await api.updateVariant(v.id, b);
                  setEditing(null);
                  await reload();
                }}
              />
            </li>
          ) : (
            <li key={v.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {v.label}{' '}
                  <Badge variant={v.playable ? 'success' : 'warning'}>
                    {v.playable ? t('admin.extrasPanel.playable') : t('admin.extrasPanel.noVideo')}
                  </Badge>
                </div>
                <div className="ss-meta">
                  {[
                    v.runtime_min ? t('admin.extrasPanel.minutes', { n: v.runtime_min }) : null,
                    v.notes,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </div>
              </div>
              {canEdit && (
                <>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.extrasPanel.variants.editAria', { label: v.label })}
                    onClick={() => setEditing(v.id)}
                  >
                    {t('admin.extrasPanel.edit')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('admin.extrasPanel.variants.deleteAria', { label: v.label })}
                    onClick={() => setRemove(v)}
                  >
                    {t('admin.extrasPanel.delete')}
                  </Button>
                </>
              )}
            </li>
          ),
        )}
      </ul>
      {editing === 'new' && (
        <VariantForm
          onCancel={() => setEditing(null)}
          onSave={async (b) => {
            await api.createVariant(detail.title.id, { ...b, position: items.length });
            setEditing(null);
            await reload();
          }}
        />
      )}
      {canEdit && editing !== 'new' && (
        <Button variant="secondary" onClick={() => setEditing('new')}>
          {t('admin.extrasPanel.variants.add')}
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={t('admin.extrasPanel.variants.deleteTitle', { label: remove?.label ?? '' })}
        description={t('admin.extrasPanel.variants.deleteDescription')}
        confirmLabel={t('admin.extrasPanel.variants.deleteConfirm')}
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const v = remove;
          if (!v) return;
          void run(
            async () => {
              await api.deleteVariant(v.id);
              await reload();
              return true;
            },
            t('admin.extrasPanel.deleted', { label: v.label }),
          ).then(() => setRemove(null));
        }}
      />
    </Section>
  );
}

function VariantForm({
  initial,
  onSave,
  onCancel,
}: {
  initial?: api.Variant;
  onSave: (b: api.VariantInput) => Promise<void>;
  onCancel: () => void;
}) {
  const t = useT();
  const [label, setLabel] = useState(initial?.label ?? '');
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [runtime, setRuntime] = useState(initial?.runtime_min?.toString() ?? '');
  const [asset, setAsset] = useState(initial?.asset_id ?? '');
  const { busy, error, run } = useRun();
  return (
    <form
      className="adm-form"
      aria-label={
        initial
          ? t('admin.extrasPanel.variants.editAria', { label: initial.label })
          : t('admin.extrasPanel.variants.new')
      }
      onSubmit={(e) => {
        e.preventDefault();
        void run(() =>
          onSave({
            label: label.trim(),
            notes: notes.trim() || null,
            runtime_min: intOrNull(runtime),
            asset_id: asset.trim() || null,
            ...(initial ? { position: initial.position } : {}),
          }),
        );
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label={t('admin.extrasPanel.variants.label')}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <TextInput
          label={t('admin.extrasPanel.variants.notes')}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
        />
        <TextInput
          label={t('admin.extrasPanel.variants.runtime')}
          type="number"
          value={runtime}
          onChange={(e) => setRuntime(e.target.value)}
        />
        <TextInput
          label={t('admin.extrasPanel.variants.assetId')}
          value={asset}
          onChange={(e) => setAsset(e.target.value)}
        />
      </div>
      <Banner error={error} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!label.trim()}>
          {initial ? t('admin.extrasPanel.variants.save') : t('admin.extrasPanel.variants.add')}
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          {t('admin.extrasPanel.cancel')}
        </Button>
      </div>
    </form>
  );
}

function TrailersBlock({ detail, reload }: Props) {
  const t = useT();
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const { busy, error, notice, run } = useRun();
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const [remove, setRemove] = useState<api.Trailer | null>(null);
  return (
    <Section title={t('admin.extrasPanel.trailers.title')}>
      <Banner error={error} notice={notice} />
      {detail.trailers.length === 0 && editing !== 'new' && (
        <EmptyState title={t('admin.extrasPanel.trailers.empty')} />
      )}
      <ul className="ss-list" aria-label={t('admin.extrasPanel.trailers.title')}>
        {detail.trailers.map((tr) =>
          editing === tr.id ? (
            <li key={tr.id} className="ss-row">
              <TrailerForm
                initial={tr}
                onCancel={() => setEditing(null)}
                onSave={async (b) => {
                  await api.updateTrailer(tr.id, b);
                  setEditing(null);
                  await reload();
                }}
              />
            </li>
          ) : (
            <li key={tr.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {tr.label} <Badge>{tr.kind}</Badge>{' '}
                  <Badge variant={tr.playable ? 'success' : 'warning'}>
                    {tr.playable ? t('admin.extrasPanel.playable') : t('admin.extrasPanel.noVideo')}
                  </Badge>
                </div>
                <div className="ss-meta">
                  {tr.language ?? t('admin.extrasPanel.trailers.noLanguage')}
                </div>
              </div>
              {canEdit && (
                <>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('admin.extrasPanel.trailers.editAria', { label: tr.label })}
                    onClick={() => setEditing(tr.id)}
                  >
                    {t('admin.extrasPanel.edit')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('admin.extrasPanel.trailers.deleteAria', { label: tr.label })}
                    onClick={() => setRemove(tr)}
                  >
                    {t('admin.extrasPanel.delete')}
                  </Button>
                </>
              )}
            </li>
          ),
        )}
      </ul>
      {editing === 'new' && (
        <TrailerForm
          onCancel={() => setEditing(null)}
          onSave={async (b) => {
            await api.createTrailer(detail.title.id, b);
            setEditing(null);
            await reload();
          }}
        />
      )}
      {canEdit && editing !== 'new' && (
        <Button variant="secondary" onClick={() => setEditing('new')}>
          {t('admin.extrasPanel.trailers.add')}
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={t('admin.extrasPanel.trailers.deleteTitle', { label: remove?.label ?? '' })}
        confirmLabel={t('admin.extrasPanel.trailers.deleteConfirm')}
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const tr = remove;
          if (!tr) return;
          void run(
            async () => {
              await api.deleteTrailer(tr.id);
              await reload();
              return true;
            },
            t('admin.extrasPanel.deleted', { label: tr.label }),
          ).then(() => setRemove(null));
        }}
      />
    </Section>
  );
}

function TrailerForm({
  initial,
  onSave,
  onCancel,
}: {
  initial?: api.Trailer;
  onSave: (b: api.TrailerInput) => Promise<void>;
  onCancel: () => void;
}) {
  const t = useT();
  const trailerKinds = [
    { value: 'trailer', label: t('admin.extrasPanel.kind.trailer') },
    { value: 'teaser', label: t('admin.extrasPanel.kind.teaser') },
    { value: 'clip', label: t('admin.extrasPanel.kind.clip') },
  ];
  const [label, setLabel] = useState(initial?.label ?? '');
  const [kind, setKind] = useState<string>(initial?.kind ?? 'trailer');
  const [language, setLanguage] = useState(initial?.language ?? '');
  const [asset, setAsset] = useState(initial?.asset_id ?? '');
  const { busy, error, run } = useRun();
  return (
    <form
      className="adm-form"
      aria-label={
        initial
          ? t('admin.extrasPanel.trailers.editAria', { label: initial.label })
          : t('admin.extrasPanel.trailers.new')
      }
      onSubmit={(e) => {
        e.preventDefault();
        void run(() =>
          onSave({
            label: label.trim(),
            kind: kind as api.TrailerKind,
            language: language.trim() || null,
            asset_id: asset.trim() || null,
          }),
        );
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label={t('admin.extrasPanel.trailers.label')}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <Select
          aria-label={t('admin.extrasPanel.trailers.kind')}
          options={trailerKinds}
          value={kind}
          onValueChange={setKind}
        />
        <TextInput
          label={t('admin.extrasPanel.trailers.language')}
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
        />
        <TextInput
          label={t('admin.extrasPanel.trailers.assetId')}
          value={asset}
          onChange={(e) => setAsset(e.target.value)}
        />
      </div>
      <Banner error={error} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!label.trim()}>
          {initial ? t('admin.extrasPanel.trailers.save') : t('admin.extrasPanel.trailers.add')}
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          {t('admin.extrasPanel.cancel')}
        </Button>
      </div>
    </form>
  );
}
