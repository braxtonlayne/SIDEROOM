// Settings of the selected block (EXT-PRF-005). Each block type has a small typed config; the server checks the
// same limits again when the page is saved, the forms below only keep people inside them.
import { Button, Checkbox, Select, Switch, TextArea, TextInput } from '@sideroom/ui';
import { useRef } from 'react';
import { useT } from '../i18n';
import type { Block, GalleryItem, ProfileMedia } from './types';

/** Most pictures one gallery holds. */
export const MAX_GALLERY = 12;

export interface FriendChoice {
  id: string;
  name: string;
}

/** A server the member belongs to, for the Featured servers block. */
export interface GuildChoice {
  id: string;
  name: string;
}

/** Most servers one Featured servers block names, and most links one Links block holds. */
export const MAX_FEATURED = 6;
export const MAX_LINKTREE = 12;

type Cfg = Record<string, unknown>;
const str = (c: Cfg, k: string) => (typeof c[k] === 'string' ? (c[k] as string) : '');
const arr = <T,>(c: Cfg, k: string): T[] => (Array.isArray(c[k]) ? (c[k] as T[]) : []);

/** `url caption` per line to gallery items; lines that are empty are dropped. */
export function parseGallery(text: string): { url: string; caption?: string }[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .slice(0, 12)
    .map((l) => {
      const i = l.search(/\s/);
      if (i < 0) return { url: l };
      const caption = l.slice(i).trim();
      return caption
        ? { url: l.slice(0, i), caption: caption.slice(0, 80) }
        : { url: l.slice(0, i) };
    });
}
export const galleryText = (items: { url: string; caption?: string }[]) =>
  items.map((i) => (i.caption ? `${i.url} ${i.caption}` : i.url)).join('\n');

/** `label | url` per line to links. */
export function parseLinks(text: string, max = 6): { label: string; url: string }[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .slice(0, max)
    .map((l) => {
      const i = l.lastIndexOf('|');
      return i < 0
        ? { label: l.slice(0, 40), url: l }
        : { label: l.slice(0, i).trim().slice(0, 40), url: l.slice(i + 1).trim() };
    });
}
export const linksText = (items: { label: string; url: string }[]) =>
  items.map((l) => `${l.label} | ${l.url}`).join('\n');

/** `label | url` per line to links, for the Links block (up to twelve). */
export const parseLinktree = (text: string) => parseLinks(text, MAX_LINKTREE);

