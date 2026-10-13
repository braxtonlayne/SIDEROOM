import type { ReactNode } from 'react';

function Page({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <main>
      <h1>{title}</h1>
      {children}
    </main>
  );
}

export const Cinema = () => <Page title="Cinema" />;
export const Library = () => <Page title="Library" />;
export const Developers = () => <Page title="Developers" />;
export const Support = () => <Page title="Support" />;
