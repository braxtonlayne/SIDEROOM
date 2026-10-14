import { Button, Switch, TextArea, TextInput, useToast } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useRef, useState } from 'react';
import { AppIcon } from '../Applications';
import { errorText } from '../api';
import { t as translate, useT } from '../../i18n';
import { useDeleteApp, usePatchApp } from '../hooks';
import { policyUrlError } from '../oauth';
import type { DevApp } from '../types';
import { PasswordConfirm } from '../ui';

const MAX_ICON_BYTES = 2 * 1024 * 1024;

function readDataUri(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error(translate('developers.general.readFailed')));
    r.readAsDataURL(file);
  });
}

export function GeneralTab({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const navigate = useNavigate();
  const patch = usePatchApp(app.id);
  const del = useDeleteApp();
  const [name, setName] = useState(app.name);
  const [description, setDescription] = useState(app.description);
  const [privacy, setPrivacy] = useState(app.privacy_policy_url ?? '');
  const [terms, setTerms] = useState(app.terms_of_service_url ?? '');
  const [icon, setIcon] = useState<string | null | undefined>(undefined); // undefined = unchanged
  const [iconError, setIconError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const file = useRef<HTMLInputElement>(null);

  const trimmed = name.trim();
  const nameError =
    trimmed.length < 2 || trimmed.length > 32 ? t('developers.general.nameError') : undefined;
  const privacyError = policyUrlError(privacy) ?? undefined;
  const termsError = policyUrlError(terms) ?? undefined;
  const dirty =
    trimmed !== app.name ||
    description !== app.description ||
    privacy.trim() !== (app.privacy_policy_url ?? '') ||
    terms.trim() !== (app.terms_of_service_url ?? '') ||
    icon !== undefined;
  const shownIcon = icon === undefined ? app.icon : icon;

  const pick = async (f: File | undefined) => {
    if (!f) return;
    if (!/\.(png|jpe?g|gif|webp)$/i.test(f.name)) {
      setIconError(t('developers.general.iconBadType', { name: f.name }));
      return;
    }
    if (f.size > MAX_ICON_BYTES) {
      setIconError(t('developers.general.iconTooLarge', { name: f.name }));
      return;
    }
    try {
      setIcon(await readDataUri(f));
      setIconError(null);
    } catch (e) {
      setIconError(e instanceof Error ? e.message : t('developers.general.readFailed'));
    }
  };

  const save = () => {
    setError(null);
    patch.mutate(
      {
        ...(trimmed !== app.name ? { name: trimmed } : {}),
        ...(description !== app.description ? { description } : {}),
        ...(privacy.trim() !== (app.privacy_policy_url ?? '')
          ? { privacy_policy_url: privacy.trim() || null }
          : {}),
        ...(terms.trim() !== (app.terms_of_service_url ?? '')
          ? { terms_of_service_url: terms.trim() || null }
          : {}),
        ...(icon !== undefined ? { icon } : {}),
      },
      {
        onSuccess: () => {
          setIcon(undefined);
          toast.show({ title: t('developers.general.saved'), variant: 'success' });
        },
        onError: (e) => setError(errorText(e, t('developers.general.saveFailed'))),
      },
    );
  };

  return (
    <div className="dev-stack">
      <section className="dev-card" aria-labelledby="dev-general-h">
        <h2 id="dev-general-h" className="dev-card-title">
          {t('developers.general.heading')}
        </h2>
        <div className="dev-icon-row">
          {shownIcon?.startsWith('data:') ? (
            <img className="dev-app-icon" src={shownIcon} alt="" width={64} height={64} />
          ) : (
            <AppIcon app={{ ...app, icon: shownIcon ?? null }} size={64} />
          )}
          <div className="dev-actions">
            <Button variant="secondary" onClick={() => file.current?.click()}>
              {shownIcon ? t('developers.general.changeIcon') : t('developers.general.uploadIcon')}
            </Button>
            {shownIcon && (
              <Button variant="ghost" onClick={() => setIcon(null)}>
                {t('developers.general.removeIcon')}
              </Button>
            )}
          </div>
          <input
            ref={file}
            type="file"
            hidden
            accept=".png,.jpg,.jpeg,.gif,.webp"
            aria-label={t('developers.general.iconFile')}
            onChange={(e) => {
              void pick(e.target.files?.[0]);
              e.target.value = '';
            }}
          />
        </div>
        {iconError && (
          <p className="dev-error" role="alert">
            {iconError}
          </p>
        )}
        <TextInput
          label={t('developers.general.nameLabel')}
          hint={t('developers.general.nameHint')}
          value={name}
          maxLength={40}
          onChange={(e) => setName(e.target.value)}
          error={nameError}
        />
        <TextArea
          label={t('developers.general.descriptionLabel')}
          hint={t('developers.general.descriptionHint')}
          value={description}
          maxLength={400}
          onChange={(e) => setDescription(e.target.value)}
        />
        <TextInput
          label={t('developers.general.privacyLabel')}
          placeholder={t('developers.general.privacyPlaceholder')}
          value={privacy}
          onChange={(e) => setPrivacy(e.target.value)}
          error={privacyError}
          inputMode="url"
        />
        <TextInput
          label={t('developers.general.termsLabel')}
          placeholder={t('developers.general.termsPlaceholder')}
          value={terms}
          onChange={(e) => setTerms(e.target.value)}
          error={termsError}
          inputMode="url"
        />
        <TextInput label={t('developers.general.applicationId')} value={app.id} readOnly />
        {error && (
          <p className="dev-error" role="alert">
            {error}
          </p>
        )}
        <div className="dev-actions">
          <Button
            disabled={!dirty || !!nameError || !!privacyError || !!termsError}
            loading={patch.isPending}
            onClick={save}
          >
            {t('developers.general.saveChanges')}
          </Button>
        </div>
      </section>

      <section className="dev-card" aria-labelledby="dev-public-h">
        <h2 id="dev-public-h" className="dev-card-title">
          {t('developers.general.whoHeading')}
        </h2>
        <Switch
          label={t('developers.general.publicBot')}
          description={t('developers.general.publicBotDescription')}
          checked={app.bot_public}
          disabled={patch.isPending}
          onChange={(e) =>
            patch.mutate(
              { bot_public: e.target.checked },
              { onError: (err) => setError(errorText(err)) },
            )
          }
        />
      </section>

      <section className="dev-card dev-danger" aria-labelledby="dev-delete-h">
        <h2 id="dev-delete-h" className="dev-card-title">
          {t('developers.general.deleteHeading')}
        </h2>
        <p className="dev-muted">{t('developers.general.deleteWarning')}</p>
        <div className="dev-actions">
          <Button variant="danger" onClick={() => setConfirmDelete(true)}>
            {t('developers.general.deleteHeading')}
          </Button>
        </div>
      </section>
      <PasswordConfirm
        open={confirmDelete}
        title={t('developers.general.deleteTitle', { name: app.name })}
        description={t('developers.general.deleteDescription')}
        requireText={app.name}
        confirmLabel={t('developers.general.deleteHeading')}
        onCancel={() => setConfirmDelete(false)}
        onSubmit={async (password) => {
          await del.mutateAsync({ appId: app.id, password });
          setConfirmDelete(false);
          void navigate({ to: '/developers/applications' });
        }}
      />
    </div>
  );
}
