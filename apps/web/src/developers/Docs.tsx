import { EmptyState, Notice } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';
import { DOC_BY_ID, DOC_PAGES, type Block } from './docsContent';
import { CodeBlock } from './ui';
import { GatewayReference, OpenApiReference } from './Reference';

const fill = (s: string) => s.replaceAll('{origin}', window.location.origin);

function Blocks({ blocks }: { blocks: Block[] }) {
  return (
    <>
      {blocks.map((b, i) => {
        switch (b.t) {
          case 'h':
            return (
              <h2 key={i} className="dev-doc-h">
                {b.text}
              </h2>
            );
          case 'p':
            return <p key={i}>{fill(b.text)}</p>;
          case 'ul':
            return (
              <ul key={i} className="dev-doc-list">
                {b.items.map((x) => (
                  <li key={x}>{fill(x)}</li>
                ))}
              </ul>
            );
          case 'code':
            return <CodeBlock key={i} lang={b.lang} title={b.title} code={fill(b.code)} />;
          case 'ref':
            return b.what === 'openapi' ? (
              <OpenApiReference key={i} />
            ) : (
              <GatewayReference key={i} />
            );
          case 'note':
            return (
              <Notice key={i} variant="info" title={b.title}>
                {fill(b.text)}
              </Notice>
            );
        }
      })}
    </>
  );
}

export function DocsIndexPage() {
  const t = useT();
  return (
    <>
      <h1 className="dev-title">{t('developers.docs.heading')}</h1>
      <p className="dev-muted">{t('developers.docs.intro')}</p>
      <ul className="dev-doc-grid" aria-label={t('developers.docs.pagesLabel')}>
        {DOC_PAGES.map((p) => (
          <li key={p.id}>
            <Link to="/developers/docs/$page" params={{ page: p.id }} className="dev-app-card">
              <span className="dev-app-card-text">
                <strong>{p.title}</strong>
                <span className="dev-muted">{p.summary}</span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </>
  );
}

export function DocPageView({ page }: { page: string }) {
  const t = useT();
  const doc = DOC_BY_ID.get(page);
  if (!doc)
    return (
      <EmptyState
        title={t('developers.docs.noSuchPage')}
        action={
          <Link to="/developers/docs" className="dev-link">
            {t('developers.docs.allDocumentation')}
          </Link>
        }
      />
    );
  const i = DOC_PAGES.indexOf(doc);
  const prev = DOC_PAGES[i - 1];
  const next = DOC_PAGES[i + 1];
  return (
    <div className="dev-doc">
      <nav aria-label={t('developers.docs.navLabel')} className="dev-doc-nav">
        <ul>
          {DOC_PAGES.map((p) => (
            <li key={p.id}>
              <Link
                to="/developers/docs/$page"
                params={{ page: p.id }}
                aria-current={p.id === doc.id ? 'page' : undefined}
              >
                {p.title}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
      <article className="dev-doc-body">
        <h1 className="dev-title">{doc.title}</h1>
        <Blocks blocks={doc.blocks} />
        <div className="dev-doc-pager">
          {prev ? (
            <Link to="/developers/docs/$page" params={{ page: prev.id }} className="dev-link">
              {t('developers.docs.previous', { title: prev.title })}
            </Link>
          ) : (
            <span />
          )}
          {next && (
            <Link to="/developers/docs/$page" params={{ page: next.id }} className="dev-link">
              {t('developers.docs.next', { title: next.title })}
            </Link>
          )}
        </div>
      </article>
    </div>
  );
}
