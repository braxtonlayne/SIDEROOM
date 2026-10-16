import { Badge, Button, EmptyState, Notice } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useRef, useState, type DragEvent } from 'react';
import { useT, type MessageKey } from '../i18n';
import {
  myPackages,
  problemLines,
  submitPackage,
  withdrawVersion,
  type MyPackage,
  type PackageKind,
  type VersionView,
} from '../packages/api';
import { useLoad } from '../packages/useLoad';
import '../packages/packages.css';

const STATE_VARIANT: Record<
  VersionView['review_state'],
  'neutral' | 'accent' | 'success' | 'warning' | 'danger'
> = {
  pending: 'accent',
  approved: 'success',
  rejected: 'danger',
  withdrawn: 'neutral',
  revoked: 'warning',
};

function size(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

/**
 * The creator's Packages page (EXT-PKG-003, EXT-PKG-007): submit a package zip (a theme, profile layout,
 * emoji, sticker or sound pack, recipe or server template), see every problem the checks found, follow the
 * review, and withdraw a version that is still waiting. The creator guide is the "Packages" documentation page.
 */
export function PackagesPage() {
  const t = useT();
  const mine = useLoad<MyPackage[]>('my-packages', myPackages);
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [problems, setProblems] = useState<string[]>([]);
  const [sent, setSent] = useState<VersionView | null>(null);

  const send = async (file: File) => {
    setProblems([]);
    setSent(null);
    setProgress(0);
    try {
      const v = await submitPackage(file, setProgress);
      setSent(v);
      mine.reload();
    } catch (e) {
      const lines = problemLines(e);
      setProblems(lines.length ? lines : [t('developers.packages.uploadFailed')]);
    } finally {
      setProgress(null);
    }
  };
  const drop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    const f = e.dataTransfer.files[0];
    if (f) void send(f);
  };

  return (
    <div className="dev-page">
      <h1>{t('developers.packages.title')}</h1>
      <p className="dev-muted">
        {t('developers.packages.intro')}{' '}
        <Link to="/developers/docs/$page" params={{ page: 'packages' }}>
          {t('developers.packages.guideLink')}
        </Link>
      </p>

      <div
        className="pkg-dropzone"
        data-over={over}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={drop}
      >
        <p>{t('developers.packages.drop')}</p>
        <input
          ref={input}
          type="file"
          accept=".zip,application/zip"
          hidden
          aria-label={t('developers.packages.fileLabel')}
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void send(f);
            e.target.value = '';
          }}
        />
        <Button
          variant="primary"
          loading={progress !== null}
          onClick={() => input.current?.click()}
        >
          {progress === null
            ? t('developers.packages.choose')
            : t('developers.packages.uploading', { percent: Math.round(progress * 100) })}
        </Button>
      </div>

      {sent && (
        <Notice variant="success" title={t('developers.packages.sentTitle')}>
          {t('developers.packages.sentBody', { name: sent.name, version: sent.version })}
          {sent.warnings.length > 0 && (
            <ul className="pkg-problems">
              {sent.warnings.map((w, i) => (
                <li key={i}>{w}</li>
              ))}
            </ul>
          )}
        </Notice>
      )}
      {problems.length > 0 && (
        <Notice variant="danger" title={t('developers.packages.refusedTitle')}>
          <ul className="pkg-problems">
            {problems.map((p, i) => (
              <li key={i}>{p}</li>
            ))}
          </ul>
        </Notice>
      )}

      <h2>{t('developers.packages.mine')}</h2>
      {mine.error ? (
        <Notice variant="danger" title={t('developers.packages.loadFailed')}>
          {mine.error}
        </Notice>
      ) : mine.data && mine.data.length === 0 ? (
        <EmptyState
          title={t('developers.packages.emptyTitle')}
          description={t('developers.packages.emptyBody')}
        />
      ) : (
        <ul className="pkg-list" aria-label={t('developers.packages.mine')}>
          {(mine.data ?? []).map((p) => (
            <li key={p.id} className="pkg-card">
              <h3>
                {p.name} <code>{p.slug}</code>
              </h3>
              <p className="pkg-meta">{t(kindLabel(p.kind))}</p>
              <ul className="pkg-list">
                {p.versions.map((v) => (
                  <li key={v.id} className="pkg-row">
                    <span>
                      {v.version} <span className="dev-muted">{size(v.size_bytes)}</span>{' '}
                      <Badge variant={STATE_VARIANT[v.review_state]}>
                        {t(stateLabel(v.review_state))}
                      </Badge>
                      {v.review_note && <span className="dev-muted"> {v.review_note}</span>}
                    </span>
                    {v.review_state === 'pending' && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('developers.packages.withdrawLabel', {
                          name: p.name,
                          version: v.version,
                        })}
                        onClick={() => void withdrawVersion(v.id).then(() => mine.reload())}
                      >
                        {t('developers.packages.withdraw')}
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const KIND: Record<PackageKind, MessageKey> = {
  theme: 'developers.packages.kindTheme',
  profile_layout: 'developers.packages.kindLayout',
  emoji_pack: 'developers.packages.kindEmoji',
  sticker_pack: 'developers.packages.kindSticker',
  sound_pack: 'developers.packages.kindSound',
  recipe: 'developers.packages.kindRecipe',
  server_template: 'developers.packages.kindTemplate',
};
const kindLabel = (k: PackageKind): MessageKey => KIND[k];

const STATE: Record<VersionView['review_state'], MessageKey> = {
  pending: 'developers.packages.statePending',
  approved: 'developers.packages.stateApproved',
  rejected: 'developers.packages.stateRejected',
  withdrawn: 'developers.packages.stateWithdrawn',
  revoked: 'developers.packages.stateRevoked',
};
const stateLabel = (s: VersionView['review_state']): MessageKey => STATE[s];
