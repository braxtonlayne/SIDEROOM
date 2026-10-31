#!/usr/bin/env bash
# Install Playwright browsers with a bounded retry (OPS-CI-001). The CDN download failed once on PR #67 ("Failed to
# download Firefox") and took the whole job with it; a second try nearly always works.
#
#   scripts/playwright-install.sh chromium firefox webkit
#
# Three attempts, 15 s then 45 s apart; the last failure is the exit status. Needs pnpm and the installed workspace.
set -u
attempts=3
delay=15
for ((i = 1; i <= attempts; i++)); do
  if pnpm --filter @sideroom/e2e exec playwright install --with-deps "$@"; then
    exit 0
  fi
  if ((i < attempts)); then
    echo "playwright install failed (attempt $i of $attempts); retrying in ${delay}s" >&2
    sleep "$delay"
    delay=$((delay * 3))
  fi
done
echo "playwright install failed after $attempts attempts" >&2
exit 1
