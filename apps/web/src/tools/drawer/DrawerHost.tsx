import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useStore } from '../../store';
import { GadgetLayer } from '../gadgets/GadgetLayer';
import { startLayoutSync } from '../gadgets/sync';
import { PinRail } from '../rail/PinRail';
import '../tools.css';
import { Drawer } from './Drawer';

/**
 * The signed-in account's id. The store knows it once the Lounge has connected; on any other section after a
 * reload (Settings, Cinema, Library) it asks once.
 */
function useAccountId(): string | null {
  const fromStore = useStore((s) => s.currentUserId);
  const [fetched, setFetched] = useState<string | null>(null);
  useEffect(() => {
    if (fromStore) return;
    let live = true;
    apiFetch<{ id?: string }>('/users/@me', { retries: 0 })
      .then((u) => {
        if (live && typeof u.id === 'string') setFetched(u.id);
      })
      .catch(() => {
        /* signed out or offline: no layout until the next visit */
      });
    return () => {
      live = false;
    };
  }, [fromStore]);
  return fromStore ?? fetched;
}

/**
 * UTL-SHL-001: the host. One instance, mounted by the gate (`DrawerGate.tsx`) above every signed-in route, so the
 * gadgets and the rail outlive route changes the way the player does. It loads the account's layout, keeps it in
 * step with the synced settings, and renders the drawer, the floating gadgets and the pin rail.
 */
export default function DrawerHost() {
  const account = useAccountId();
  useEffect(() => (account ? startLayoutSync(account) : undefined), [account]);
  return (
    <>
      <GadgetLayer />
      <PinRail />
      <Drawer />
    </>
  );
}
