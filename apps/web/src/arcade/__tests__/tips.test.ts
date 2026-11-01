// ARC-ROOM-001: tip badge host validation and the two-tier list (mirrors crates/sr-core/src/tips.rs and
// crates/sr-activities/src/arcade.rs, which are the authority).
import { describe, expect, it } from 'vitest';
import { safeTips, tipProblem, tipsProblem } from '../tips';
import { TIERS } from '../types';

const BC1 = 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';

describe('tip links', () => {
  it('admit only allow-listed hosts for Ko-fi and Patreon', () => {
    for (const [k, u] of [
      ['kofi', 'https://ko-fi.com/someone'],
      ['kofi', 'https://www.ko-fi.com/someone'],
      ['patreon', 'https://www.patreon.com/someone'],
      ['patreon', 'https://patreon.com/user?u=123456'],
    ] as const) {
      expect(tipProblem(k, u), u).toBeNull();
    }
    for (const [k, u, p] of [
      ['kofi', 'https://evil.example/x', 'host'],
      ['kofi', 'https://ko-fi.com.evil.example/x', 'host'],
      ['kofi', 'https://sub.ko-fi.com/x', 'host'],
      ['patreon', 'https://ko-fi.com/x', 'host'],
      ['kofi', 'http://ko-fi.com/x', 'malformed'],
      ['kofi', 'https://user@ko-fi.com/x', 'malformed'],
      ['kofi', 'https://ko-fi.com:8443/x', 'malformed'],
      ['kofi', 'https://ko-fi.com/', 'malformed'],
      ['kofi', 'https://ko-fi.com/a/../b', 'malformed'],
      ['kofi', 'https://ko-fi.com/a#f', 'malformed'],
      ['kofi', 'javascript:alert(1)', 'malformed'],
      ['paypal', 'https://paypal.me/x', 'kind'],
    ] as const) {
      expect(tipProblem(k, u), u).toBe(p);
    }
  });

  it('check Bitcoin and Lightning URIs', () => {
    expect(tipProblem('btc', BC1)).toBeNull();
    expect(tipProblem('btc', `${BC1}?amount=0.001&label=Thanks`)).toBeNull();
    expect(tipProblem('btc', 'bitcoin:nope')).toBe('malformed');
    expect(tipProblem('btc', `${BC1}?req-x=1`)).toBe('malformed');
    expect(tipProblem('btc', 'https://example.com')).toBe('malformed');
    expect(tipProblem('lightning', `lightning:lnbc1pvjluezsp5${'q'.repeat(60)}`)).toBeNull();
    expect(tipProblem('lightning', 'lightning:user@example.com')).toBe('malformed');
    expect(tipProblem('lightning', `lightning:lnbc${'q'.repeat(1300)}`)).toBe('long');
  });

  it('allow at most four, one of each kind', () => {
    const kofi = { kind: 'kofi', url: 'https://ko-fi.com/a' };
    expect(tipsProblem([kofi, { kind: 'btc', url: BC1 }])).toBeNull();
    expect(tipsProblem([kofi, kofi])).toBe('duplicate');
    expect(tipsProblem(Array.from({ length: 5 }, () => ({ kind: 'btc', url: BC1 })))).toBe('many');
    expect(
      safeTips([kofi, { kind: 'kofi', url: 'https://evil.example/x' }, { kind: 'btc', url: BC1 }]),
    ).toEqual([kofi, { kind: 'btc', url: BC1 }]);
  });
});

describe('content tiers', () => {
  it('are exactly the two the manifest validator accepts', () => {
    expect([...TIERS]).toEqual(['TIER_1_ORIGINAL', 'TIER_2_REIMPLEMENTATION']);
    expect(TIERS as readonly string[]).not.toContain('TIER_3_FAN_IP');
  });
});
