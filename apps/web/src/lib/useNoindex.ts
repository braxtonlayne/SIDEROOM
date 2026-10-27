import { useEffect } from 'react';

// OPS-IDX-001 (D-384): only the landing page is indexable. The static index.html carries no robots meta (it is the
// landing page's document); the root layout calls this with the current pathname and every route but `/` gets
// <meta name="robots" content="noindex"> for as long as it is shown. robots.txt and X-Robots-Tag are the other layers.
export const isIndexablePath = (pathname: string): boolean => pathname === '/';

export function useNoindex(pathname: string): void {
  const indexable = isIndexablePath(pathname);
  useEffect(() => {
    if (indexable) return;
    const meta = document.createElement('meta');
    meta.name = 'robots';
    meta.content = 'noindex';
    meta.dataset.srNoindex = '1';
    document.head.appendChild(meta);
    return () => meta.remove();
  }, [indexable]);
}
