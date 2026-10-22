import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
// @ts-expect-error plain ES module without types
import { DOC, OUT, events, opcodes, render } from '../../../../../scripts/lib/gateway-docs.mjs';
import { GATEWAY_EVENTS, GATEWAY_OPCODES } from '../gatewayDocs.generated';
import { endpointsOf } from '../Reference';

const squash = (s: string) =>
  s
    .replace(/\s+/g, '')
    .replace(/["']/g, '')
    .replace(/,(?=[\]}])/g, '');

describe('generated references (DEV-PORT-004)', () => {
  it('the gateway tables are the ones docs/05 produces (run scripts/lib/gateway-docs.mjs to refresh)', () => {
    const md = readFileSync(DOC as string, 'utf8');
    expect(squash(readFileSync(OUT as string, 'utf8'))).toBe(squash(render(md) as string));
  });

  it('reads the opcode table and the event names', () => {
    const md = readFileSync(DOC as string, 'utf8');
    expect((opcodes(md) as unknown[]).length).toBe(GATEWAY_OPCODES.length);
    expect(GATEWAY_OPCODES.find((o) => o.op === '2')?.name).toBe('IDENTIFY');
    expect(GATEWAY_OPCODES.find((o) => o.op === '8')?.note).toMatch(/6\.6a/);
    expect(events(md)).toEqual(GATEWAY_EVENTS);
    expect(GATEWAY_EVENTS).toContain('SR_ACTIVITY_INSTANCE_UPDATE');
    expect(GATEWAY_EVENTS).toContain('MESSAGE_CREATE');
  });

  it('documents the restored flag of MESSAGE_CREATE (D-338)', () => {
    const md = readFileSync(DOC as string, 'utf8');
    const s63 = md.slice(
      md.indexOf('### 6.3 Dispatch events'),
      md.indexOf('### 6.4 Session lifecycle'),
    );
    expect(s63).toMatch(/`MESSAGE_CREATE` has one SIDE ROOM field: `restored`/);
    expect(s63).toMatch(/no notification is due/);
  });

  it('lists the operations of an OpenAPI document by area and path', () => {
    const list = endpointsOf({
      paths: {
        '/b': {
          get: { summary: 'B', tags: ['zeta'], security: [{ session: [] }] },
          post: { tags: ['zeta'], responses: { '201': { description: 'Created' } } },
        },
        '/a/{id}': {
          delete: {
            summary: 'A',
            tags: ['alpha'],
            parameters: [{ name: 'id', in: 'path', required: true }],
          },
          parameters: [],
        },
      },
    });
    expect(list.map((e) => `${e.tag} ${e.method} ${e.path}`)).toEqual([
      'alpha DELETE /a/{id}',
      'zeta GET /b',
      'zeta POST /b',
    ]);
    expect(list[1]!.auth).toEqual(['session']);
    expect(list[2]!.responses).toEqual([['201', 'Created']]);
    expect(endpointsOf(null)).toEqual([]);
  });
});
