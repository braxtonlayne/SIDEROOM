import type { MdNode } from '@sideroom/markdown-react';
import { accountAgeDays, requestReasonText } from '../shell/requestReason';
import { showRawSuspiciousLinks, suspiciousTarget } from '../suspiciousLinks';

const text = (s: string): MdNode => ({ type: 'text', text: s });
const masked = (label: string, url: string): Extract<MdNode, { type: 'link' }> => ({
  type: 'link',
  url,
  kind: 'masked',
  children: [text(label)],
});

describe('suspicious links (G-95, D-224)', () => {
  it('flags a masked link whose text names another host', () => {
    expect(suspiciousTarget(masked('paypal.com', 'https://evil.example/login'))).toBe(
      'evil.example',
    );
    expect(suspiciousTarget(masked('https://www.bank.com/x', 'https://phish.test/'))).toBe(
      'phish.test',
    );
  });

  it('leaves honest and plain-worded links alone', () => {
    expect(suspiciousTarget(masked('docs.example.com', 'https://docs.example.com/a'))).toBeNull();
    expect(suspiciousTarget(masked('www.example.com', 'https://example.com/a'))).toBeNull();
    expect(suspiciousTarget(masked('click here', 'https://evil.example/'))).toBeNull();
    const auto: MdNode = {
      type: 'link',
      url: 'https://a.test',
      kind: 'auto',
      children: [text('b.test')],
    };
    expect(suspiciousTarget(auto as Extract<MdNode, { type: 'link' }>)).toBeNull();
  });

  it('shows the raw target and a warning, also inside formatting', () => {
    const nodes: MdNode[] = [
      { type: 'bold', children: [masked('paypal.com', 'https://evil.example/login')] },
    ];
    const out = showRawSuspiciousLinks(nodes, (h) => `WARN ${h}`);
    expect(out).toEqual([
      {
        type: 'bold',
        children: [
          {
            type: 'link',
            url: 'https://evil.example/login',
            kind: 'auto',
            children: [text('https://evil.example/login')],
          },
          text(' WARN evil.example'),
        ],
      },
    ]);
  });

  it('returns the same array when nothing is suspicious', () => {
    const nodes: MdNode[] = [text('hi'), masked('docs', 'https://a.test')];
    expect(showRawSuspiciousLinks(nodes, () => 'x')).toBe(nodes);
  });
});

describe('message request reasons (G-95, D-224)', () => {
  const t = (key: string, vars?: Record<string, string | number>) =>
    `${key}${vars ? JSON.stringify(vars) : ''}`;
  // 3 days after the epoch of crates/sr-core/src/snowflake.rs
  const epoch = 1_767_225_600_000;
  const idAt = (ms: number) => ((BigInt(ms - epoch) << 22n) | 5n).toString();

  it('reads the account age from the snowflake', () => {
    const now = epoch + 10 * 86_400_000;
    expect(accountAgeDays(idAt(epoch + 7 * 86_400_000), now)).toBe(3);
    expect(accountAgeDays('abc', now)).toBeNull();
  });

  it('words each reason', () => {
    const now = epoch + 10 * 86_400_000;
    const recipients = [{ id: idAt(epoch + 7 * 86_400_000), username: 'sam' }];
    expect(requestReasonText({ sr_request_reason: 'new_account', recipients }, t, now)).toBe(
      'lounge.requests.reasonNewAccount{"days":3}',
    );
    expect(requestReasonText({ sr_request_reason: 'server_filter', recipients }, t, now)).toBe(
      'lounge.requests.reasonServerFilter',
    );
    expect(requestReasonText({ sr_request_reason: null, recipients }, t, now)).toBeNull();
  });
});
