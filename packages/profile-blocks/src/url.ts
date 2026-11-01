/** Only https links are followed from a profile page (no credentials in the URL). */
export const httpsUrl = (u: string | null | undefined): string | null => {
  if (!u) return null;
  try {
    const url = new URL(u);
    return url.protocol === 'https:' && !url.username && !url.password ? url.toString() : null;
  } catch {
    return null;
  }
};

/**
 * A Bitcoin or Lightning tip link of the Links block (ARC-ROOM-001). The server validates the full shape when the
 * document is saved (`sr_core::tips`); this only refuses anything that is not a plain payment URI, so a stored
 * document that was edited by hand can never put another scheme into an `href`.
 */
export const tipUri = (kind: unknown, u: string | null | undefined): string | null => {
  if (!u || u.length > 1200) return null;
  if (kind === 'btc') return /^bitcoin:[13][1-9A-HJ-NP-Za-km-z]{25,34}(\?[A-Za-z0-9=&._~%-]{1,200})?$|^bitcoin:bc1[a-z0-9]{11,71}(\?[A-Za-z0-9=&._~%-]{1,200})?$/.test(u) ? u : null;
  if (kind === 'lightning') return /^lightning:(lnbcrt|lnbc|lntbs|lntb|lnsb|lnurl1|lno1)[a-z0-9]{20,1100}$/.test(u) ? u : null;
  if (kind === 'kofi' || kind === 'patreon') {
    const h = httpsUrl(u) ? new URL(u).hostname : '';
    const ok = kind === 'kofi' ? ['ko-fi.com', 'www.ko-fi.com'] : ['patreon.com', 'www.patreon.com'];
    return ok.includes(h) ? u : null;
  }
  return null;
};
