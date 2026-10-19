// Server Settings > Extensions & Themes (EXT-PKG-006): approved declarative packages for this server. Recipes
// (a preset of feature module settings) are applied after the manager has seen what they change; sound,
// emoji and sticker packs can be browsed; server templates start a new server. Packages are data and were
// reviewed by staff (D-063, D-064); nothing here runs code.
import {
  Badge,
  Button,
  ColorPicker,
  EmptyState,
  Modal,
  Notice,
  Select,
  TextInput,
} from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import {
  applyRecipe,
  createGuildFromPackage,
  disableForGuild,
  enableForGuild,
  guildAppearance,
  importPack,
  setGuildAccent,
  type PackImportResult,
  guildEnabled,
  listPackages,
  versionFileUrl,
  type Enablement,
  type PackageKind,
  type PackageView,
} from '../../../packages/api';
import { useLoad } from '../../../packages/useLoad';
import { accentOfHex, hexOfAccent } from '../../../packages/serverAppearance';
import '../../../packages/packages.css';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, SectionHeader, useAction } from '../ui';

const KINDS: { kind: PackageKind; label: MessageKey }[] = [
  { kind: 'recipe', label: 'settings.extensions.kindRecipe' },
  { kind: 'theme', label: 'settings.extensions.kindTheme' },
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

/** What an "Add to server" did: the items that were added and the ones that did not fit, with the reason. */
function ImportReport({ name, result }: { name: string; result: PackImportResult }) {
  const t = useT();
  return (
    <Notice
      variant={result.skipped.length > 0 ? 'warning' : 'success'}
      title={t('settings.extensions.importTitle', { name })}
    >
      <p>{t('settings.extensions.importAdded', { count: result.added.length })}</p>
      {result.skipped.length > 0 && (
        <ul aria-label={t('settings.extensions.importSkipped')}>
          {result.skipped.map((s) => (
            <li key={s.name}>
              <code>{s.name}</code> {s.reason}
            </li>
          ))}
        </ul>
      )}
    </Notice>
  );
}

/** The server accent colour and the theme every member sees in this server (LNG-SSET-050). */
function LookCard({ guildId, themeName }: { guildId: string; themeName: string | null }) {
  const t = useT();
  const action = useAction();
  const look = useLoad(`ext-look:${guildId}`, () => guildAppearance(guildId));
  const saved = hexOfAccent(look.data?.accent_color);
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? saved ?? '#c2b08f';
  const save = (accent: number | null) =>
    void action
      .run(() => setGuildAccent(guildId, accent))
      .then((out) => {
        if (out) {
          setDraft(null);
          look.reload();
        }
      });
  return (
    <section aria-labelledby="ext-look-h" className="pkg-look">
      <h2 className="ss-subtitle" id="ext-look-h">
        {t('settings.extensions.lookTitle')}
      </h2>
      <p className="ss-sub">{t('settings.extensions.lookBody')}</p>
      <p>
        <strong>{t('settings.extensions.themeLabel')}</strong>{' '}
        {themeName ?? t('settings.extensions.themeNone')}
      </p>
      <ColorPicker
        label={t('settings.extensions.accentLabel')}
        value={shown}
        onValueChange={setDraft}
      />
      <div className="pkg-actions">
        <Button
          size="sm"
          variant="primary"
          loading={action.busy}
          disabled={draft === null || accentOfHex(draft) === null}
          onClick={() => save(accentOfHex(draft ?? ''))}
        >
          {t('settings.extensions.accentSave')}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          loading={action.busy}
          disabled={saved === null}
          onClick={() => save(null)}
        >
          {t('settings.extensions.accentClear')}
        </Button>
      </div>
      <ActionStatus error={action.error} done={action.done} />
    </section>
  );
}

const PACK_KINDS: PackageKind[] = ['emoji_pack', 'sticker_pack', 'sound_pack'];

export function ExtensionsSection({ guildId, viewer }: SectionProps) {
  const t = useT();
  // Packs need MANAGE_GUILD_EXPRESSIONS alone; everything else needs MANAGE_GUILD (D-282).
  const manager = viewer.can('MANAGE_GUILD');
  const kinds = manager ? KINDS : KINDS.filter((k) => PACK_KINDS.includes(k.kind));
  const [kind, setKind] = useState<PackageKind>(manager ? 'recipe' : 'emoji_pack');
  const [query, setQuery] = useState('');
  const [applying, setApplying] = useState<PackageView | null>(null);
  const [templating, setTemplating] = useState<PackageView | null>(null);
  const [imported, setImported] = useState<{ name: string; result: PackImportResult } | null>(null);
  const action = useAction();
  const on = useLoad<Enablement[]>(`ext-on:${guildId}`, () => guildEnabled(guildId));
  const list = useLoad<PackageView[]>(`ext:${kind}:${query}`, () => listPackages(kind, query));
  const shown = (on.data ?? []).filter((e) => manager || PACK_KINDS.includes(e.kind));
  const appliedIds = new Set(shown.map((e) => e.package_id));
  const themeOn = shown.find((e) => e.kind === 'theme');
  const countOf = (id: string) => shown.find((e) => e.package_id === id)?.item_count ?? 0;

  return (
    <>
      <SectionHeader
        title={t('settings.extensions.title')}
        description={t('settings.extensions.description')}
      />
      <ActionStatus error={action.error} done={action.done} />

      {manager && <LookCard guildId={guildId} themeName={themeOn?.name ?? null} />}
      {imported && <ImportReport name={imported.name} result={imported.result} />}

      {shown.length > 0 && (
        <section aria-labelledby="ext-on-h">
          <h2 className="ss-subtitle" id="ext-on-h">
            {t('settings.extensions.turnedOn')}
          </h2>
          <ul className="pkg-list">
            {shown.map((e) => (
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
          options={kinds.map((k) => ({ value: k.kind, label: t(k.label) }))}
          value={kind}
          onValueChange={(v) => setKind(v as PackageKind)}
        />
      </div>

      {kind !== 'recipe' && kind !== 'server_template' && kind !== 'theme' && (
        <Notice variant="info" title={t('settings.extensions.packsAddTitle')}>
          {t('settings.extensions.packsAddBody')}
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
                {(p.kind === 'emoji_pack' ||
                  p.kind === 'sticker_pack' ||
                  p.kind === 'sound_pack') &&
                  (appliedIds.has(p.id) ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      loading={action.busy}
                      aria-label={t('settings.extensions.removePackCount', {
                        name: p.name,
                        count: countOf(p.id),
                      })}
                      onClick={() =>
                        void action
                          .run(() => disableForGuild(guildId, p.id))
                          .then(() => on.reload())
                      }
                    >
                      {t('settings.extensions.removeFromServerCount', { count: countOf(p.id) })}
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={action.busy}
                      aria-label={t('settings.extensions.addPack', { name: p.name })}
                      onClick={() =>
                        void action
                          .run(() => importPack(guildId, p.id))
                          .then((result) => {
                            if (result) setImported({ name: p.name, result });
                            on.reload();
                          })
                      }
                    >
                      {t('settings.extensions.addToServer')}
                    </Button>
                  ))}
                {p.kind === 'theme' &&
                  (appliedIds.has(p.id) ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      loading={action.busy}
                      onClick={() =>
                        void action
                          .run(() => disableForGuild(guildId, p.id))
                          .then(() => on.reload())
                      }
                    >
                      {t('settings.extensions.themeStop')}
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={action.busy}
                      aria-label={t('settings.extensions.themeUseNamed', { name: p.name })}
                      onClick={() =>
                        void action.run(() => enableForGuild(guildId, p.id)).then(() => on.reload())
                      }
                    >
                      {t('settings.extensions.themeUse')}
                    </Button>
                  ))}
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
