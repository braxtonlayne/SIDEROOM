// Server Settings > Extensions & Themes (EXT-PKG-006): approved declarative packages for this server. Recipes
// (a preset of feature module settings) are applied after the manager has seen what they change; sound,
// emoji and sticker packs can be browsed; server templates start a new server. Packages are data and were
// reviewed by staff (D-063, D-064); nothing here runs code.
import { Badge, Button, EmptyState, Modal, Notice, Select, TextInput } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import {
  applyRecipe,
  createGuildFromPackage,
  disableForGuild,
  guildEnabled,
  listPackages,
  versionFileUrl,
  type Enablement,
  type PackageKind,
  type PackageView,
} from '../../../packages/api';
import { useLoad } from '../../../packages/useLoad';
import '../../../packages/packages.css';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, SectionHeader, useAction } from '../ui';

const KINDS: { kind: PackageKind; label: MessageKey }[] = [
  { kind: 'recipe', label: 'settings.extensions.kindRecipe' },
  { kind: 'emoji_pack', label: 'settings.extensions.kindEmoji' },
  { kind: 'sticker_pack', label: 'settings.extensions.kindSticker' },
  { kind: 'sound_pack', label: 'settings.extensions.kindSound' },
  { kind: 'server_template', label: 'settings.extensions.kindTemplate' },
];

interface RecipeEntry {
  id: string;
  enabled: boolean | null;
  settings: Record<string, unknown> | null;
}

const recipeEntries = (p: PackageView): RecipeEntry[] => {
  const f = (p.payload as { features?: RecipeEntry[] } | null)?.features;
  return Array.isArray(f) ? f : [];
};
const packItems = (p: PackageView): { name: string; file: string }[] => {
  const i = (p.payload as { items?: { name: string; file: string }[] } | null)?.items;
  return Array.isArray(i) ? i : [];
};

