import { useEffect } from 'react';
import { getMe } from '../api/entrance';
import { enterApp } from './enterApp';

/**
 * ENT-UI-023: members who hit `/` or `/login` go straight to the app (a document load into `/app`, which
 * sends them to their last section). A public surface knows no section path.
 */
export function useRedirectIfLoggedIn() {
  useEffect(() => {
    let live = true;
    getMe()
      .then(() => {
        if (live) enterApp();
      })
      .catch(() => {
        /* not logged in (or API unreachable): stay put */
      });
    return () => {
      live = false;
    };
  }, []);
}
