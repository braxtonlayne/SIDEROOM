// Reviewer extras (REV-RV-002, REV-RV-003): the duplicate panel, the video proxy player, the
// manifest diff and the sandboxed package preview. They sit on the review page next to the
// metadata; each renders nothing when the submission has nothing of its kind.
import { Button, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { ActionStatus, Async, useAction, useResource } from '../admin/ui';
import * as api from './apiP13';
import './review.css';

const REASONS: Record<string, MessageKey> = {
  title: 'review.dup.reasonTitle',
  alt_title: 'review.dup.reasonAltTitle',
  year: 'review.dup.reasonYear',
  cover: 'review.dup.reasonCover',
};

/** Possible duplicates with "merge", "replacement" and "not a duplicate" for the claim holder. */
export function DuplicatePanel({ id, canResolve }: { id: string; canResolve: boolean }) {
  const t = useT();
  const res = useResource(`dup:${id}`, () => api.duplicatesForReview(id));
  const action = useAction();
  const [note, setNote] = useState('');
  const resolve = async (
    act: api.DuplicateResolution['action'],
    target?: string,
  ): Promise<void> => {
    const out = await action.run(
      () =>
        api.resolveDuplicate(id, {
          action: act,
          ...(target ? { target } : {}),
          ...(note.trim() ? { note: note.trim() } : {}),
        }),
      t('review.ingest.saved'),
    );
    if (out) res.set(out);
  };
  return (
    <section className="rv-panel" aria-labelledby="rv-dup-h">
      <h3 id="rv-dup-h">{t('review.dup.title')}</h3>
      <Async res={res} label={t('review.dup.loading')}>
        {(r: api.DuplicateReport) => (
          <>
            {r.matches.length === 0 ? (
              <p className="rv-muted">{t('review.dup.none')}</p>
            ) : (
              <ul className="rv-changes" aria-label={t('review.dup.list')}>
                {r.matches.map((m) => (
                  <li key={m.ref_id}>
                    <strong>
                      {m.href ? <a href={m.href}>{m.title}</a> : m.title}
                      {m.year ? ` ${t('review.dup.year', { year: m.year })}` : ''}
                    </strong>
                    <span>
                      {t('review.dup.score', { score: m.score })}
                      {': '}
                      {m.reasons.map((x) => t(REASONS[x] ?? 'review.dup.reasonTitle')).join(', ')}
                    </span>
                    {canResolve && !r.resolution && (
                      <span className="rv-row">
                        <Button
                          size="sm"
                          variant="secondary"
                          loading={action.busy}
                          onClick={() => void resolve('merge', m.ref_id)}
                        >
                          {t('review.dup.merge', { title: m.title })}
                        </Button>
                        <Button
                          size="sm"
                          variant="secondary"
                          loading={action.busy}
                          onClick={() => void resolve('replacement', m.ref_id)}
                        >
                          {t('review.dup.replacement', { title: m.title })}
                        </Button>
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {r.resolution && (
              <p role="status">
                {r.resolution.action === 'merge'
                  ? t('review.dup.resolvedMerge', { target: r.resolution.target ?? '' })
                  : r.resolution.action === 'replacement'
                    ? t('review.dup.resolvedReplacement', { target: r.resolution.target ?? '' })
                    : t('review.dup.resolvedNot')}
              </p>
            )}
            {canResolve && !r.resolution && r.matches.length > 0 && (
              <div className="rv-stack">
                <TextInput
                  label={t('review.dup.note')}
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                />
                <div className="rv-row">
                  <Button
                    size="sm"
                    variant="secondary"
                    loading={action.busy}
                    onClick={() => void resolve('not_duplicate')}
                  >
                    {t('review.dup.notDuplicate')}
                  </Button>
                </div>
                <p className="rv-muted">{t('review.dup.mergeHint')}</p>
              </div>
            )}
            <ActionStatus error={action.error} done={action.done} />
          </>
        )}
      </Async>
    </section>
  );
}

/** One video with its review proxy: native controls, a scrubber and single-frame steps. */
function ProxyVideo({ item }: { item: api.ReviewMediaItem }) {
  const t = useT();
  const [video, setVideo] = useState<HTMLVideoElement | null>(null);
  const [pos, setPos] = useState(0);
  const total = item.duration_ms ? item.duration_ms / 1000 : 0;
  const seek = (secs: number) => {
    if (!video) return;
    video.currentTime = Math.max(0, Math.min(total || video.duration || secs, secs));
    setPos(video.currentTime);
  };
  const FRAME = 1 / 24;
  return (
    <div className="rv-stack">
      <h4>{item.label}</h4>
      <video
        ref={setVideo}
        src={item.url ?? undefined}
        controls
        preload="metadata"
        aria-label={t('review.media.player', { label: item.label })}
        onTimeUpdate={(e) => setPos(e.currentTarget.currentTime)}
        style={{ maxInlineSize: '100%' }}
      />
      {total > 0 && (
        <div className="rv-row">
          <Button
            size="sm"
            variant="secondary"
            aria-label={t('review.media.back')}
            onClick={() => seek(pos - FRAME)}
          >
            ‹
          </Button>
          <input
            type="range"
            min={0}
            max={total}
            step={FRAME}
            value={Math.min(pos, total)}
            aria-label={t('review.media.scrub', { label: item.label })}
            onChange={(e) => seek(Number(e.target.value))}
          />
          <Button
            size="sm"
            variant="secondary"
            aria-label={t('review.media.forward')}
            onClick={() => seek(pos + FRAME)}
          >
            ›
          </Button>
        </div>
      )}
    </div>
  );
}

/** The 480p review proxies of the videos in a Cinema submission. Nothing for other kinds. */
export function VideoProxyPlayer({ id }: { id: string }) {
  const t = useT();
  const res = useResource(`media:${id}`, () => api.reviewMedia(id));
  if (res.data && res.data.items.length === 0) return null;
  return (
    <section className="rv-panel" aria-labelledby="rv-media-h">
      <h3 id="rv-media-h">{t('review.media.title')}</h3>
      <Async res={res} label={t('review.media.loading')}>
        {(m: { items: api.ReviewMediaItem[] }) => (
          <>
            {m.items.map((it) =>
              it.state === 'ready' && it.url ? (
                <ProxyVideo key={it.ref_id} item={it} />
              ) : (
                <p key={it.ref_id} className="rv-muted">
                  {it.state === 'processing'
                    ? t('review.media.processing', { label: it.label })
                    : it.state === 'failed'
                      ? t('review.media.failed', { label: it.label })
                      : t('review.media.none', { label: it.label })}
                </p>
              ),
            )}
          </>
        )}
      </Async>
    </section>
  );
}

// ---- manifest diff ----

export interface ManifestChange {
  path: string;
  kind: 'added' | 'removed' | 'changed';
  before?: string;
  after?: string;
}

function flatten(v: unknown, path: string, out: Map<string, string>): void {
  if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
    for (const [k, x] of Object.entries(v as Record<string, unknown>))
      flatten(x, path ? `${path}.${k}` : k, out);
    return;
  }
  out.set(path, JSON.stringify(v));
}

/** Leaf-by-leaf difference between two manifests (objects flatten to dotted paths, arrays compare whole). */
export function manifestDiff(before: unknown, after: unknown): ManifestChange[] {
  const a = new Map<string, string>();
  const b = new Map<string, string>();
  flatten(before ?? {}, '', a);
  flatten(after ?? {}, '', b);
  const out: ManifestChange[] = [];
  for (const [k, v] of b) {
    if (!a.has(k)) out.push({ path: k, kind: 'added', after: v });
    else if (a.get(k) !== v) out.push({ path: k, kind: 'changed', before: a.get(k)!, after: v });
  }
  for (const [k, v] of a) if (!b.has(k)) out.push({ path: k, kind: 'removed', before: v });
  return out.sort((x, y) => x.path.localeCompare(y.path));
}

export function ManifestDiff({ before, after }: { before: unknown; after: unknown }) {
  const t = useT();
  const changes = manifestDiff(before, after);
  const label: Record<ManifestChange['kind'], MessageKey> = {
    added: 'review.manifest.added',
    removed: 'review.manifest.removed',
    changed: 'review.manifest.changed',
  };
  return (
    <section className="rv-panel" aria-labelledby="rv-manifest-h">
      <h3 id="rv-manifest-h">{t('review.manifest.title')}</h3>
      {changes.length === 0 ? (
        <p className="rv-muted">{t('review.manifest.none')}</p>
      ) : (
        <ul className="rv-changes" aria-label={t('review.manifest.list')}>
          {changes.map((c) => (
            <li key={`${c.kind}:${c.path}`}>
              <strong>
                {t(label[c.kind])}: {c.path}
              </strong>
              {c.before !== undefined && <span className="rv-before">{c.before}</span>}
              {c.before !== undefined && c.after !== undefined && <span aria-hidden="true">→</span>}
              {c.after !== undefined && <span className="rv-after">{c.after}</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

// ---- package sandbox preview ----

/** Only plain declarations survive: no at-rules that load things, no urls, no expressions. */
export function safeThemeCss(css: string): string {
  return css
    .replace(/@import[^;]*;?/gi, '')
    .replace(/url\s*\([^)]*\)/gi, 'none')
    .replace(/expression\s*\([^)]*\)/gi, 'none')
    .replace(/<\/?style[^>]*>/gi, '')
    .slice(0, 20_000);
}

/** A theme package's CSS applied to a few sample elements inside a sandboxed frame (no scripts, no network). */
export function PackagePreview({ css }: { css: string }) {
  const t = useT();
  const doc = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'"><style>${safeThemeCss(css)}</style><body><p>${t('review.package.sample')}</p><button type="button">${t('review.package.button')}</button></body>`;
  return (
    <section className="rv-panel" aria-labelledby="rv-package-h">
      <h3 id="rv-package-h">{t('review.package.title')}</h3>
      <iframe
        title={t('review.package.frame')}
        sandbox=""
        srcDoc={doc}
        style={{ inlineSize: '100%', blockSize: 160, border: '1px solid currentColor' }}
      />
      <p className="rv-muted">{t('review.package.note')}</p>
    </section>
  );
}
