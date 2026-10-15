import { useEffect, useState } from 'react';
import { getConfig } from '../../support/api';

/** Whether the operator turned donations on (`GET /donations/config`); asked once per page load. */
let supportOn: Promise<boolean> | null = null;
export const resetSupportToggle = () => {
  supportOn = null;
};
export function useSupportEnabled(): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    let live = true;
    supportOn ??= getConfig().then(
      (c) => c.enabled,
      () => false,
    );
    void supportOn.then((v) => live && setOn(v));
    return () => {
      live = false;
    };
  }, []);
  return on;
}
