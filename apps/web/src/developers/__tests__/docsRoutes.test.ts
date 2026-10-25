// RDY-DEV-004: every route and gateway event the developer docs name exists in the built API. The OpenAPI document
// is the one the server generates (packages/api-client/openapi.json, kept current by the openapi workflow); the
// event names come from docs/05 through gatewayDocs.generated.ts.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { DOC_PAGES, type Block } from '../docsContent';
import { GATEWAY_EVENTS, GATEWAY_OPCODES } from '../gatewayDocs.generated';

interface OpenApi {
  paths: Record<string, Record<string, unknown>>;
}
const openapi = JSON.parse(
  readFileSync(resolve(process.cwd(), '../../packages/api-client/openapi.json'), 'utf8'),
) as OpenApi;

/** Every string the docs show a reader. */
function texts(): string[] {
  const out: string[] = [];
  const add = (b: Block) => {
    if (b.t === 'p' || b.t === 'h') out.push(b.text);
    else if (b.t === 'ul') out.push(...b.items);
    else if (b.t === 'code') out.push(b.code);
    else if (b.t === 'note') out.push(b.title, b.text);
  };
  for (const page of DOC_PAGES) {
    out.push(page.summary);
    page.blocks.forEach(add);
  }
  return out;
}

/** `{id}`, `$ID` and `:id` all stand for one path segment. */
const shape = (path: string) =>
  path
    .split('/')
    .map((s) => (/^(\{[^}]*\}|\$[A-Za-z_]+|:[A-Za-z_]+)$/.test(s) ? '{}' : s))
    .join('/');

const known = new Map<string, Set<string>>();
for (const [path, ops] of Object.entries(openapi.paths)) {
  const key = shape(path);
  const methods = known.get(key) ?? new Set<string>();
  Object.keys(ops).forEach((m) => methods.add(m.toUpperCase()));
  known.set(key, methods);
}

/** Routes the docs name as `METHOD /path` or as `{origin}/api/v1/path`. */
function namedRoutes(): { method?: string; path: string }[] {
  const found: { method?: string; path: string }[] = [];
  for (const t of texts()) {
    for (const m of t.matchAll(
      /\b(GET|POST|PUT|PATCH|DELETE)\s+(\/[A-Za-z0-9_@{}$:/.-]*[A-Za-z0-9_@}])/g,
    )) {
      found.push({ method: m[1] ?? '', path: m[2] ?? '' });
    }
    for (const m of t.matchAll(
      /\{origin\}\/api\/v(?:1|10)(\/[A-Za-z0-9_@{}$:/.-]*[A-Za-z0-9_@}])/g,
    )) {
      found.push({ path: m[1] ?? '' });
    }
  }
  return found;
}

/** Served by the server but not described by the generated document. */
const NOT_IN_OPENAPI = new Set(['/openapi.json']);

describe('the developer docs name only routes the API has (RDY-DEV-004)', () => {
  const routes = namedRoutes();

  it('finds the routes the pages name', () => {
    const paths = routes.map((r) => r.path);
    expect(paths).toEqual(
      expect.arrayContaining([
        '/users/@me',
        '/oauth2/token',
        '/gateway/bot',
        '/interactions/{id}/{token}/callback',
        '/applications/$APP_ID/commands',
        '/channels/$CHANNEL_ID/messages',
      ]),
    );
  });

  it.each([...new Set(routes.map((r) => `${r.method ?? 'any'} ${r.path}`))])(
    '%s is in OpenAPI',
    (key) => {
      const [method = '', path = ''] = key.split(' ');
      if (NOT_IN_OPENAPI.has(path)) return;
      const methods = known.get(shape(path));
      expect(methods, `${path} is not a route of the generated OpenAPI document`).toBeDefined();
      if (method !== 'any') {
        expect(methods?.has(method), `${method} ${path} is not an operation of the API`).toBe(true);
      }
    },
  );

  it('names only gateway events and opcodes the reference lists', () => {
    const names = new Set([...GATEWAY_EVENTS, ...GATEWAY_OPCODES.map((o) => o.name)]);
    const named = new Set<string>();
    for (const t of texts()) {
      for (const m of t.matchAll(
        /\b([A-Z]+(?:_[A-Z]+)*_(?:CREATE|UPDATE|DELETE|CHUNK)|READY|RESUMED)\b/g,
      )) {
        named.add(m[1] ?? '');
      }
    }
    expect([...named]).toEqual(expect.arrayContaining(['GUILD_CREATE', 'INTERACTION_CREATE']));
    for (const n of named) {
      expect(
        names.has(n) || n === 'READY' || n === 'RESUMED',
        `${n} is not a documented gateway event`,
      ).toBe(true);
    }
  });
});
