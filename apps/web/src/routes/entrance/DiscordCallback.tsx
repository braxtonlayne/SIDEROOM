import { useEffect } from 'react';

/**
 * `/oauth/discord/callback` (Q79). The server owns the OAuth callback: the Discord redirect URI is the API
 * path, which finishes the link and 302s to `/register` or `/login`. This SPA route only forwards any
 * browser that lands here to that API path, keeping the query string.
 */
export function DiscordCallback() {
  useEffect(() => {
    window.location.replace(`/api/v1/sr/auth/discord/callback${window.location.search}`);
  }, []);
  return (
    <p role="status" className="ent-sub">
      Finishing your Discord sign-in…
    </p>
  );
}
