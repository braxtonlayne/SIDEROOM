// The profile page editor (EXT-PRF-005): arrange blocks on the grid, set each block's content, choose a
// style, or start from an approved layout. Save checks everything on the server again and lists every
// problem it finds.
import { Button, Notice, SegmentedControl, Select, ThemeProvider } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import { useGuilds } from '../lounge/data';
import { listRelationships, REL } from '../lounge/shell/api';
import { listPackages, problemLines, type PackageView } from '../packages/api';
import { useLoad } from '../packages/useLoad';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { cdnUrl } from '../store/cdn';
import {
  adoptLayout,
  deleteMedia,
  getOwnDoc,
  getPageById,
  listMedia,
  saveDoc,
  uploadMedia,
} from './api';
import { Canvas, KIND_LABEL } from './Canvas';
import { addBlock } from './grid';
import { Inspector, type FriendChoice } from './Inspector';
import { ProfileRenderer } from './ProfileRenderer';
import { StylePanel } from './StylePanel';
import {
  BLOCK_KINDS,
  SINGLETONS,
  emptyDoc,
  type BlockKind,
  type ProfileDoc,
  type ProfileMedia,
  type ProfilePage,
} from './types';
import './profilepage.css';

/** The document as it should be saved: empty list entries dropped. */
export function cleanDoc(doc: ProfileDoc): ProfileDoc {
  return {
    ...doc,
    blocks: doc.blocks.map((b) => {
      if (b.type !== 'favorites') return b;
      const items = Array.isArray(b.config.items)
        ? (b.config.items as string[]).map((s) => s.trim()).filter(Boolean)
        : [];
      return { ...b, config: { ...b.config, items } };
    }),
  };
}

const same = (a: ProfileDoc, b: ProfileDoc) => JSON.stringify(a) === JSON.stringify(b);

