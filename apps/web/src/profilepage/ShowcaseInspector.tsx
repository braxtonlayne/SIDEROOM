// The settings of a media showcase block (PRF-PRO-001, D-502): up to six audio or video files from the member's own
// shelves, chosen through the app's shelf picker (D-466's dialog, own shelves only), each with an optional caption
// of 80 characters. The server checks again on save that each one is on a shelf the member owns and is audio or video.
import { Button, TextInput } from '@sideroom/ui';
import { lazy, Suspense, useState } from 'react';
import { useT } from '../i18n';
import type { ShowcaseItem, ShowcaseView } from './types';

const ShelfPicker = lazy(() => import('../activities/ShelfPicker'));

/** Most items one showcase holds. */
export const MAX_SHOWCASE = 6;
const ACCEPT = ['audio/*', 'video/*'];

export function ShowcaseInspector({
  items,
  known = [],
  onItems,
}: {
  items: ShowcaseItem[];
  /** The showcase as the server last resolved it, for titles. */
  known?: ShowcaseView[];
  onItems: (items: ShowcaseItem[]) => void;
}) {
  const t = useT();
  const [picking, setPicking] = useState(false);
  const [names, setNames] = useState<Record<string, string>>({});
  const nameOf = (it: ShowcaseItem) =>
    names[it.object_hash] ??
    known.find((k) => k.object_hash === it.object_hash)?.title ??
    `${it.object_hash.slice(0, 8)}…`;
  const move = (i: number, by: -1 | 1) => {
    const next = [...items];
    const [x] = next.splice(i, 1);
    if (x) next.splice(i + by, 0, x);
    onItems(next);
  };

  return (
    <div className="pp-fields">
      <p className="pp-muted">{t('profile.showcase.editorHint')}</p>
      {items.length === 0 && <p className="pp-muted">{t('profile.showcase.editorEmpty')}</p>}
      {items.map((it, i) => (
        <div key={`${it.shelf_id}:${it.object_hash}`} className="pp-showcase-edit">
          <TextInput
            label={t('profile.showcase.caption', { name: nameOf(it) })}
            value={it.caption ?? ''}
            maxLength={80}
            onChange={(e) =>
              onItems(
                items.map((x, j) =>
                  j === i
                    ? e.target.value
                      ? { ...x, caption: e.target.value.replace(/\n/g, ' ') }
                      : { shelf_id: x.shelf_id, object_hash: x.object_hash }
                    : x,
                ),
              )
            }
          />
          <div className="pp-showcase-edit-actions">
            <Button
              size="sm"
              variant="ghost"
              disabled={i === 0}
              aria-label={t('profile.showcase.moveUp', { name: nameOf(it) })}
              onClick={() => move(i, -1)}
            >
              {t('profile.showcase.up')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={i === items.length - 1}
              aria-label={t('profile.showcase.moveDown', { name: nameOf(it) })}
              onClick={() => move(i, 1)}
            >
              {t('profile.showcase.down')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              aria-label={t('profile.showcase.removeItem', { name: nameOf(it) })}
              onClick={() => onItems(items.filter((_, j) => j !== i))}
            >
              {t('profile.showcase.remove')}
            </Button>
          </div>
        </div>
      ))}
      <Button
        variant="secondary"
        disabled={items.length >= MAX_SHOWCASE}
        onClick={() => setPicking(true)}
      >
        {t('profile.showcase.add')}
      </Button>
      {picking && (
        <Suspense fallback={null}>
          <ShelfPicker
            appName=""
            ownOnly
            title={t('profile.showcase.pickerTitle')}
            description={t('profile.showcase.pickerDescription')}
            req={{ accept: ACCEPT, maxSize: null }}
            onDone={(pick) => {
              setPicking(false);
              if (!pick) return;
              if (items.some((x) => x.object_hash === pick.object_hash)) return;
              setNames((n) => ({ ...n, [pick.object_hash]: pick.name }));
              onItems(
                [...items, { shelf_id: pick.shelf_id, object_hash: pick.object_hash }].slice(
                  0,
                  MAX_SHOWCASE,
                ),
              );
            }}
          />
        </Suspense>
      )}
    </div>
  );
}
