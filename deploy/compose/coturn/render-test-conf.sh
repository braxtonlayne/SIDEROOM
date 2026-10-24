#!/usr/bin/env bash
# Renders turnserver.prod.conf.example into a coturn config the e2e stack can run (RDY-VOX-001).
#
#   deploy/compose/coturn/render-test-conf.sh <announce-ip> <rest-secret>
#
# Writes deploy/compose/coturn/rendered/turnserver.conf and a self-signed certificate under rendered/tls/, then
# `docker compose -f deploy/compose/dev.yml --profile relay up -d coturn-relay` runs it with host networking.
# The production template is used as shipped; only these things differ from a real install:
#   1. CHANGE_ME_PUBLIC_IP and CHANGE_ME get the runner address and the REST secret (the same value goes to
#      sr-server as SR_VOICE__TURN__SECRET, which is the `voice.turn.secret` path under test);
#   2. `allowed-peer-ip=<announce-ip>` is appended: the template denies private ranges so TURN cannot be used as
#      a pivot, and the runner (and so the SFU the relay must reach) sits in one of them;
#   3. the certificate is self-signed (nothing in the test dials TLS; coturn only needs the files to start).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ip="${1:?usage: render-test-conf.sh <announce-ip> <rest-secret>}"
secret="${2:?usage: render-test-conf.sh <announce-ip> <rest-secret>}"
out="$here/rendered"
mkdir -p "$out/tls"
sed -e "s/^external-ip=CHANGE_ME_PUBLIC_IP$/external-ip=$ip/" \
    -e "s/^static-auth-secret=CHANGE_ME$/static-auth-secret=$secret/" \
    "$here/turnserver.prod.conf.example" > "$out/turnserver.conf"
if grep -v '^[[:space:]]*#' "$out/turnserver.conf" | grep -q CHANGE_ME; then
  echo "render-test-conf.sh: a CHANGE_ME placeholder is left in the template" >&2
  exit 1
fi
printf '\n# e2e only: the SFU under test announces this address, which is inside a denied private range.\nallowed-peer-ip=%s\n' "$ip" >> "$out/turnserver.conf"
# Git Bash (MSYS) would rewrite "/CN=..." into a Windows path; the variable is ignored everywhere else.
MSYS2_ARG_CONV_EXCL="/CN=" openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=chat.example.org" \
  -keyout "$out/tls/privkey.pem" -out "$out/tls/fullchain.pem" 2>/dev/null
# coturn runs as an unprivileged user in the image: let it read the key.
chmod 644 "$out/tls/privkey.pem" "$out/tls/fullchain.pem" "$out/turnserver.conf"
echo "rendered $out/turnserver.conf (external-ip=$ip)"