export function ProfileEditor() {
  const t = useT();
  const theme = useAppliedTheme();
  const loaded = useLoad<ProfileDoc>('profile-doc', getOwnDoc);
  const page = useLoad<ProfilePage>('profile-page:me', () => getPageById('@me'));
  const friendsRes = useLoad<FriendChoice[]>('profile-friends', async () =>
    (await listRelationships())
      .filter((r) => r.type === REL.FRIEND)
      .map((r) => ({ id: r.user.id, name: r.user.global_name ?? r.user.username })),
  );
  const layouts = useLoad<PackageView[]>('profile-layouts', () =>
    listPackages('profile_layout', ''),
  );

  const [doc, setDoc] = useState<ProfileDoc>(emptyDoc());
  const [saved, setSaved] = useState<ProfileDoc>(emptyDoc());
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<'layout' | 'preview'>('layout');
  const [problems, setProblems] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [adding, setAdding] = useState<BlockKind>('header');
  const [layoutId, setLayoutId] = useState('');
  const [media, setMedia] = useState<ProfileMedia[]>([]);
  const [uploading, setUploading] = useState(false);

  useEffect(() => {
    if (loaded.data) {
      setDoc(loaded.data);
      setSaved(loaded.data);
    }
  }, [loaded.data]);

  useEffect(() => {
    let live = true;
    listMedia().then(
      (m) => live && setMedia(m),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);

  /** Upload a picture; the new one replaces the old background. A failure is shown among the problems. */
  const upload = async (kind: 'background' | 'gallery', file: File) => {
    setUploading(true);
    setProblems([]);
    try {
      const m = await uploadMedia(kind, file);
      setMedia((cur) => [...cur.filter((x) => kind === 'gallery' || x.kind !== 'background'), m]);
      return m;
    } catch (e) {
      setProblems(
        problemLines(e).length ? problemLines(e) : [t('lounge.profileEditor.uploadFailed')],
      );
      return null;
    } finally {
      setUploading(false);
    }
  };
  /** Delete a picture on the server (it also leaves the saved document there). */
  const dropMedia = (id: string) => {
    setMedia((cur) => cur.filter((x) => x.id !== id));
    deleteMedia(id).catch(() => setProblems([t('lounge.profileEditor.uploadFailed')]));
  };

  const myGuilds = useGuilds().map((g) => ({ id: g.id, name: g.name }));
  const dirty = !same(doc, saved);
  const block = doc.blocks.find((b) => b.id === selected) ?? null;
  const present = new Set(doc.blocks.map((b) => b.type));
  const addable = BLOCK_KINDS.filter((k) => !(SINGLETONS.includes(k) && present.has(k)));
  const addKind = addable.includes(adding) ? adding : (addable[0] ?? 'about');
  const bannerUrl = useMemo(
    () =>
      cdnUrl('avatars', page.data?.user.id ?? '', page.data?.user.banner ?? null, { size: 1024 }),
    [page.data?.user.id, page.data?.user.banner],
  );

  const add = () => {
    const next = addBlock(doc.blocks, addKind);
    if (!next) {
      setProblems([t('lounge.profileEditor.gridFull')]);
      return;
    }
    setProblems([]);
    setDoc({ ...doc, blocks: next });
    setSelected(next[next.length - 1]!.id);
  };
  const remove = (id: string) => {
    setDoc({ ...doc, blocks: doc.blocks.filter((b) => b.id !== id) });
    setSelected(null);
  };
  const save = async () => {
    setBusy(true);
    setProblems([]);
    setDone(false);
    try {
      const out = await saveDoc(cleanDoc(doc));
      setDoc(out);
      setSaved(out);
      setDone(true);
      page.reload();
    } catch (e) {
      setProblems(
        problemLines(e).length ? problemLines(e) : [t('lounge.profileEditor.saveFailed')],
      );
    } finally {
      setBusy(false);
    }
  };
  const applyLayout = async () => {
    if (!layoutId) return;
    setBusy(true);
    setProblems([]);
    try {
      const out = await adoptLayout(layoutId);
      setDoc(out);
      setSaved(out);
      setSelected(null);
      page.reload();
    } catch (e) {
      setProblems(
        problemLines(e).length ? problemLines(e) : [t('lounge.profileEditor.saveFailed')],
      );
    } finally {
      setBusy(false);
    }
  };

  const previewPage: ProfilePage | null = page.data ? { ...page.data, doc, media } : null;

  return (
    <ThemeProvider
      section="lounge"
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={theme.style}
      {...theme.data}
      className="pp-page"
    >
      <header className="pp-bar">
        <h1>{t('lounge.profileEditor.title')}</h1>
        <span className="pp-actions">
          <Link to="/channels" className="us-link">
            {t('lounge.profileEditor.back')}
          </Link>{' '}
          {page.data && (
            <Link
              to="/u/$username"
              params={{ username: page.data.user.username }}
              className="us-link"
            >
              {t('lounge.profileEditor.viewPage')}
            </Link>
          )}{' '}
          <Button
            variant="secondary"
            disabled={!dirty || busy}
            onClick={() => {
              setDoc(saved);
              setSelected(null);
            }}
          >
            {t('lounge.profileEditor.revert')}
          </Button>{' '}
          <Button variant="primary" loading={busy} disabled={!dirty} onClick={() => void save()}>
            {t('lounge.profileEditor.save')}
          </Button>
        </span>
      </header>

      <div className="pp-body">
        <div className="pp-editor">
          <main>
            {loaded.error && (
              <Notice variant="danger" title={t('lounge.profileEditor.loadFailed')}>
                {loaded.error}
              </Notice>
            )}
            {problems.length > 0 && (
              <Notice variant="danger" title={t('lounge.profileEditor.problems')}>
                <ul className="pkg-problems">
                  {problems.map((p, i) => (
                    <li key={i}>{p}</li>
                  ))}
                </ul>
              </Notice>
            )}
            {done && !dirty && (
              <Notice variant="success" title={t('lounge.profileEditor.saved')}>
                {t('lounge.profileEditor.savedBody')}
              </Notice>
            )}
            <SegmentedControl
              label={t('lounge.profileEditor.viewLabel')}
              value={view}
              onValueChange={(v) => setView(v as 'layout' | 'preview')}
              options={[
                { value: 'layout', label: t('lounge.profileEditor.viewLayout') },
                { value: 'preview', label: t('lounge.profileEditor.viewPreview') },
              ]}
            />
            <p className="us-muted">{t('lounge.profileEditor.help')}</p>
            {!loaded.data && !loaded.error ? (
              <p className="us-muted" role="status">
                {t('lounge.profileEditor.loading')}
              </p>
            ) : view === 'layout' ? (
              <Canvas
                blocks={doc.blocks}
                selected={selected}
                onSelect={setSelected}
                onChange={(blocks) => setDoc({ ...doc, blocks })}
                onRemove={remove}
              />
            ) : previewPage ? (
              <ProfileRenderer page={previewPage} doc={doc} bannerUrl={bannerUrl} media={media} />
            ) : (
              <p className="us-muted" role="status">
                {t('lounge.profileEditor.loading')}
              </p>
            )}
          </main>

          <aside className="pp-side" aria-label={t('lounge.profileEditor.sideLabel')}>
            <section className="us-card">
              <h2 className="us-card-title">{t('lounge.profileEditor.addBlock')}</h2>
              <div className="pp-fields">
                <Select
                  aria-label={t('lounge.profileEditor.blockType')}
                  options={addable.map((k) => ({ value: k, label: t(KIND_LABEL[k]) }))}
                  value={addKind}
                  onValueChange={(v) => setAdding(v as BlockKind)}
                />
                <Button
                  variant="secondary"
                  disabled={!loaded.data || doc.blocks.length >= 16}
                  onClick={add}
                >
                  {t('lounge.profileEditor.add')}
                </Button>
              </div>
            </section>

            {block && (
              <section className="us-card">
                <h2 className="us-card-title">
                  {t('lounge.profileEditor.blockSettings', { kind: t(KIND_LABEL[block.type]) })}
                </h2>
                <Inspector
                  block={block}
                  friends={friendsRes.data ?? []}
                  guilds={myGuilds}
                  media={media}
                  uploading={uploading}
                  onUploadGallery={(f) => upload('gallery', f)}
                  onRemoveMedia={dropMedia}
                  onConfig={(config) =>
                    setDoc({
                      ...doc,
                      blocks: doc.blocks.map((b) => (b.id === block.id ? { ...b, config } : b)),
                    })
                  }
                />
                <Button variant="danger" onClick={() => remove(block.id)}>
                  {t('lounge.profileEditor.removeBlock')}
                </Button>
              </section>
            )}

            <section className="us-card">
              <h2 className="us-card-title">{t('lounge.profileEditor.styleTitle')}</h2>
              <StylePanel
                style={doc.style}
                onChange={(style) => setDoc({ ...doc, style })}
                media={media}
                uploading={uploading}
                onUploadBackground={(f) => upload('background', f)}
                onRemoveMedia={dropMedia}
              />
            </section>

            <section className="us-card">
              <h2 className="us-card-title">{t('lounge.profileEditor.layoutsTitle')}</h2>
              {layouts.data && layouts.data.length > 0 ? (
                <div className="pp-fields">
                  <p className="us-muted">{t('lounge.profileEditor.layoutsHelp')}</p>
                  <Select
                    aria-label={t('lounge.profileEditor.layoutPick')}
                    options={[
                      { value: '', label: t('lounge.profileEditor.layoutNone') },
                      ...layouts.data.map((l) => ({
                        value: l.id,
                        label: `${l.name} (${l.author_name})`,
                      })),
                    ]}
                    value={layoutId}
                    onValueChange={setLayoutId}
                  />
                  <Button
                    variant="secondary"
                    disabled={!layoutId || busy}
                    onClick={() => void applyLayout()}
                  >
                    {t('lounge.profileEditor.layoutUse')}
                  </Button>
                </div>
              ) : (
                <p className="us-muted">{t('lounge.profileEditor.layoutsNone')}</p>
              )}
            </section>
          </aside>
        </div>
      </div>
    </ThemeProvider>
  );
}
