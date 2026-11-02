import { useState } from 'react';
import { useT } from '../i18n';
import { QrBox } from '../tips/QrBox';
import { isQrKind, safeTips, type TipKind, type TipLink } from './tips';

const LABEL = {
  btc: 'arcade.tips.btc',
  lightning: 'arcade.tips.lightning',
  kofi: 'arcade.tips.kofi',
  patreon: 'arcade.tips.patreon',
} as const satisfies Record<TipKind, string>;

function QrTip({ link, name }: { link: TipLink; name: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const label = t(LABEL[link.kind]);
  return (
    <span className="arc-tip-qr">
      <button
        type="button"
        className="arc-tip"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {label}
      </button>
      {open && (
        <span className="arc-tip-pop">
          <QrBox value={link.url} label={t('arcade.tips.qrLabel', { kind: label, name })} />
          <code className="arc-tip-code">{link.url}</code>
        </span>
      )}
    </span>
  );
}

/**
 * Zero-cut creator tip badges (ARC-ROOM-001): Ko-fi and Patreon open the creator's page, Bitcoin and Lightning show
 * a QR code. The platform is never in the payment. Links that fail the host allow-list are not drawn.
 */
export function TipBadges({
  links,
  name,
}: {
  links: readonly { kind: string; url: string }[] | undefined;
  name: string;
}) {
  const t = useT();
  const tips = safeTips(links);
  if (tips.length === 0) return null;
  return (
    <div className="arc-tips" role="group" aria-label={t('arcade.tips.group', { name })}>
      {tips.map((l) =>
        isQrKind(l.kind) ? (
          <QrTip key={l.kind} link={l} name={name} />
        ) : (
          <a
            key={l.kind}
            className="arc-tip"
            href={l.url}
            target="_blank"
            rel="noopener noreferrer nofollow"
          >
            {t(LABEL[l.kind])}
          </a>
        ),
      )}
    </div>
  );
}
