import { Spinner, useToast } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import type { RawComponent } from '../../store/types';
import { useChannels, useMembers, useRoles, type Snowflake } from '../data';
import { newNonce, sendInteraction } from '../apps/interactions';
import { useReportOutcome } from '../apps/useReportOutcome';
import { isHttpUrl } from './format';
import { parseStaffButton, runStaffButton } from './staffActions';

export type { RawComponent };

const STYLE = ['', 'primary', 'secondary', 'success', 'danger', 'link'] as const;

/** Discord waits three seconds for an app to answer an interaction before it shows the failure. */
export const interactionConfig = { timeoutMs: 3000 };

/** Select menu kinds (Discord component types): string, user, role, mentionable, channel. */
const SELECT_TYPES = [3, 5, 6, 7, 8];

/** Where the components sit; with it, presses are real interactions (`POST /interactions`, type 3). */
export interface ComponentContext {
  channelId: Snowflake;
  messageId: Snowflake;
}

const isStaff = (customId: string | undefined) => parseStaffButton(customId) !== null;

/**
 * Buttons and select menus on app messages (LNG-MSGUI-022). Link buttons open their URL. Everything else
 * shows a loading state while the app answers the interaction (DEV-BOT-004), then the app's reply arrives
 * as a normal message update; if the app stays silent or the server refuses, "This interaction failed".
 * Without a `context` (no message to point at) a press only shows that waiting state and the failure.
 * User, role, mentionable and channel selects list the server's own members, roles and channels (every
 * non-category channel for the channel select).
 */
export function ComponentRows({
  rows,
  guildId,
  context,
}: {
  rows: RawComponent[];
  guildId?: Snowflake | undefined;
  context?: ComponentContext | undefined;
}) {
  const t = useT();
  const toast = useToast();
  const report = useReportOutcome();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const [busy, setBusy] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  // Staff card buttons (`sr:` custom ids) are answered once; afterwards they stay disabled.
  const [staffDone, setStaffDone] = useState<string | null>(null);

  const staffPress = async (key: string, customId: string | undefined) => {
    const button = parseStaffButton(customId);
    if (!button || !context) return;
    setBusy(key);
    try {
      const out = await runStaffButton(button, context);
      setStaffDone(out.message ?? t('lounge.components.staffDone'));
      // A rejection needs a reason, so the card sends the reviewer to the review page's reject dialog.
      if (out.link?.startsWith('/')) window.location.assign(out.link);
    } catch (e) {
      // D-198: accepting an appointment needs an authenticator app or a passkey first.
      if (e instanceof ApiError && e.body?.code === 900014) {
        toast.show({
          title: t('lounge.components.staffNeedsSecondFactor'),
          description: (
            <a href="/settings/user/account">{t('lounge.components.staffOpenSecurity')}</a>
          ),
          variant: 'warning',
          duration: 0,
        });
        return;
      }
      toast.show({
        title:
          e instanceof ApiError && e.body?.message
            ? e.body.message
            : t('lounge.components.staffFailed'),
        variant: 'danger',
      });
    } finally {
      setBusy(null);
    }
  };

  /** Starts an interaction and keeps the control busy until the app answered or the wait ran out. */
  const interact = (key: string, c: RawComponent, values?: string[]) => {
    setBusy(key);
    clearTimeout(timer.current);
    if (context && guildId && c.custom_id) {
      void sendInteraction(
        {
          type: 3,
          channel_id: context.channelId,
          guild_id: guildId,
          message_id: context.messageId,
          nonce: newNonce(),
          data: {
            custom_id: c.custom_id,
            component_type: c.type,
            ...(values ? { values } : {}),
          },
        },
        interactionConfig.timeoutMs,
      ).then((o) => {
        setBusy((cur) => (cur === key ? null : cur));
        report(o);
      });
      return;
    }
    timer.current = setTimeout(() => {
      setBusy(null);
      toast.show({
        title: t('lounge.components.interactionFailed'),
        variant: 'danger',
        duration: 3000,
      });
    }, interactionConfig.timeoutMs);
  };

  const optionsFor = (c: RawComponent): { value: string; label: string }[] => {
    switch (c.type) {
      case 5:
        return members.map((m) => ({ value: m.user.id, label: m.nick || m.user.displayName }));
      case 6:
        return roles.filter((r) => r.id !== guildId).map((r) => ({ value: r.id, label: r.name }));
      case 7:
        return [
          ...members.map((m) => ({ value: m.user.id, label: `@${m.nick || m.user.displayName}` })),
          ...roles
            .filter((r) => r.id !== guildId)
            .map((r) => ({ value: r.id, label: `@${r.name}` })),
        ];
      case 8:
        return channels
          .filter((ch) => ch.type !== 'category')
          .map((ch) => ({ value: ch.id, label: `#${ch.name}` }));
      default:
        return (c.options ?? []).slice(0, 25).map((o) => ({ value: o.value, label: o.label }));
    }
  };

  return (
    <div className="lng-components">
      {staffDone !== null && (
        <p className="lng-muted" role="status">
          {staffDone}
        </p>
      )}
      {rows
        .filter((r) => r.type === 1)
        .slice(0, 5)
        .map((row, i) => (
          <div
            key={i}
            className="lng-component-row"
            role="group"
            aria-label={t('lounge.components.messageActions')}
          >
            {(row.components ?? []).slice(0, 5).map((c, j) => {
              const key = `${i}:${j}`;
              if (c.type === 2) {
                const style = STYLE[c.style ?? 1] ?? 'primary';
                if (style === 'link') {
                  return isHttpUrl(c.url) ? (
                    <a
                      key={j}
                      className="lng-cbtn"
                      data-style="link"
                      href={c.url}
                      target="_blank"
                      rel="noreferrer noopener"
                      aria-disabled={c.disabled || undefined}
                    >
                      {c.label ?? t('lounge.components.linkDefault')}
                    </a>
                  ) : null;
                }
                return (
                  <button
                    key={j}
                    type="button"
                    className="lng-cbtn"
                    data-style={style}
                    disabled={
                      c.disabled || busy === key || (staffDone !== null && isStaff(c.custom_id))
                    }
                    aria-busy={busy === key || undefined}
                    onClick={() =>
                      isStaff(c.custom_id) && context
                        ? void staffPress(key, c.custom_id)
                        : interact(key, c)
                    }
                  >
                    {busy === key && <Spinner label={t('lounge.components.waitingForApp')} />}
                    {c.label ?? t('lounge.components.buttonDefault')}
                  </button>
                );
              }
              if (SELECT_TYPES.includes(c.type)) {
                const label = c.placeholder ?? t('lounge.components.selectPlaceholder');
                return (
                  <span key={j} className="lng-cselect-wrap">
                    <select
                      className="lng-cselect"
                      aria-label={label}
                      data-select-type={c.type}
                      disabled={c.disabled || busy === key}
                      aria-busy={busy === key || undefined}
                      value=""
                      onChange={(e) => interact(key, c, [e.target.value])}
                    >
                      <option value="" disabled>
                        {label}
                      </option>
                      {optionsFor(c).map((o) => (
                        <option key={o.value} value={o.value}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                    {busy === key && <Spinner label={t('lounge.components.waitingForApp')} />}
                  </span>
                );
              }
              return null;
            })}
          </div>
        ))}
    </div>
  );
}
