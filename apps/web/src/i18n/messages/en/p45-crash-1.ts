// OPS-CRASH-001 (wave 45-A): the page the last error boundary shows after a render failure.
export default {
  'crash.boundary.title': 'Something went wrong',
  'crash.boundary.body':
    'This page hit an unexpected problem. A short, anonymous report (which part of the app failed and which build you run, never your messages or files) was sent to the staff so they can fix it. Reloading usually helps.',
  'crash.boundary.reload': 'Reload',
} satisfies Record<string, string>;
