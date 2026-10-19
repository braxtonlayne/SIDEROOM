// JUnit XML for the SDK compatibility run (M-16). One <testsuite> per library, one <testcase> per tier-2 check.

const esc = (s) =>
  String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    // Characters XML 1.0 cannot carry (control characters other than tab, LF, CR).
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, '?');

/**
 * @param {{ library: string, version: string, cases: { name: string, ok: boolean, message?: string, seconds?: number }[] }} run
 * @returns {string}
 */
export function toJunit(run) {
  const failures = run.cases.filter((c) => !c.ok).length;
  const time = run.cases.reduce((t, c) => t + (c.seconds ?? 0), 0).toFixed(3);
  const name = `${run.library} ${run.version}`;
  const body = run.cases
    .map((c) => {
      const open = `<testcase classname="${esc(run.library)}" name="${esc(c.name)}" time="${(c.seconds ?? 0).toFixed(3)}"`;
      return c.ok
        ? `${open}/>`
        : `${open}><failure message="${esc(c.message ?? 'failed')}">${esc(c.message ?? 'failed')}</failure></testcase>`;
    })
    .join('\n    ');
  return `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="sdk-compat" tests="${run.cases.length}" failures="${failures}" time="${time}">
  <testsuite name="${esc(name)}" tests="${run.cases.length}" failures="${failures}" time="${time}">
    ${body}
  </testsuite>
</testsuites>
`;
}
