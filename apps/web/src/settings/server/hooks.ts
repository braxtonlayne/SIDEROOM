import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  createContext,
} from 'react';
import { useCurrentUser, useGuild, useStore } from '../../store';
import { memberKey } from '../../store/reducer';
import { onDispatch } from '../../store/events';
import { ALL_BITS, PERM, has, toBig } from './permissions';
import * as api from './api';

// ---- fetched resources ----

export interface Resource<T> {
  data: T | undefined;
  error: string | null;
  loading: boolean;
  reload: () => Promise<void>;
  /** Replace the data locally (optimistic updates and mutation results). */
  set: (next: T | ((cur: T | undefined) => T)) => void;
}

/**
 * Fetch on mount and whenever `key` changes. `reload` refetches (after mutations); a stale response from a
 * previous key is dropped.
 */
export function useResource<T>(key: string, fetcher: () => Promise<T>): Resource<T> {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const fetchRef = useRef(fetcher);
  fetchRef.current = fetcher;
  const keyRef = useRef(key);
  keyRef.current = key;
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    const forKey = keyRef.current;
    try {
      const next = await fetchRef.current();
      if (alive.current && keyRef.current === forKey) {
        setData(next);
        setError(null);
      }
    } catch (e) {
      if (alive.current && keyRef.current === forKey)
        setError(api.errorText(e, 'Could not load this.'));
    } finally {
      if (alive.current && keyRef.current === forKey) setLoading(false);
    }
  }, []);

  useEffect(() => {
    setLoading(true);
    setData(undefined);
    setError(null);
    void reload();
  }, [key, reload]);

  const set = useCallback((next: T | ((cur: T | undefined) => T)) => {
    setData((cur) => (typeof next === 'function' ? (next as (c: T | undefined) => T)(cur) : next));
  }, []);

  return useMemo(
    () => ({ data, error, loading, reload, set }),
    [data, error, loading, reload, set],
  );
}

/** Run `fn` when a gateway dispatch with one of `events` arrives for this guild. */
export function useGuildEvents(guildId: string, events: string[], fn: () => void) {
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const wanted = events.join('|');
  useEffect(() => {
    const set = new Set(wanted.split('|'));
    return onDispatch((event, data) => {
      if (!set.has(event)) return;
      const d = data as { guild_id?: string; id?: string } | null;
      const gid = d?.guild_id ?? (event.startsWith('GUILD_UPDATE') ? d?.id : undefined);
      if (gid === undefined || gid === guildId) fnRef.current();
    });
  }, [guildId, wanted]);
}

// ---- viewer permissions ----

export interface Viewer {
  ready: boolean;
  userId: string | null;
  isOwner: boolean;
  /** Base guild permissions (owner and Administrator hold every bit). */
  perms: bigint;
  /** Highest role position the viewer holds; Infinity for the owner. */
  topPosition: number;
  can: (name: keyof typeof PERM | string) => boolean;
  /** May the viewer edit/assign/delete this role (MANAGE_ROLES and strictly above it, owner always)? */
  canManageRole: (position: number, isEveryone?: boolean) => boolean;
  /** Does the viewer hold every bit in `bits` (so they may grant it)? */
  canGrant: (bits: bigint) => boolean;
}

export function useViewer(guildId: string): Viewer {
  const me = useCurrentUser();
  const { guild, roles } = useGuild(guildId);
  const member = useStore((s) => (me ? s.members[memberKey(guildId, me.id)] : undefined));
  const fetched = useRef(false);

  // The member list may not be loaded (deep link): fetch only our own membership.
  useEffect(() => {
    if (!me || !guild || member || fetched.current) return;
    fetched.current = true;
    api
      .getMember(guildId, me.id)
      .then((m) =>
        useStore.getState().applyDispatch('GUILD_MEMBER_UPDATE', {
          guild_id: guildId,
          user: m.user,
          nick: m.nick,
          roles: m.roles,
          joined_at: m.joined_at,
        }),
      )
      .catch(() => undefined);
  }, [me, guild, member, guildId]);

  return useMemo<Viewer>(() => {
    const ready = !!me && !!guild && roles.length > 0 && !!member;
    const isOwner = !!me && !!guild && guild.owner_id === me.id;
    let perms = 0n;
    let top = 0;
    if (ready) {
      const everyone = roles.find((r) => r.id === guildId);
      perms = toBig(everyone?.permissions);
      for (const rid of member.roles) {
        const r = roles.find((x) => x.id === rid);
        if (!r) continue;
        perms |= toBig(r.permissions);
        top = Math.max(top, r.position);
      }
      if (has(perms, PERM.ADMINISTRATOR!)) perms = ALL_BITS;
    }
    if (isOwner) {
      perms = ALL_BITS;
      top = Number.POSITIVE_INFINITY;
    }
    const can = (name: string) => {
      const bit = PERM[name];
      return bit !== undefined && has(perms, bit);
    };
    return {
      ready: ready || isOwner,
      userId: me?.id ?? null,
      isOwner,
      perms,
      topPosition: top,
      can,
      canManageRole: (position, isEveryone) =>
        can('MANAGE_ROLES') && (isEveryone ? true : isOwner || top > position),
      canGrant: (bits) => (perms & bits) === bits,
    };
  }, [me, guild, roles, member, guildId]);
}

// ---- unsaved-changes bar ----

export interface FormHandle {
  dirty: boolean;
  saving: boolean;
  save: () => void;
  reset: () => void;
}
export const FormContext = createContext<(f: FormHandle | null) => void>(() => undefined);

/** Publish the section's dirty state to the shell's unsaved-changes bar. */
export function useSettingsForm(h: FormHandle) {
  const publish = useContext(FormContext);
  const latest = useRef(h);
  latest.current = h;
  useEffect(() => {
    publish({
      dirty: h.dirty,
      saving: h.saving,
      save: () => latest.current.save(),
      reset: () => latest.current.reset(),
    });
  }, [publish, h.dirty, h.saving]);
  useEffect(() => () => publish(null), [publish]);
}

// ---- misc ----

export const hexFromInt = (n: number) => `#${n.toString(16).padStart(6, '0')}`;
export const intFromHex = (h: string) => parseInt(h.replace('#', ''), 16) || 0;

// ---- drafts ----

export interface Draft<T extends object> {
  /** Baseline with the user's edits applied. */
  value: T;
  set: <K extends keyof T>(key: K, v: T[K]) => void;
  dirty: boolean;
  /** Only the keys that differ from the baseline. */
  changed: Partial<T>;
  reset: () => void;
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Edits kept as overrides on top of a baseline, so a refetch (someone else saved) updates the fields the
 * user has not touched and never clobbers the ones they have.
 */
export function useDraft<T extends object>(base: T): Draft<T> {
  const [edits, setEdits] = useState<Partial<T>>({});
  const value = useMemo(() => ({ ...base, ...edits }), [base, edits]);
  const changed = useMemo(() => {
    const out: Partial<T> = {};
    for (const k of Object.keys(edits) as (keyof T)[]) {
      if (!same(edits[k], base[k])) out[k] = edits[k] as T[keyof T];
    }
    return out;
  }, [edits, base]);
  const set = useCallback(
    <K extends keyof T>(key: K, v: T[K]) => setEdits((e) => ({ ...e, [key]: v })),
    [],
  );
  const reset = useCallback(() => setEdits({}), []);
  return { value, set, dirty: Object.keys(changed).length > 0, changed, reset };
}
