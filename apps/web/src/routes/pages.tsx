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
export const Support = () => <Page title="Support" />;
