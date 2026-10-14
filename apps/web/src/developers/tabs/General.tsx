import { Button, Switch, TextArea, TextInput, useToast } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useRef, useState } from 'react';
import { AppIcon } from '../Applications';
import { errorText } from '../api';
import { useDeleteApp, usePatchApp } from '../hooks';
import { policyUrlError } from '../oauth';
import type { DevApp } from '../types';
import { PasswordConfirm } from '../ui';

const MAX_ICON_BYTES = 2 * 1024 * 1024;

function readDataUri(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error('Could not read that file.'));
    r.readAsDataURL(file);
  });
}

export function GeneralTab({ app }: { app: DevApp }) {
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
    trimmed.length < 2 || trimmed.length > 32 ? 'Use 2 to 32 characters.' : undefined;
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
      setIconError(`${f.name} is not a PNG, JPEG, GIF or WebP image.`);
      return;
    }
    if (f.size > MAX_ICON_BYTES) {
      setIconError(`${f.name} is larger than 2 MB.`);
      return;
    }
    try {
      setIcon(await readDataUri(f));
      setIconError(null);
    } catch (e) {
      setIconError(e instanceof Error ? e.message : 'Could not read that file.');
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
          toast.show({ title: 'Saved.', variant: 'success' });
        },
        onError: (e) => setError(errorText(e, 'Could not save your changes.')),
      },
    );
  };

  return (
    <div className="dev-stack">
      <section className="dev-card" aria-labelledby="dev-general-h">
        <h2 id="dev-general-h" className="dev-card-title">
          General information
        </h2>
        <div className="dev-icon-row">
          {shownIcon?.startsWith('data:') ? (
            <img className="dev-app-icon" src={shownIcon} alt="" width={64} height={64} />
          ) : (
            <AppIcon app={{ ...app, icon: shownIcon ?? null }} size={64} />
          )}
          <div className="dev-actions">
            <Button variant="secondary" onClick={() => file.current?.click()}>
              {shownIcon ? 'Change icon' : 'Upload icon'}
            </Button>
            {shownIcon && (
              <Button variant="ghost" onClick={() => setIcon(null)}>
                Remove icon
              </Button>
            )}
          </div>
          <input
            ref={file}
            type="file"
            hidden
            accept=".png,.jpg,.jpeg,.gif,.webp"
            aria-label="Icon file"
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
          label="Name"
          hint="Also the display name of the bot."
          value={name}
          maxLength={40}
          onChange={(e) => setName(e.target.value)}
          error={nameError}
        />
        <TextArea
          label="Description"
          hint="Shown on the authorize screen and in the app directory."
          value={description}
          maxLength={400}
          onChange={(e) => setDescription(e.target.value)}
        />
        <TextInput
          label="Privacy policy URL"
          placeholder="https://example.com/privacy"
          value={privacy}
          onChange={(e) => setPrivacy(e.target.value)}
          error={privacyError}
          inputMode="url"
        />
        <TextInput
          label="Terms of service URL"
          placeholder="https://example.com/terms"
          value={terms}
          onChange={(e) => setTerms(e.target.value)}
          error={termsError}
          inputMode="url"
        />
        <TextInput label="Application ID" value={app.id} readOnly />
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
            Save changes
          </Button>
        </div>
      </section>

      <section className="dev-card" aria-labelledby="dev-public-h">
        <h2 id="dev-public-h" className="dev-card-title">
          Who can add the bot
        </h2>
        <Switch
          label="Public bot"
          description="Anyone can add the bot to a server they manage. Turn off to keep it to you."
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
          Delete application
        </h2>
        <p className="dev-muted">
          Deleting removes the bot from every server and revokes its token, every authorization and
          all commands. This cannot be undone.
        </p>
        <div className="dev-actions">
          <Button variant="danger" onClick={() => setConfirmDelete(true)}>
            Delete application
          </Button>
        </div>
      </section>
      <PasswordConfirm
        open={confirmDelete}
        title={`Delete ${app.name}?`}
        description="The bot is removed from every server and everyone who authorized the app loses that access."
        requireText={app.name}
        confirmLabel="Delete application"
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
