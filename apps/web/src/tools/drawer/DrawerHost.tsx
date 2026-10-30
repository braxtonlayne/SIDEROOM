import { useEffect } from 'react';
import { useCurrentUser } from '../../lounge/data';
import { GadgetLayer } from '../gadgets/GadgetLayer';
import { startLayoutSync } from '../gadgets/sync';
import { PinRail } from '../rail/PinRail';
import '../tools.css';
import { Drawer } from './Drawer';

/**
 * UTL-SHL-001: the host. One instance, mounted by the gate (`DrawerGate.tsx`) above every signed-in route, so the
 * gadgets and the rail outlive route changes the way the player does. It loads the account's layout, keeps it in
 * step with the synced settings, and renders the drawer, the floating gadgets and the pin rail.
 */
export default function DrawerHost() {
  const me = useCurrentUser();
  useEffect(() => (me.id ? startLayoutSync(me.id) : undefined), [me.id]);
  return (
    <>
      <GadgetLayer />
      <PinRail />
      <Drawer />
    </>
  );
}
