// Variants (other cuts such as "Director's Cut") and trailers of a title (CIN-ADM-001).
import { Badge, Button, ConfirmDialog, EmptyState, Select, TextInput } from '@sideroom/ui';
import { useState } from 'react';
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
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const { busy, error, notice, run } = useRun();
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const [remove, setRemove] = useState<api.Variant | null>(null);
  const items = [...detail.variants].sort((a, b) => a.position - b.position);
  return (
    <Section title="Variants">
      <p className="ss-desc">
        The title itself is the standard cut. Add the other cuts here; members pick one before
        playing.
      </p>
      <Banner error={error} notice={notice} />
      {items.length === 0 && editing !== 'new' && <EmptyState title="No variants" />}
      <ul className="ss-list" aria-label="Variants">
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
                    {v.playable ? 'playable' : 'no video'}
                  </Badge>
                </div>
                <div className="ss-meta">
                  {[v.runtime_min ? `${v.runtime_min} min` : null, v.notes]
                    .filter(Boolean)
                    .join(' · ')}
                </div>
              </div>
              {canEdit && (
                <>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={`Edit variant ${v.label}`}
                    onClick={() => setEditing(v.id)}
                  >
                    Edit
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Delete variant ${v.label}`}
                    onClick={() => setRemove(v)}
                  >
                    Delete
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
          Add variant
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={`Delete variant ${remove?.label ?? ''}`}
        description="Members can no longer pick this cut."
        confirmLabel="Delete variant"
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const v = remove;
          if (!v) return;
          void run(async () => {
            await api.deleteVariant(v.id);
            await reload();
            return true;
          }, `${v.label} deleted.`).then(() => setRemove(null));
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
  const [label, setLabel] = useState(initial?.label ?? '');
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [runtime, setRuntime] = useState(initial?.runtime_min?.toString() ?? '');
  const [asset, setAsset] = useState(initial?.asset_id ?? '');
  const { busy, error, run } = useRun();
  return (
    <form
      className="adm-form"
      aria-label={initial ? `Edit variant ${initial.label}` : 'New variant'}
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
        <TextInput label="Variant label" value={label} onChange={(e) => setLabel(e.target.value)} />
        <TextInput label="Variant notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
        <TextInput
          label="Variant runtime (minutes)"
          type="number"
          value={runtime}
          onChange={(e) => setRuntime(e.target.value)}
        />
        <TextInput
          label="Variant video asset id"
          value={asset}
          onChange={(e) => setAsset(e.target.value)}
        />
      </div>
      <Banner error={error} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!label.trim()}>
          {initial ? 'Save variant' : 'Add variant'}
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

const TRAILER_KINDS = ['trailer', 'teaser', 'clip'].map((k) => ({ value: k, label: k }));

function TrailersBlock({ detail, reload }: Props) {
  const canEdit = useAdminViewer().can('PUBLISH_MEDIA');
  const { busy, error, notice, run } = useRun();
  const [editing, setEditing] = useState<string | 'new' | null>(null);
  const [remove, setRemove] = useState<api.Trailer | null>(null);
  return (
    <Section title="Trailers">
      <Banner error={error} notice={notice} />
      {detail.trailers.length === 0 && editing !== 'new' && <EmptyState title="No trailers" />}
      <ul className="ss-list" aria-label="Trailers">
        {detail.trailers.map((t) =>
          editing === t.id ? (
            <li key={t.id} className="ss-row">
              <TrailerForm
                initial={t}
                onCancel={() => setEditing(null)}
                onSave={async (b) => {
                  await api.updateTrailer(t.id, b);
                  setEditing(null);
                  await reload();
                }}
              />
            </li>
          ) : (
            <li key={t.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  {t.label} <Badge>{t.kind}</Badge>{' '}
                  <Badge variant={t.playable ? 'success' : 'warning'}>
                    {t.playable ? 'playable' : 'no video'}
                  </Badge>
                </div>
                <div className="ss-meta">{t.language ?? 'No language set'}</div>
              </div>
              {canEdit && (
                <>
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={`Edit trailer ${t.label}`}
                    onClick={() => setEditing(t.id)}
                  >
                    Edit
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Delete trailer ${t.label}`}
                    onClick={() => setRemove(t)}
                  >
                    Delete
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
          Add trailer
        </Button>
      )}
      <ConfirmDialog
        open={!!remove}
        danger
        title={`Delete trailer ${remove?.label ?? ''}`}
        confirmLabel="Delete trailer"
        loading={busy}
        onCancel={() => setRemove(null)}
        onConfirm={() => {
          const t = remove;
          if (!t) return;
          void run(async () => {
            await api.deleteTrailer(t.id);
            await reload();
            return true;
          }, `${t.label} deleted.`).then(() => setRemove(null));
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
  const [label, setLabel] = useState(initial?.label ?? '');
  const [kind, setKind] = useState<string>(initial?.kind ?? 'trailer');
  const [language, setLanguage] = useState(initial?.language ?? '');
  const [asset, setAsset] = useState(initial?.asset_id ?? '');
  const { busy, error, run } = useRun();
  return (
    <form
      className="adm-form"
      aria-label={initial ? `Edit trailer ${initial.label}` : 'New trailer'}
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
        <TextInput label="Trailer label" value={label} onChange={(e) => setLabel(e.target.value)} />
        <Select
          aria-label="Trailer kind"
          options={TRAILER_KINDS}
          value={kind}
          onValueChange={setKind}
        />
        <TextInput
          label="Trailer language"
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
        />
        <TextInput
          label="Trailer video asset id"
          value={asset}
          onChange={(e) => setAsset(e.target.value)}
        />
      </div>
      <Banner error={error} />
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!label.trim()}>
          {initial ? 'Save trailer' : 'Add trailer'}
        </Button>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
