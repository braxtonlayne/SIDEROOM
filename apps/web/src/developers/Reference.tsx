// Generated reference sections of the documentation hub (DEV-PORT-004): the endpoint list is read from the
// server's own OpenAPI document, the gateway tables come from gatewayDocs.generated.ts (built from the
// specification by scripts/lib/gateway-docs.mjs).
import { Notice, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { useT } from '../i18n';
import { useLoad } from '../packages/useLoad';
import '../packages/packages.css';
import { GATEWAY_EVENTS, GATEWAY_OPCODES } from './gatewayDocs.generated';

interface Param {
  name: string;
  in: string;
  required?: boolean;
  description?: string;
}
interface Operation {
  summary?: string;
  description?: string;
  tags?: string[];
  parameters?: Param[];
  responses?: Record<string, { description?: string }>;
  security?: Record<string, unknown>[];
}
export interface Endpoint {
  method: string;
  path: string;
  tag: string;
  summary: string;
  description: string;
  params: Param[];
  responses: [string, string][];
  /** Names of the credentials the operation accepts; empty for a public endpoint. */
  auth: string[];
}

const METHODS = ['get', 'post', 'put', 'patch', 'delete'] as const;

/** The operations of an OpenAPI document, ordered by tag then path. */
export function endpointsOf(doc: unknown): Endpoint[] {
  const paths = (doc as { paths?: Record<string, Record<string, Operation>> } | null)?.paths ?? {};
  const out: Endpoint[] = [];
  for (const [path, item] of Object.entries(paths)) {
    for (const m of METHODS) {
      const op = item?.[m];
      if (!op) continue;
      out.push({
        method: m.toUpperCase(),
        path,
        tag: op.tags?.[0] ?? 'other',
        summary: op.summary ?? '',
        description: op.description ?? '',
        params: op.parameters ?? [],
        responses: Object.entries(op.responses ?? {}).map(([code, r]) => [
          code,
          r.description ?? '',
        ]),
        auth: (op.security ?? []).flatMap((s) => Object.keys(s)),
      });
    }
  }
  return out.sort((a, b) => a.tag.localeCompare(b.tag) || a.path.localeCompare(b.path));
}

async function fetchDoc(): Promise<Endpoint[]> {
  const r = await fetch('/api/v1/openapi.json', { credentials: 'same-origin' });
  if (!r.ok) throw new Error(String(r.status));
  return endpointsOf(await r.json());
}

export function OpenApiReference() {
  const t = useT();
  const res = useLoad<Endpoint[]>('openapi-endpoints', fetchDoc);
  const [q, setQ] = useState('');
  const [tag, setTag] = useState('');
  const all = useMemo(() => res.data ?? [], [res.data]);
  const tags = useMemo(() => [...new Set(all.map((e) => e.tag))], [all]);
  const needle = q.trim().toLowerCase();
  const shown = all.filter(
    (e) =>
      (!tag || e.tag === tag) &&
      (!needle ||
        `${e.method} ${e.path} ${e.summary} ${e.description}`.toLowerCase().includes(needle)),
  );
  if (res.error)
    return (
      <Notice variant="danger" title={t('developers.reference.loadFailed')}>
        {res.error}
      </Notice>
    );
  if (!res.data)
    return (
      <p className="dev-muted" role="status">
        {t('developers.reference.loading')}
      </p>
    );
  return (
    <div>
      <div className="pkg-toolbar">
        <TextInput
          label={t('developers.reference.search')}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <select
          aria-label={t('developers.reference.area')}
          value={tag}
          onChange={(e) => setTag(e.target.value)}
        >
          <option value="">{t('developers.reference.allAreas')}</option>
          {tags.map((x) => (
            <option key={x} value={x}>
              {x}
            </option>
          ))}
        </select>
      </div>
      <p className="dev-muted" role="status">
        {t('developers.reference.count', { shown: shown.length, total: all.length })}
      </p>
      <ul className="pkg-list" aria-label={t('developers.reference.listLabel')}>
        {shown.slice(0, 300).map((e) => (
          <li key={`${e.method} ${e.path}`}>
            <details className="pkg-row" style={{ display: 'block' }}>
              <summary>
                <code>{e.method}</code> <code>{e.path}</code>{' '}
                <span className="dev-muted">{e.summary}</span>
              </summary>
              {e.description && <p>{e.description}</p>}
              <p className="dev-muted">
                {e.auth.length
                  ? t('developers.reference.auth', { names: e.auth.join(', ') })
                  : t('developers.reference.public')}
              </p>
              {e.params.length > 0 && (
                <ul>
                  {e.params.map((p) => (
                    <li key={`${p.in}:${p.name}`}>
                      <code>{p.name}</code> ({p.in}
                      {p.required ? `, ${t('developers.reference.required')}` : ''})
                      {p.description ? ` ${p.description}` : ''}
                    </li>
                  ))}
                </ul>
              )}
              {e.responses.length > 0 && (
                <ul>
                  {e.responses.map(([code, d]) => (
                    <li key={code}>
                      <code>{code}</code> {d}
                    </li>
                  ))}
                </ul>
              )}
            </details>
          </li>
        ))}
      </ul>
      {shown.length > 300 && (
        <p className="dev-muted">{t('developers.reference.truncated', { count: 300 })}</p>
      )}
    </div>
  );
}

export function GatewayReference() {
  const t = useT();
  return (
    <div>
      <table className="dev-table">
        <caption>{t('developers.reference.opcodes')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('developers.reference.op')}</th>
            <th scope="col">{t('developers.reference.opName')}</th>
            <th scope="col">{t('developers.reference.direction')}</th>
            <th scope="col">{t('developers.reference.opNote')}</th>
          </tr>
        </thead>
        <tbody>
          {GATEWAY_OPCODES.map((o) => (
            <tr key={o.op}>
              <td>{o.op}</td>
              <td>
                <code>{o.name}</code>
              </td>
              <td>{o.dir}</td>
              <td>{o.note}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h2 className="dev-doc-h">{t('developers.reference.events')}</h2>
      <ul className="pkg-items" aria-label={t('developers.reference.events')}>
        {GATEWAY_EVENTS.map((e) => (
          <li key={e}>
            <code>{e}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}
