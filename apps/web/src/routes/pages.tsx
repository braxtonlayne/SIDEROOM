import type { ReactNode } from 'react';
import { useT } from '../i18n';

function Page({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <main>
      <h1>{title}</h1>
      {children}
    </main>
  );
}

export const Cinema = () => {
  const t = useT();
  return <Page title={t('routes.pages.cinema')} />;
};
export const Support = () => {
  const t = useT();
  return <Page title={t('routes.pages.support')} />;
};
