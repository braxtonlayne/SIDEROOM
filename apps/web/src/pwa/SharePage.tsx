import { Button, Notice } from '@sideroom/ui';
import { useLocation, useNavigate } from '@tanstack/react-router';
import { useEffect, useMemo, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { HOME } from '../lounge/channels';
import { drafts } from '../lounge/drafts';
import * as rest from '../store/rest';
import { parseShare, shareToDraft } from './shareText';
import './pwa.css';

interface Target {
  id: string;
  label: string;
  guildId: string;
}
interface Group {
  id: string;
  name: string;
  targets: Target[];
}

const TEXT_TYPES = new Set([0, 5]);

async function loadGroups(t: (k: 'pwa.share.dms') => string): Promise<Group[]> {
  const guilds = await rest.fetchGuilds();
  const groups: Group[] = await Promise.all(
    guilds.map(async (g) => {
      const channels = await rest.fetchChannels(g.id).catch(() => []);
      return {
        id: g.id,
        name: g.name,
        targets: channels
          .filter((c) => TEXT_TYPES.has(c.type))
          .sort((a, b) => (a.position ?? 0) - (b.position ?? 0))
          .map((c) => ({ id: c.id, label: `#${c.name ?? c.id}`, guildId: g.id })),
      };
    }),
  );
  const dms = await rest
    .fetchDmChannels()
    .then((list) =>
      list
        .filter((c) => !c.is_message_request)
        .map((c) => ({
          id: c.id,
          label: c.name || (c.recipients ?? []).map((u) => u.global_name || u.username).join(', '),
          guildId: HOME,
        })),
    )
    .catch(() => []);
  return [
    ...(dms.length ? [{ id: HOME, name: t('pwa.share.dms'), targets: dms }] : []),
    ...groups.filter((g) => g.targets.length),
  ];
}

/**
 * Share Target landing page (XC-PWA-006): other apps share text or a link to SIDE ROOM; the user picks a
 * channel and the text lands in that channel's composer draft, ready to review and send. Nothing is posted
 * without the user pressing send. Files are not accepted yet (the manifest asks for text and links only).
 */
export function SharePage() {
  const t = useT();
  const navigate = useNavigate();
  const searchStr = useLocation({ select: (l) => l.searchStr });
  const shared = useMemo(() => parseShare(searchStr), [searchStr]);
  const draft = shareToDraft(shared);
  const [groups, setGroups] = useState<Group[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let live = true;
    loadGroups(t)
      .then((g) => live && setGroups(g))
      .catch((e: unknown) => {
        if (!live) return;
        if (e instanceof ApiError && e.status === 401) {
          void navigate({
            to: '/login',
            search: { next: `/share${searchStr}` },
          });
        } else setFailed(true);
      });
    return () => {
      live = false;
    };
  }, [navigate, t, searchStr]);

  const pick = (target: Target) => {
    const existing = drafts.get(target.id);
    drafts.set(target.id, existing ? `${existing}\n${draft}` : draft);
    void navigate({
      to: '/channels/$guildId/$channelId',
      params: { guildId: target.guildId, channelId: target.id },
    });
  };

  return (
    <main className="sr-share" id="main">
      <h1>{t('pwa.share.title')}</h1>
      {draft ? (
        <blockquote className="sr-share-preview" aria-label={t('pwa.share.preview')}>
          {draft}
        </blockquote>
      ) : (
        <Notice variant="info">{t('pwa.share.empty')}</Notice>
      )}
      {failed && <Notice variant="danger">{t('pwa.share.error')}</Notice>}
      {draft && !groups && !failed && <p role="status">{t('pwa.share.loading')}</p>}
      {draft &&
        groups?.map((g) => (
          <section key={g.id} aria-labelledby={`share-${g.id}`}>
            <h2 id={`share-${g.id}`}>{g.name}</h2>
            <ul className="sr-share-list">
              {g.targets.map((x) => (
                <li key={x.id}>
                  <Button variant="secondary" onClick={() => pick(x)}>
                    {x.label}
                  </Button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      {draft && groups && groups.length === 0 && (
        <Notice variant="info">{t('pwa.share.noTargets')}</Notice>
      )}
    </main>
  );
}
