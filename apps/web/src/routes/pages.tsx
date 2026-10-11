import type { ReactNode } from 'react';

function Page({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <main>
      <h1>{title}</h1>
      {children}
    </main>
  );
}

export const Channels = () => <Page title="Lounge" />;
export const Cinema = () => <Page title="Cinema" />;
export const Library = () => <Page title="Library" />;
export const Admin = () => <Page title="Admin" />;
export const Developers = () => <Page title="Developers" />;
export const Support = () => <Page title="Support" />;
