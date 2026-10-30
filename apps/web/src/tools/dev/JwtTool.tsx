import { useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { jwtDecode } from './logic';
import { CopyButton, Field, Problem, Root } from './parts';

/**
 * JWT decode. It reads the header and payload; it never checks the signature, and the banner says so. Nothing
 * is sent anywhere: the token stays in this page.
 */
export default function JwtTool() {
  const t = useT();
  const [token, setToken] = useState('');
  const decoded = useMemo(() => (token.trim() === '' ? null : jwtDecode(token)), [token]);
  const header = decoded ? JSON.stringify(decoded.header, null, 2) : '';
  const payload = decoded ? JSON.stringify(decoded.payload, null, 2) : '';

  return (
    <Root label={t('tools.jwt.name')}>
      <p className="ud-warn" role="note">
        {t('tools.jwt.banner')}
      </p>
      <Field label={t('tools.jwt.token')}>
        <textarea
          className="ud-mono"
          value={token}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={token.trim() !== '' && decoded === null}
          onChange={(e) => setToken(e.target.value)}
        />
      </Field>
      {token.trim() !== '' && decoded === null ? <Problem>{t('tools.jwt.invalid')}</Problem> : null}
      {decoded ? (
        <>
          <h4>{t('tools.jwt.header')}</h4>
          <pre className="ud-out ud-mono">{header}</pre>
          <h4>{t('tools.jwt.payload')}</h4>
          <pre className="ud-out ud-mono">{payload}</pre>
          {decoded.times.length > 0 ? (
            <ul className="ud-history">
              {decoded.times.map((c) => (
                <li key={c.claim}>
                  <code>{c.claim}</code> {c.iso}
                </li>
              ))}
            </ul>
          ) : null}
          <p className="ud-muted">{t('tools.jwt.notVerified')}</p>
          <div className="ud-row">
            <CopyButton text={payload} />
          </div>
        </>
      ) : null}
    </Root>
  );
}
