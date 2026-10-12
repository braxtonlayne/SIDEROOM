import { useRouter } from '@tanstack/react-router';
import { useEffect } from 'react';
import { getMe } from '../api/entrance';
import { useUiStore } from '../state/session';

/** ENT-UI-023: members who hit `/` or `/login` go straight to their last section. */
export function useRedirectIfLoggedIn() {
  const router = useRouter();
  useEffect(() => {
    let live = true;
    getMe()
      .then(() => {
        if (live) router.history.replace(useUiStore.getState().lastSection);
      })
      .catch(() => {
        /* not logged in (or API unreachable): stay put */
      });
    return () => {
      live = false;
    };
  }, [router]);
}
