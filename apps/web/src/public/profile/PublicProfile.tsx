import {
  BlocksEnv,
  ProfileRenderer,
  type BlockSlots,
  type PageLike,
} from '@sideroom/profile-blocks';
import { useEffect, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { EntranceShell } from '../../components/EntranceShell';
import { useT } from '../../i18n';
import { cdnUrl } from '../../store/cdn';

/**
 * The public profile page (PRF-PRO-001, D-382, D-385). It renders exactly what `GET /public/profiles/{handle}`
 * returned: the server already dropped every block the visitor may not see, so there is nothing here to hide and
 * no way to ask for more. It imports the shared renderer (`packages/profile-blocks`) and nothing from the app.
 */

interface Entry {
  id: string;
  author: { display_name: string | null; username: string } | null;
  body: string;
  created_at_ms: number;
}

interface PublicProfileData {
  page: PageLike;
  guestbook: Entry[] | null;
  accepts_guestbook: boolean;
}

const cdn = (
  kind: 'avatars' | 'icons',
  owner: string,
  hash: string | null | undefined,
  size: number,
) => cdnUrl(kind, owner, hash, { size });

function PublicGuestbook({ handle, data }: { handle: string; data: PublicProfileData }) {
  const t = useT();
  const [draft, setDraft] = useState('');
  const [state, setState] = useState<'idle' | 'busy' | 'held' | 'refused' | 'limited'>('idle');
  const send = async (e: FormEvent) => {
    e.preventDefault();
    if (!draft.trim()) return;
    setState('busy');
    try {
      await apiFetch<void>(`/public/profiles/${encodeURIComponent(handle)}/guestbook`, {
        method: 'POST',
        json: { body: draft },
        retries: 0,
      });
      setDraft('');
      setState('held');
    } catch (err) {
      setState(err instanceof ApiError && err.status === 429 ? 'limited' : 'refused');
    }
  };
  const entries = data.guestbook ?? [];
  return (
    <>
      <h3 className="pp-title">{t('profile.public.guestbookTitle')}</h3>
      {entries.length === 0 && <p className="pp-muted">{t('profile.public.guestbookEmpty')}</p>}
      <ul className="pp-entries" aria-label={t('profile.public.guestbookTitle')}>
        {entries.map((e) => (
          <li key={e.id}>
            <div>
              <p className="pp-muted">
                {e.author
                  ? e.author.display_name?.trim() || e.author.username
                  : t('profile.public.visitor')}
              </p>
              <p className="pp-text">{e.body}</p>
            </div>
          </li>
        ))}
      </ul>
      {data.accepts_guestbook && (
        <form onSubmit={(e) => void send(e)} className="pp-sign">
          <label>
            <span className="pp-muted">{t('profile.public.guestbookLabel')}</span>
            <textarea
              value={draft}
              maxLength={500}
              rows={2}
              onChange={(e) => setDraft(e.target.value)}
              aria-describedby="pp-gb-hint"
            />
          </label>
          <p id="pp-gb-hint" className="pp-muted">
            {t('profile.public.guestbookHint')}
          </p>
          <button type="submit" className="pp-link" disabled={state === 'busy' || !draft.trim()}>
            {t('profile.public.guestbookSend')}
          </button>
          {state === 'held' && <p role="status">{t('profile.public.guestbookHeld')}</p>}
          {state === 'refused' && <p role="alert">{t('profile.public.guestbookRefused')}</p>}
          {state === 'limited' && <p role="alert">{t('profile.public.guestbookLimit')}</p>}
        </form>
      )}
    </>
  );
}

function NothingHere({ handle }: { handle: string }) {
  const t = useT();
  return (
    <EntranceShell nav="back-to-entrance">
      <main id="main" className="ent-prose">
        <h1>{t('entrance.notFound.title')}</h1>
        <p>{t('entrance.notFound.body')}</p>
        <p>
          <a href={`/login?next=${encodeURIComponent(`/u/${handle}`)}`}>
            {t('entrance.shell.memberAccess')}
          </a>
        </p>
      </main>
    </EntranceShell>
  );
}

export function PublicProfile({ handle }: { handle: string }) {
  const t = useT();
  const [data, setData] = useState<PublicProfileData | 'none' | null>(null);
  useEffect(() => {
    let live = true;
    apiFetch<PublicProfileData>(`/public/profiles/${encodeURIComponent(handle)}`, {
      retries: 0,
    }).then(
      (d) => live && setData(d),
      () => live && setData('none'),
    );
    return () => {
      live = false;
    };
  }, [handle]);

  useEffect(() => {
    if (data && data !== 'none') {
      document.title = data.page.user.display_name?.trim() || data.page.user.username;
    }
  }, [data]);

  if (data === null) {
    return (
      <p role="status" className="ent-prose">
        {t('profile.public.loading')}
      </p>
    );
  }
  if (data === 'none') return <NothingHere handle={handle} />;
  const slots: BlockSlots = {
    guestbook: () => <PublicGuestbook handle={handle} data={data} />,
  };
  return (
    <BlocksEnv t={t} cdn={cdn}>
      <main id="main">
        <ProfileRenderer
          page={data.page}
          bannerUrl={cdnUrl('avatars', data.page.user.id, data.page.user.banner, { size: 1024 })}
          slots={slots}
        />
      </main>
    </BlocksEnv>
  );
}
