import { Spinner, useToast } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import type { RawComponent } from '../../store/types';
import { useChannels, useMembers, useRoles, type Snowflake } from '../data';
import { isHttpUrl } from './format';

export type { RawComponent };

const STYLE = ['', 'primary', 'secondary', 'success', 'danger', 'link'] as const;

/** Discord waits three seconds for an app to answer an interaction before it shows the failure. */
export const interactionConfig = { timeoutMs: 3000 };

/** Select menu kinds (Discord component types): string, user, role, mentionable, channel. */
const SELECT_TYPES = [3, 5, 6, 7, 8];

/**
 * Buttons and select menus on app messages (LNG-MSGUI-022). Link buttons open their URL; everything else
 * shows a loading state and then "This interaction failed" because the interaction endpoint is not part of
 * the server yet. User, role, mentionable and channel selects list the server's own members, roles and
 * channels (every non-category channel for the channel select).
 */
export function ComponentRows({
  rows,
  guildId,
}: {
  rows: RawComponent[];
  guildId?: Snowflake | undefined;
}) {
  const toast = useToast();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const [busy, setBusy] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  /** Starts an interaction: the control shows a spinner until the (never arriving) answer times out. */
  const interact = (key: string) => {
    setBusy(key);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setBusy(null);
      toast.show({ title: 'This interaction failed', variant: 'danger', duration: 3000 });
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
      {rows
        .filter((r) => r.type === 1)
        .slice(0, 5)
        .map((row, i) => (
          <div key={i} className="lng-component-row" role="group" aria-label="Message actions">
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
                      {c.label ?? 'Link'}
                    </a>
                  ) : null;
                }
                return (
                  <button
                    key={j}
                    type="button"
                    className="lng-cbtn"
                    data-style={style}
                    disabled={c.disabled || busy === key}
                    aria-busy={busy === key || undefined}
                    onClick={() => interact(key)}
                  >
                    {busy === key && <Spinner label="Waiting for the app" />}
                    {c.label ?? 'Button'}
                  </button>
                );
              }
              if (SELECT_TYPES.includes(c.type)) {
                const label = c.placeholder ?? 'Select an option';
                return (
                  <span key={j} className="lng-cselect-wrap">
                    <select
                      className="lng-cselect"
                      aria-label={label}
                      data-select-type={c.type}
                      disabled={c.disabled || busy === key}
                      aria-busy={busy === key || undefined}
                      value=""
                      onChange={() => interact(key)}
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
                    {busy === key && <Spinner label="Waiting for the app" />}
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
