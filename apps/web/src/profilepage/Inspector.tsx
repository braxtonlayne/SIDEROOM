// Settings of the selected block (EXT-PRF-005). Each block type has a small typed config; the server checks the
// same limits again when the page is saved, the forms below only keep people inside them.
import { Checkbox, Select, Switch, TextArea, TextInput } from '@sideroom/ui';
import { useT } from '../i18n';
import type { Block } from './types';

export interface FriendChoice {
  id: string;
  name: string;
}

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
export function parseLinks(text: string): { label: string; url: string }[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .slice(0, 6)
    .map((l) => {
      const i = l.lastIndexOf('|');
      return i < 0
        ? { label: l.slice(0, 40), url: l }
        : { label: l.slice(0, i).trim().slice(0, 40), url: l.slice(i + 1).trim() };
    });
}
export const linksText = (items: { label: string; url: string }[]) =>
  items.map((l) => `${l.label} | ${l.url}`).join('\n');

export function Inspector({
  block,
  friends,
  onConfig,
}: {
  block: Block;
  friends: FriendChoice[];
  onConfig: (config: Cfg) => void;
}) {
  const t = useT();
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
    case 'gallery':
      return (
        <TextArea
          label={t('lounge.profileEditor.galleryItems')}
          hint={t('lounge.profileEditor.galleryHint')}
          value={galleryText(arr<{ url: string; caption?: string }>(c, 'items'))}
          rows={6}
          onChange={(e) => set({ items: parseGallery(e.target.value) })}
        />
      );
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