export function Inspector({
  block,
  friends,
  guilds = [],
  onConfig,
  media = [],
  onUploadGallery,
  onRemoveMedia,
  uploading = false,
}: {
  block: Block;
  friends: FriendChoice[];
  guilds?: GuildChoice[];
  onConfig: (config: Cfg) => void;
  /** The person's uploaded pictures (K-17). */
  media?: ProfileMedia[];
  /** Upload a gallery picture; resolves to the stored picture, or null when it failed. */
  onUploadGallery?: (file: File) => Promise<ProfileMedia | null>;
  /** Delete an uploaded picture (and take it out of the saved document on the server). */
  onRemoveMedia?: (id: string) => void;
  uploading?: boolean;
}) {
  const t = useT();
  const fileInput = useRef<HTMLInputElement>(null);
  const c = block.config;
  const set = (patch: Cfg) => onConfig({ ...c, ...patch });
  const textOrDrop = (k: string, v: string) => {
    const next = { ...c, [k]: v };
    if (!v) delete next[k];
    onConfig(next);
  };

  switch (block.type) {
    case 'header':
      return (
        <div className="pp-fields">
          <TextInput
            label={t('lounge.profileEditor.tagline')}
            value={str(c, 'tagline')}
            maxLength={80}
            onChange={(e) => textOrDrop('tagline', e.target.value)}
          />
          <Switch
            label={t('lounge.profileEditor.showBanner')}
            checked={c.show_banner !== false}
            onChange={(e) => set({ show_banner: e.target.checked })}
          />
        </div>
      );
    case 'about':
      return (
        <TextArea
          label={t('lounge.profileEditor.aboutText')}
          value={str(c, 'text')}
          maxLength={1500}
          rows={6}
          onChange={(e) => set({ text: e.target.value })}
        />
      );
    case 'top_friends': {
      const chosen = arr<string>(c, 'user_ids');
      return (
        <fieldset className="pp-fields">
          <legend>{t('lounge.profileEditor.friendsLegend')}</legend>
          {friends.length === 0 && <p>{t('lounge.profileEditor.noFriends')}</p>}
          {friends.map((f) => (
            <Checkbox
              key={f.id}
              label={f.name}
              checked={chosen.includes(f.id)}
              disabled={!chosen.includes(f.id) && chosen.length >= 8}
              onChange={(e) =>
                set({
                  user_ids: e.target.checked ? [...chosen, f.id] : chosen.filter((x) => x !== f.id),
                })
              }
            />
          ))}
        </fieldset>
      );
    }
    case 'now_watching': {
      const manual = str(c, 'mode') === 'manual';
      return (
        <div className="pp-fields">
          <Select
            aria-label={t('lounge.profileEditor.nowMode')}
            options={[
              { value: 'auto', label: t('lounge.profileEditor.nowAuto') },
              { value: 'manual', label: t('lounge.profileEditor.nowManual') },
            ]}
            value={manual ? 'manual' : 'auto'}
            onValueChange={(v) => set({ mode: v })}
          />
          {manual && (
            <>
              <TextInput
                label={t('lounge.profileEditor.nowTitle')}
                value={str(c, 'title')}
                maxLength={80}
                onChange={(e) => textOrDrop('title', e.target.value)}
              />
              <TextInput
                label={t('lounge.profileEditor.nowSubtitle')}
                value={str(c, 'subtitle')}
                maxLength={80}
                onChange={(e) => textOrDrop('subtitle', e.target.value)}
              />
            </>
          )}
        </div>
      );
    }
    case 'linktree':
      return (
        <div className="pp-fields">
          <TextInput
            label={t('profile.editor.linksTitle')}
            value={str(c, 'title')}
            maxLength={40}
            onChange={(e) => textOrDrop('title', e.target.value)}
          />
          <TextArea
            label={t('profile.editor.linksList')}
            hint={t('profile.editor.linksHint')}
            value={linksText(arr<{ label: string; url: string }>(c, 'links'))}
            rows={5}
            onChange={(e) => set({ links: parseLinktree(e.target.value) })}
          />
        </div>
      );
    case 'featured_guilds': {
      const chosen = arr<string>(c, 'guild_ids');
      return (
        <fieldset className="pp-fields">
          <legend>{t('profile.editor.serversLegend')}</legend>
          <p>{t('profile.editor.serversHint')}</p>
          {guilds.length === 0 && <p>{t('profile.editor.noServers')}</p>}
          {guilds.map((g) => (
            <Checkbox
              key={g.id}
              label={g.name}
              checked={chosen.includes(g.id)}
              disabled={!chosen.includes(g.id) && chosen.length >= MAX_FEATURED}
              onChange={(e) =>
                set({
                  guild_ids: e.target.checked
                    ? [...chosen, g.id]
                    : chosen.filter((x) => x !== g.id),
                })
              }
            />
          ))}
        </fieldset>
      );
    }
    case 'now_playing': {
      const manual = str(c, 'mode') === 'manual';
      return (
        <div className="pp-fields">
          <Select
            aria-label={t('lounge.profileEditor.nowMode')}
            options={[
              { value: 'auto', label: t('profile.editor.playingAuto') },
              { value: 'manual', label: t('lounge.profileEditor.nowManual') },
            ]}
            value={manual ? 'manual' : 'auto'}
            onValueChange={(v) => set({ mode: v })}
          />
          {!manual && <p>{t('profile.editor.playingAutoHint')}</p>}
          {manual && (
            <>
              <TextInput
                label={t('lounge.profileEditor.nowTitle')}
                value={str(c, 'title')}
                maxLength={80}
                onChange={(e) => textOrDrop('title', e.target.value)}
              />
              <TextInput
                label={t('lounge.profileEditor.nowSubtitle')}
                value={str(c, 'subtitle')}
                maxLength={80}
                onChange={(e) => textOrDrop('subtitle', e.target.value)}
              />
            </>
          )}
        </div>
      );
    }
    case 'favorites':
      return (
        <div className="pp-fields">
          <TextInput
            label={t('lounge.profileEditor.favoritesTitle')}
            value={str(c, 'title')}
            maxLength={40}
            onChange={(e) => textOrDrop('title', e.target.value)}
          />
          <TextArea
            label={t('lounge.profileEditor.favoritesItems')}
            hint={t('lounge.profileEditor.favoritesHint')}
            value={arr<string>(c, 'items').join('\n')}
            rows={6}
            onChange={(e) =>
              set({
                items: e.target.value
                  .split('\n')
                  .map((l) => l.slice(0, 60))
                  .slice(0, 10),
              })
            }
          />
        </div>
      );
    case 'gallery': {
      const all = arr<GalleryItem>(c, 'items');
      const urls = all.filter((i) => i.url);
      const mine = all.filter((i) => i.image_id);
      const known = new Map(media.map((m) => [m.id, m]));
      return (
        <div className="pp-fields">
          <TextArea
            label={t('lounge.profileEditor.galleryItems')}
            hint={t('lounge.profileEditor.galleryHint')}
            value={galleryText(urls as { url: string; caption?: string }[])}
            rows={6}
            onChange={(e) =>
              set({ items: [...parseGallery(e.target.value), ...mine].slice(0, MAX_GALLERY) })
            }
          />
          {onUploadGallery && (
            <>
              <input
                ref={fileInput}
                type="file"
                hidden
                accept="image/png,image/jpeg,image/gif,image/webp"
                aria-label={t('lounge.profileEditor.galleryUploadFile')}
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  e.target.value = '';
                  if (!f) return;
                  void onUploadGallery(f).then((m) => {
                    if (m)
                      set({ items: [...urls, ...mine, { image_id: m.id }].slice(0, MAX_GALLERY) });
                  });
                }}
              />
              <Button
                variant="secondary"
                loading={uploading}
                disabled={all.length >= MAX_GALLERY}
                onClick={() => fileInput.current?.click()}
              >
                {t('lounge.profileEditor.galleryUpload')}
              </Button>
            </>
          )}
          {mine.map((it) => (
            <div key={it.image_id} className="pp-media-row">
              <TextInput
                label={t('lounge.profileEditor.galleryCaption', {
                  name: known.get(it.image_id ?? '') ? it.image_id : (it.image_id ?? ''),
                })}
                value={it.caption ?? ''}
                maxLength={80}
                onChange={(e) =>
                  set({
                    items: all.map((x) =>
                      x.image_id === it.image_id
                        ? e.target.value
                          ? { ...x, caption: e.target.value }
                          : { image_id: x.image_id }
                        : x,
                    ),
                  })
                }
              />
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  onRemoveMedia?.(it.image_id ?? '');
                  set({ items: all.filter((x) => x.image_id !== it.image_id) });
                }}
              >
                {t('lounge.profileEditor.galleryRemove')}
              </Button>
            </div>
          ))}
        </div>
      );
    }
    case 'guestbook':
      return (
        <TextInput
          label={t('lounge.profileEditor.guestbookTitle')}
          value={str(c, 'title')}
          maxLength={40}
          onChange={(e) => textOrDrop('title', e.target.value)}
        />
      );
    case 'song':
      return (
        <div className="pp-fields">
          <TextInput
            label={t('lounge.profileEditor.songTitle')}
            value={str(c, 'title')}
            maxLength={80}
            onChange={(e) => set({ title: e.target.value })}
          />
          <TextInput
            label={t('lounge.profileEditor.songArtist')}
            value={str(c, 'artist')}
            maxLength={80}
            onChange={(e) => textOrDrop('artist', e.target.value)}
          />
          <TextInput
            label={t('lounge.profileEditor.songUrl')}
            hint={t('lounge.profileEditor.songHint')}
            value={str(c, 'url')}
            maxLength={512}
            onChange={(e) => textOrDrop('url', e.target.value)}
          />
        </div>
      );
    case 'custom':
      return (
        <div className="pp-fields">
          <TextInput
            label={t('lounge.profileEditor.customTitle')}
            value={str(c, 'title')}
            maxLength={40}
            onChange={(e) => textOrDrop('title', e.target.value)}
          />
          <TextArea
            label={t('lounge.profileEditor.customText')}
            value={str(c, 'text')}
            maxLength={1000}
            rows={5}
            onChange={(e) => set({ text: e.target.value })}
          />
          <TextArea
            label={t('lounge.profileEditor.customLinks')}
            hint={t('lounge.profileEditor.customLinksHint')}
            value={linksText(arr<{ label: string; url: string }>(c, 'links'))}
            rows={3}
            onChange={(e) => set({ links: parseLinks(e.target.value) })}
          />
        </div>
      );
    default:
      return null;
  }
}
