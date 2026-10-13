import { useNavigate } from '@tanstack/react-router';
import { Button, Icon, Modal, TextInput } from '@sideroom/ui';
import { useEffect, useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { useStore } from '../store';
import type { Channel as StoreChannel } from '../store';
import { listBuiltinTemplates, type BuiltinTemplate } from '../store/rest';
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

type Step = 'choose' | 'purpose' | 'customize' | 'join';
type Purpose = 'club' | 'friends' | null;

/**
 * "Create a Server" wizard (LNG-GLD-015..018): pick Create My Own or a template, say what it is for, name it;
 * or Join a Server with an invite link or code.
 */
export function AddServerDialog({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>('choose');
  const [templateId, setTemplateId] = useState<string | null>(null);
  const [purpose, setPurpose] = useState<Purpose>(null);
  const me = useCurrentUser();
  const goto = useGotoGuild();
  const [name, setName] = useState(me.displayName ? `${me.displayName}'s server` : '');
  const [link, setLink] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The starter layouts come from the server (GET /guilds/templates/builtin).
  const [templates, setTemplates] = useState<BuiltinTemplate[] | null>(null);
  const [templatesFailed, setTemplatesFailed] = useState(false);
  useEffect(() => {
    let live = true;
    listBuiltinTemplates().then(
      (list) => live && setTemplates(list),
      () => live && setTemplatesFailed(true),
    );
    return () => {
      live = false;
    };
  }, []);

  const template = templateId ? templates?.find((t) => t.id === templateId) : undefined;

  const go = (next: Step) => {
    setStep(next);
    setError(null);
  };

  const create = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return setError('Give your server a name.');
    setBusy(true);
    setError(null);
    try {
      const guild = await useStore.getState().createGuild(trimmed, templateId ?? undefined);
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

  const titles: Record<Step, string> = {
    choose: 'Create a Server',
    purpose: 'Tell us more about your server',
    customize: 'Customize your server',
    join: 'Join a Server',
  };
  const descriptions: Record<Step, string | undefined> = {
    choose: 'Your server is where you and your friends hang out. Make yours and start talking.',
    purpose:
      'In order to help you with your setup, is your new server for just a few friends or a larger community?',
    customize: 'Give your new server a personality with a name. You can always change it later.',
    join: 'Enter an invite below to join an existing server.',
  };

  return (
    <Modal open onClose={onClose} size="sm" title={titles[step]} description={descriptions[step]}>
      {step === 'choose' && (
        <div className="lng-choice-list">
          <Button
            variant="secondary"
            fullWidth
            onClick={() => {
              setTemplateId(null);
              go('purpose');
            }}
          >
            Create My Own
          </Button>
          <h3 className="lng-form-h">Start from a template</h3>
          {templates && templates.length > 0 && (
            <ul className="lng-template-list" aria-label="Templates">
              {templates.map((t) => (
                <li key={t.id}>
                  <button
                    type="button"
                    className="lng-template"
                    onClick={() => {
                      setTemplateId(t.id);
                      go('purpose');
                    }}
                  >
                    <span className="lng-template-text">
                      <strong>{t.name}</strong>
                      <small>{t.description}</small>
                    </span>
                    <Icon name="chevron-right" size={16} />
                  </button>
                </li>
              ))}
            </ul>
          )}
          {templatesFailed && (
            <p className="lng-muted" role="status">
              Templates are unavailable right now. You can still create your own.
            </p>
          )}
          <h3 className="lng-form-h">Have an invite already?</h3>
          <Button variant="secondary" fullWidth onClick={() => go('join')}>
            Join a Server
          </Button>
        </div>
      )}
      {step === 'purpose' && (
        <div className="lng-choice-list">
          <Button
            variant="secondary"
            fullWidth
            onClick={() => {
              setPurpose('club');
              go('customize');
            }}
          >
            For a club or community
          </Button>
          <Button
            variant="secondary"
            fullWidth
            onClick={() => {
              setPurpose('friends');
              go('customize');
            }}
          >
            For me and my friends
          </Button>
          <p className="lng-muted">
            Not sure? You can{' '}
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => {
                setPurpose(null);
                go('customize');
              }}
            >
              skip this question
            </button>{' '}
            for now.
          </p>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('choose')}>
              Back
            </Button>
          </div>
        </div>
      )}
      {step === 'customize' && (
        <form className="lng-form" onSubmit={(e) => void create(e)} noValidate>
          {template && (
            <p className="lng-muted">
              Starting from the <strong>{template.name}</strong> template
              {purpose === 'club'
                ? ' for a community'
                : purpose === 'friends'
                  ? ' for friends'
                  : ''}
              .
            </p>
          )}
          <TextInput
            label="Server name"
            value={name}
            maxLength={100}
            autoFocus
            error={error}
            onChange={(e) => setName(e.target.value)}
          />
          <p className="lng-muted">
            By creating a server, you agree to follow the community guidelines of this site.
          </p>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('purpose')}>
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
          <div>
            <p className="lng-form-h">Invites look like</p>
            <ul className="lng-examples">
              <li>hTKzmak</li>
              <li>https://example.com/invite/hTKzmak</li>
              <li>example.com/invite/cool-people</li>
            </ul>
          </div>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('choose')}>
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