function ApplyDialog({
  guildId,
  pkg,
  onClose,
  onDone,
}: {
  guildId: string;
  pkg: PackageView;
  onClose: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const action = useAction();
  const entries = recipeEntries(pkg);
  const go = async () => {
    const out = await action.run(() => applyRecipe(guildId, pkg.id));
    if (out) onDone();
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.extensions.applyTitle', { name: pkg.name })}
      description={t('settings.extensions.applyDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.extensions.cancel')}
          </Button>
          <Button variant="primary" loading={action.busy} onClick={() => void go()}>
            {t('settings.extensions.apply')}
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <ul className="pkg-list" aria-label={t('settings.extensions.changesLabel')}>
        {entries.map((e) => (
          <li key={e.id} className="pkg-row">
            <code>{e.id}</code>
            <span className="pkg-meta">
              {e.enabled === true && (
                <Badge variant="success">{t('settings.extensions.turnsOn')}</Badge>
              )}
              {e.enabled === false && <Badge>{t('settings.extensions.turnsOff')}</Badge>}
              {e.settings && (
                <span>
                  {t('settings.extensions.setsKeys', { keys: Object.keys(e.settings).join(', ') })}
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

function TemplateDialog({ pkg, onClose }: { pkg: PackageView; onClose: () => void }) {
  const t = useT();
  const action = useAction();
  const [name, setName] = useState(pkg.name);
  const navigate = useNavigate();
  const go = async () => {
    const out = await action.run(() => createGuildFromPackage(pkg.id, name.trim()));
    if (out) void navigate({ to: '/channels/$guildId', params: { guildId: out.id } });
  };
  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('settings.extensions.templateTitle', { name: pkg.name })}
      description={t('settings.extensions.templateDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.extensions.cancel')}
          </Button>
          <Button
            variant="primary"
            loading={action.busy}
            disabled={name.trim().length < 2}
            onClick={() => void go()}
          >
            {t('settings.extensions.createServer')}
          </Button>
        </>
      }
    >
      <ActionStatus error={action.error} />
      <TextInput
        label={t('settings.extensions.serverName')}
        value={name}
        maxLength={100}
        onChange={(e) => setName(e.target.value)}
      />
    </Modal>
  );
}

export function ExtensionsSection({ guildId }: SectionProps) {
  const t = useT();
  const [kind, setKind] = useState<PackageKind>('recipe');
  const [query, setQuery] = useState('');
  const [applying, setApplying] = useState<PackageView | null>(null);
  const [templating, setTemplating] = useState<PackageView | null>(null);
  const action = useAction();
  const on = useLoad<Enablement[]>(`ext-on:${guildId}`, () => guildEnabled(guildId));
  const list = useLoad<PackageView[]>(`ext:${kind}:${query}`, () => listPackages(kind, query));
  const appliedIds = new Set((on.data ?? []).map((e) => e.package_id));

  return (
    <>
      <SectionHeader
        title={t('settings.extensions.title')}
        description={t('settings.extensions.description')}
      />
      <ActionStatus error={action.error} done={action.done} />

      {(on.data ?? []).length > 0 && (
        <section aria-labelledby="ext-on-h">
          <h2 className="ss-subtitle" id="ext-on-h">
            {t('settings.extensions.turnedOn')}
          </h2>
          <ul className="pkg-list">
            {(on.data ?? []).map((e) => (
              <li key={e.package_id} className="pkg-row">
                <span>
                  <strong>{e.name}</strong> <span className="ss-sub">{e.version}</span>{' '}
                  {e.update_available && (
                    <Badge variant="accent">{t('settings.extensions.updateAvailable')}</Badge>
                  )}
                </span>
                <Button
                  size="sm"
                  variant="secondary"
                  loading={action.busy}
                  aria-label={t('settings.extensions.forget', { name: e.name })}
                  onClick={() =>
                    void action
                      .run(() => disableForGuild(guildId, e.package_id))
                      .then(() => on.reload())
                  }
                >
                  {t('settings.extensions.forgetShort')}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="pkg-toolbar">
        <TextInput
          label={t('settings.extensions.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Select
          aria-label={t('settings.extensions.kind')}
          options={KINDS.map((k) => ({ value: k.kind, label: t(k.label) }))}
          value={kind}
          onValueChange={(v) => setKind(v as PackageKind)}
        />
      </div>

      {kind !== 'recipe' && kind !== 'server_template' && (
        <Notice variant="info" title={t('settings.extensions.packsTitle')}>
          {t('settings.extensions.packsBody')}
        </Notice>
      )}

      {list.error ? (
        <Notice variant="danger" title={t('settings.extensions.loadFailed')}>
          {list.error}
        </Notice>
      ) : list.data && list.data.length > 0 ? (
        <ul className="pkg-grid" aria-label={t('settings.extensions.listLabel')}>
          {list.data.map((p) => (
            <li key={p.id} className="pkg-card">
              <h3>{p.name}</h3>
              <p className="pkg-meta">
                <span>{t('settings.extensions.by', { author: p.author_name })}</span>
                <span>{p.version}</span>
                {appliedIds.has(p.id) && (
                  <Badge variant="success">{t('settings.extensions.applied')}</Badge>
                )}
              </p>
              {p.description && <p>{p.description}</p>}
              {p.kind === 'recipe' && (
                <p className="pkg-meta">
                  {t('settings.extensions.touches', {
                    count: recipeEntries(p).length,
                    modules: recipeEntries(p)
                      .map((e) => e.id)
                      .join(', '),
                  })}
                </p>
              )}
              {p.kind !== 'recipe' && p.kind !== 'server_template' && (
                <ul
                  className="pkg-items"
                  aria-label={t('settings.extensions.itemsLabel', { name: p.name })}
                >
                  {packItems(p)
                    .slice(0, 12)
                    .map((i) =>
                      p.kind === 'sound_pack' ? (
                        <li key={i.name}>
                          <code>{i.name}</code>
                        </li>
                      ) : (
                        <li key={i.name} title={i.name}>
                          <img
                            src={versionFileUrl(p.version_id, i.file)}
                            alt={i.name}
                            loading="lazy"
                          />
                        </li>
                      ),
                    )}
                </ul>
              )}
              <div className="pkg-actions">
                {p.kind === 'recipe' && (
                  <Button size="sm" variant="primary" onClick={() => setApplying(p)}>
                    {appliedIds.has(p.id)
                      ? t('settings.extensions.applyAgain')
                      : t('settings.extensions.apply')}
                  </Button>
                )}
                {p.kind === 'server_template' && (
                  <Button size="sm" variant="primary" onClick={() => setTemplating(p)}>
                    {t('settings.extensions.createServer')}
                  </Button>
                )}
              </div>
            </li>
          ))}
        </ul>
      ) : (
        !list.loading && (
          <EmptyState
            title={t('settings.extensions.emptyTitle')}
            description={t('settings.extensions.emptyBody')}
          />
        )
      )}

      {applying && (
        <ApplyDialog
          guildId={guildId}
          pkg={applying}
          onClose={() => setApplying(null)}
          onDone={() => {
            setApplying(null);
            on.reload();
          }}
        />
      )}
      {templating && <TemplateDialog pkg={templating} onClose={() => setTemplating(null)} />}
    </>
  );
}
