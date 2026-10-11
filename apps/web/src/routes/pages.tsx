import { Link, useParams } from '@tanstack/react-router';
import type { ReactNode } from 'react';

function Page({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <main>
      <h1>{title}</h1>
      {children}
    </main>
  );
}

export const Landing = () => (
  <Page title="SIDE ROOM.">
    <p>A private corner of the internet.</p>
    <Link to="/login">Member access</Link>
  </Page>
);
export const Login = () => <Page title="Come on in." />;
export const Join = () => {
  const { code } = useParams({ strict: false }) as { code?: string };
  return <Page title="Join">Invite code: {code}</Page>;
};
export const Invite = () => {
  const { code } = useParams({ strict: false }) as { code?: string };
  return (
    <Page title="Server invite">
      <Link to="/login" search={{ next: `/invite/${code ?? ''}` }}>
        Log in to accept
      </Link>
    </Page>
  );
};
export const Channels = () => <Page title="Lounge" />;
export const Cinema = () => <Page title="Cinema" />;
export const Library = () => <Page title="Library" />;
export const Admin = () => <Page title="Admin" />;
export const Developers = () => <Page title="Developers" />;
export const Support = () => <Page title="Support" />;
