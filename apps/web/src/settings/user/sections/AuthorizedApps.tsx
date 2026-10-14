import { Card, SectionTitle, Unavailable } from '../parts';

/**
 * Authorized Apps (LNG-USET-021): the OAuth2 grants the account has given to apps, each with Deauthorize.
 * The server has no OAuth2 provider or grants list yet (bots use tokens, `auth: "bot" | "bearer"` on
 * `GET /users/@me` is the only trace), so this page says so instead of showing an empty fake list.
 */
export function AuthorizedAppsSection() {
  return (
    <>
      <SectionTitle>Authorized Apps</SectionTitle>
      <Card
        id="us-apps"
        title="Apps with access to your account"
        description="Apps you signed in to with SIDE ROOM show up here, with the permissions they have and a way to remove them."
      >
        <Unavailable>
          The server cannot authorize third-party apps yet (no OAuth2 grants to list or revoke), so
          no app can have access to your account. This list will appear once it can.
        </Unavailable>
      </Card>
    </>
  );
}
