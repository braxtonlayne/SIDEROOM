import { useNavigate } from '@tanstack/react-router';
import { Button, Icon, Modal, TextInput } from '@sideroom/ui';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiError, apiFetch } from '../api/client';
import { ImageCropDialog } from '../settings/user/ImageCropDialog';
import { readAsDataUri } from '../settings/user/imageCrop';
import { useT } from '../i18n';
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
  const t = useT();
  const [step, setStep] = useState<Step>('choose');
  const [templateId, setTemplateId] = useState<string | null>(null);
  const [purpose, setPurpose] = useState<Purpose>(null);
  const me = useCurrentUser();
  const goto = useGotoGuild();
  const [name, setName] = useState(me.displayName ? `${me.displayName}'s server` : '');
  const [link, setLink] = useState('');
  // The server icon picked in the customise step (LNG-GLD-017): cropped to a square, GIFs kept as they are.
  const [icon, setIcon] = useState<string | null>(null);
  const [cropFile, setCropFile] = useState<File | null>(null);
  const iconInput = useRef<HTMLInputElement>(null);
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

  const template = templateId ? templates?.find((tpl) => tpl.id === templateId) : undefined;

  const go = (next: Step) => {
    setStep(next);
    setError(null);
  };

  const create = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return setError(t('lounge.addServerDialog.nameRequired'));
    setBusy(true);
    setError(null);
    try {
      const guild = await useStore.getState().createGuild(trimmed, templateId ?? undefined);
      // The create call takes no icon yet, so it is set right after; a failure leaves the server without one.
      if (icon)
        await apiFetch(`/guilds/${guild.id}`, {
          method: 'PATCH',
          json: { icon },
          retries: 0,
        }).catch(() => undefined);
      onClose();
      goto(guild.id, firstText(guild.channels));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : t('lounge.addServerDialog.createFailed'));
      setBusy(false);
    }
  };

  const join = async (e: FormEvent) => {
    e.preventDefault();
    const code = parseInviteCode(link);
    if (!code) return setError(t('lounge.addServerDialog.codeRequired'));
    setBusy(true);
    setError(null);
    try {
      const invite = await useStore.getState().acceptInvite(code);
      onClose();
      goto(invite.type !== 1 ? (invite.guild?.id ?? '@me') : '@me', invite.channel.id);
    } catch (err) {
      setError(inviteErrorMessage(err));
      setBusy(false);
    }
  };

  const titles: Record<Step, string> = {
    choose: t('lounge.addServerDialog.titleChoose'),
    purpose: t('lounge.addServerDialog.titlePurpose'),
    customize: t('lounge.addServerDialog.titleCustomize'),
    join: t('lounge.addServerDialog.titleJoin'),
  };
  const descriptions: Record<Step, string | undefined> = {
    choose: t('lounge.addServerDialog.descChoose'),
    purpose: t('lounge.addServerDialog.descPurpose'),
    customize: t('lounge.addServerDialog.descCustomize'),
    join: t('lounge.addServerDialog.descJoin'),
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
            {t('lounge.addServerDialog.createMyOwn')}
          </Button>
          <h3 className="lng-form-h">{t('lounge.addServerDialog.fromTemplate')}</h3>
          {templates && templates.length > 0 && (
            <ul
              className="lng-template-list"
              aria-label={t('lounge.addServerDialog.templatesLabel')}
            >
              {templates.map((tpl) => (
                <li key={tpl.id}>
                  <button
                    type="button"
                    className="lng-template"
                    onClick={() => {
                      setTemplateId(tpl.id);
                      go('purpose');
                    }}
                  >
                    <span className="lng-template-text">
                      <strong>{tpl.name}</strong>
                      <small>{tpl.description}</small>
                    </span>
                    <Icon name="chevron-right" size={16} />
                  </button>
                </li>
              ))}
            </ul>
          )}
          {templatesFailed && (
            <p className="lng-muted" role="status">
              {t('lounge.addServerDialog.templatesUnavailable')}
            </p>
          )}
          <h3 className="lng-form-h">{t('lounge.addServerDialog.haveInvite')}</h3>
          <Button variant="secondary" fullWidth onClick={() => go('join')}>
            {t('lounge.addServerDialog.titleJoin')}
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
            {t('lounge.addServerDialog.forClub')}
          </Button>
          <Button
            variant="secondary"
            fullWidth
            onClick={() => {
              setPurpose('friends');
              go('customize');
            }}
          >
            {t('lounge.addServerDialog.forFriends')}
          </Button>
          <p className="lng-muted">
            {t('lounge.addServerDialog.notSure')}{' '}
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => {
                setPurpose(null);
                go('customize');
              }}
            >
              {t('lounge.addServerDialog.skipQuestion')}
            </button>{' '}
            {t('lounge.addServerDialog.forNow')}
          </p>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('choose')}>
              {t('lounge.addServerDialog.back')}
            </Button>
          </div>
        </div>
      )}
      {step === 'customize' && (
        <form className="lng-form" onSubmit={(e) => void create(e)} noValidate>
          {template && (
            <p className="lng-muted">
              {t('lounge.addServerDialog.startingFrom')}
              <strong>{template.name}</strong>
              {t('lounge.addServerDialog.templateSuffix')}
              {purpose === 'club'
                ? t('lounge.addServerDialog.forCommunity')
                : purpose === 'friends'
                  ? t('lounge.addServerDialog.forFriendsShort')
                  : ''}
              {t('lounge.addServerDialog.period')}
            </p>
          )}
          <div className="lng-icon-pick">
            <span
              className="lng-icon-preview"
              role="img"
              aria-label={t('lounge.addServerDialog.iconPreview')}
            >
              {icon ? <img src={icon} alt="" /> : <Icon name="image" size={24} />}
            </span>
            <Button variant="secondary" onClick={() => iconInput.current?.click()}>
              {icon
                ? t('lounge.addServerDialog.iconChange')
                : t('lounge.addServerDialog.iconUpload')}
            </Button>
            {icon && (
              <Button variant="ghost" onClick={() => setIcon(null)}>
                {t('lounge.addServerDialog.iconRemove')}
              </Button>
            )}
            <input
              ref={iconInput}
              type="file"
              hidden
              accept=".png,.jpg,.jpeg,.gif,.webp"
              aria-label={t('lounge.addServerDialog.iconFile')}
              onChange={(e) => {
                const f = e.target.files?.[0];
                e.target.value = '';
                if (!f) return;
                if (/.gif$/i.test(f.name)) void readAsDataUri(f).then(setIcon, () => undefined);
                else setCropFile(f);
              }}
            />
          </div>
          <TextInput
            label={t('lounge.addServerDialog.serverName')}
            value={name}
            maxLength={100}
            autoFocus
            error={error}
            onChange={(e) => setName(e.target.value)}
          />
          <p className="lng-muted">{t('lounge.addServerDialog.agree')}</p>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('purpose')}>
              {t('lounge.addServerDialog.back')}
            </Button>
            <Button type="submit" loading={busy}>
              {t('lounge.addServerDialog.create')}
            </Button>
          </div>
        </form>
      )}
      <ImageCropDialog
        open={!!cropFile}
        file={cropFile}
        title={t('lounge.addServerDialog.cropTitle')}
        aspect={1}
        outWidth={512}
        onCancel={() => setCropFile(null)}
        onApply={(uri) => {
          setCropFile(null);
          setIcon(uri);
        }}
      />
      {step === 'join' && (
        <form className="lng-form" onSubmit={(e) => void join(e)} noValidate>
          <TextInput
            label={t('lounge.addServerDialog.inviteLabel')}
            value={link}
            placeholder={t('lounge.addServerDialog.invitePlaceholder')}
            autoFocus
            error={error}
            onChange={(e) => setLink(e.target.value)}
          />
          <div>
            <p className="lng-form-h">{t('lounge.addServerDialog.invitesLookLike')}</p>
            <ul className="lng-examples">
              <li>hTKzmak</li>
              <li>https://example.com/invite/hTKzmak</li>
              <li>example.com/invite/cool-people</li>
            </ul>
          </div>
          <div className="lng-form-actions">
            <Button variant="ghost" onClick={() => go('choose')}>
              {t('lounge.addServerDialog.back')}
            </Button>
            <Button type="submit" loading={busy}>
              {t('lounge.addServerDialog.joinServer')}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}
