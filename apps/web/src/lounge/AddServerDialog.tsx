import { useNavigate } from '@tanstack/react-router';
import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { useStore } from '../store';
import type { Channel as StoreChannel } from '../store';
import { useCurrentUser } from './data';
import { inviteErrorMessage, parseInviteCode } from './inviteFlow';

/** Navigate to a guild (and a channel in it when known). */
export function useGotoGuild() {
  const navigate = useNavigate();
  return (guildId: string, channelId?: string | null) => {
    if (channelId)
      void navigate({ to: '/channels/$guildId/$channelId', params: { guildId, channelId } });
    else void navigate({ to: '/channels/$guildId', params: { guildId } });
  };
}

const firstText = (list: readonly StoreChannel[] | undefined) =>
  [...(list ?? [])]
    .filter((c) => c.type === 0)
    .sort((a, b) => (a.position ?? 0) - (b.position ?? 0))[0]?.id ?? null;

/** "Add a Server": Create My Own (POST /guilds) or Join a Server (paste an invite link or code). */
export function AddServerDialog({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<'choose' | 'create' | 'join'>('choose');
  const me = useCurrentUser();
  const goto = useGotoGuild();
  const [name, setName] = useState(me.displayName ? `${me.displayName}'s server` : '');
  const [link, setLink] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const back = () => {
    setStep('choose');
    setError(null);
  };

  const create = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return setError('Give your server a name.');
    setBusy(true);
    setError(null);
    try {
      const guild = await useStore.getState().createGuild(trimmed);
      onClose();
      goto(guild.id, firstText(guild.channels));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Could not create the server. Try again.');
      setBusy(false);
    }
  };

  const join = async (e: FormEvent) => {
    e.preventDefault();
    const code = parseInviteCode(link);
    if (!code) return setError('Paste an invite link or code.');
    setBusy(true);
    setError(null);
    try {
      const invite = await useStore.getState().acceptInvite(code);
      onClose();
      goto(invite.guild.id, invite.channel.id);
    } catch (err) {
      setError(inviteErrorMessage(err));
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={
        step === 'choose'
          ? 'Add a Server'
          : step === 'create'
            ? 'Customize your server'
            : 'Join a Server'
      }
      description={
        step === 'choose'
          ? 'Your server is where you and your friends hang out.'
          : step === 'join'
            ? 'Enter an invite below to join an existing server.'
            : undefined
      }
    >
      {step === 'choose' && (
        <div className="lng-choice-list">
          <Button variant="secondary" fullWidth onClick={() => setStep('create')}>
            Create My Own
          </Button>
          <Button variant="secondary" fullWidth onClick={() => setStep('join')}>
            Join a Server
          </Button>
        </div>
      )}
      {step === 'create' && (
        <form className="lng-form" onSubmit={(e) => void create(e)} noValidate>
          <TextInput
            label="Server name"
            value={name}
            maxLength={100}
            autoFocus
            error={error}
            onChange={(e) => setName(e.target.value)}
          />
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={back}>
              Back
            </Button>
            <Button type="submit" loading={busy}>
              Create
            </Button>
          </div>
        </form>
      )}
      {step === 'join' && (
        <form className="lng-form" onSubmit={(e) => void join(e)} noValidate>
          <TextInput
            label="Invite link or code"
            value={link}
            placeholder="https://example.com/invite/AbCd123"
            autoFocus
            error={error}
            onChange={(e) => setLink(e.target.value)}
          />
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={back}>
              Back
            </Button>
            <Button type="submit" loading={busy}>
              Join Server
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}
