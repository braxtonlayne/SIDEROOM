#!/usr/bin/env bash
# Install Playwright browsers with a bounded retry (OPS-CI-001). The CDN download failed once on PR #67 ("Failed to
# download Firefox") and took the whole job with it; a second try nearly always works.
#
#   scripts/playwright-install.sh chromium firefox webkit
#
# Four attempts, 15 s, 45 s then 135 s apart; the last failure is the exit status. Needs pnpm and the installed workspace.
#
# Each attempt is also capped (PLAYWRIGHT_INSTALL_ATTEMPT_SECS, default 360 s): `--with-deps` runs apt-get, and an
# Ubuntu mirror that stopped answering mid-download hung the step until the job's own timeout cancelled it
# (arena3d-replay on PRs #88 and #91: "The operation was canceled" in the middle of the package list), so the retry
# never ran. An attempt that hangs is now killed and retried like a failed one. apt keeps every package it finished in
# its archive cache, so on a slow mirror (40 KB/s on PR #92) each retry carries on where the last one stopped.
set -u
attempts=4
delay=15
cap="${PLAYWRIGHT_INSTALL_ATTEMPT_SECS:-360}"
for ((i = 1; i <= attempts; i++)); do
  if timeout --kill-after=15 "$cap" pnpm --filter @sideroom/e2e exec playwright install --with-deps "$@"; then
    exit 0
  fi
  # A killed attempt can leave its apt-get (started under sudo) holding the package lock: stop it and let dpkg finish
  # anything half-configured. CI runners have passwordless sudo; elsewhere `sudo -n` fails quietly and nothing happens.
  sudo -n pkill -x apt-get 2>/dev/null || true
  sudo -n dpkg --configure -a 2>/dev/null || true
  if ((i < attempts)); then
    echo "playwright install failed or ran over ${cap}s (attempt $i of $attempts); retrying in ${delay}s" >&2
    sleep "$delay"
    delay=$((delay * 3))
  fi
done
echo "playwright install failed after $attempts attempts" >&2
exit 1
