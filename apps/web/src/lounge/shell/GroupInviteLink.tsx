import { Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { rest } from '../../store';
import { inviteUrl } from '../inviteFlow';

/**
 * Copy-link row for a group DM (RDY-PAR-001, LNG-HOME-005): makes the 24-hour invite on demand
 * (`POST /channels/{id}/invites` on a group DM, owner only) and copies it. The link then stays visible.
 */
export function GroupInviteLink({ channelId }: { channelId: string }) {
  const t = useT();
  const [state, setState] = useState<
    { kind: 'idle' } | { kind: 'busy' } | { kind: 'ready'; url: string } | { kind: 'error' }
  >({ kind: 'idle' });
  const [copied, setCopied] = useState(false);
  const copy = async (url: string) => {
    try {
      await navigator.clipboard?.writeText(url);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };
  const make = async () => {
    setState({ kind: 'busy' });
    try {
      const inv = await rest.createInvite(channelId);
      const url = inviteUrl(inv.code);
      setState({ kind: 'ready', url });
      await copy(url);
    } catch {
      setState({ kind: 'error' });
    }
  };
  return (
    <section className="lng-group-invite" aria-label={t('lounge.createDm.inviteHeading')}>
      <h3 className="lng-pop-h">{t('lounge.createDm.inviteHeading')}</h3>
      <p className="lng-muted">{t('lounge.createDm.inviteHelp')}</p>
      {state.kind === 'ready' ? (
        <div className="prf-invite">
          <input
            readOnly
            className="lng-pop-input"
            aria-label={t('lounge.dmPanel.inviteLink')}
            value={state.url}
            onFocus={(e) => e.currentTarget.select()}
          />
          <Button size="sm" variant="secondary" onClick={() => void copy(state.url)}>
            {copied ? t('lounge.dmPanel.copied') : t('lounge.dmPanel.copy')}
          </Button>
        </div>
      ) : (
        <Button
          size="sm"
          variant="secondary"
          loading={state.kind === 'busy'}
          onClick={() => void make()}
        >
          {t('lounge.createDm.copyInvite')}
        </Button>
      )}
      <span role="status" className="lng-sr-only">
        {copied ? t('lounge.createDm.inviteCopied') : ''}
      </span>
      {state.kind === 'error' && (
        <p role="alert" className="lng-failed">
          {t('lounge.dmPanel.inviteError')}
        </p>
      )}
    </section>
  );
}
