import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { setupServer } from 'msw/node';
import { createHandlers, mockOperations } from './index';

const spec = JSON.parse(
  readFileSync(new URL('../../openapi.json', import.meta.url), 'utf8'),
) as { paths: Record<string, Record<string, { operationId?: string }>> };

describe('generated MSW handlers', () => {
  it('cover every operation in openapi.json', () => {
    const specCount = Object.values(spec.paths).reduce(
      (n, item) => n + Object.keys(item).filter((k) => k !== 'parameters').length,
      0,
    );
    expect(mockOperations.length).toBe(specCount);
  });

  it('serve a schema-shaped body and honour overrides', async () => {
    const op = mockOperations.find((o) => o.method === 'get' && o.status === 200 && o.body);
    expect(op).toBeDefined();
    const url = `http://localhost/api/v1${op!.path.replace(/:[^/]+/g, '1')}`;
    const server = setupServer(...createHandlers());
    server.listen();
    try {
      const res = await fetch(url);
      expect(res.status).toBe(200);
      expect(await res.json()).toEqual(op!.body);
      server.resetHandlers(
        ...createHandlers({ overrides: { [op!.id]: { status: 200, body: { overridden: true } } } }),
      );
      expect(await (await fetch(url)).json()).toEqual({ overridden: true });
    } finally {
      server.close();
    }
  });
});
