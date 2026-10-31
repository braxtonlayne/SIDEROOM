#!/bin/sh
# Fetch the pinned emulator core files and verify their SHA-256 (ACT-EMU-003).
#
#   deploy/scripts/fetch-emu-cores.sh [--dest DIR] [--base-url URL] [--lock FILE] [--force] [--verify-only]
#
# The pins (path, URL, SHA-256, size, licence) are in deploy/emu-cores.lock and are shown as a table in
# deploy/EMULATOR.md. Each file is downloaded to a .part file next to its target, hashed, and moved into place only
# when the digest and size match; a wrong file is deleted and the script exits 1, so nothing unverified is ever served.
# Files that already match are skipped, so the script is safe to repeat. Core binaries are never committed.
#
#   --dest DIR       the directory the web server serves the cores from
#                    (default: $SR_EMU_CORES_DIR, else apps/web/public/emu/cores in this checkout)
#   --base-url URL   fetch from this mirror instead of the pinned CDN; https, or http on 127.0.0.1/localhost (tests)
#   --lock FILE      another pin list (tests)
#   --force          download again even when the file already matches
#   --verify-only    download nothing; exit 1 when a file is missing or differs
#
# Needs curl and sha256sum (or shasum). Downloads use https only, with https-only redirects and a size cap.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"

pinned_base=https://cdn.emulatorjs.org/4.2.3/data/
dest=${SR_EMU_CORES_DIR:-$here/../../apps/web/public/emu/cores}
lock=$here/../emu-cores.lock
base=
force=0
verify_only=0

while [ $# -gt 0 ]; do
  case $1 in
    --dest) [ $# -ge 2 ] || die "--dest needs a directory"; dest=$2; shift ;;
    --base-url) [ $# -ge 2 ] || die "--base-url needs a URL"; base=$2; shift ;;
    --lock) [ $# -ge 2 ] || die "--lock needs a file"; lock=$2; shift ;;
    --force) force=1 ;;
    --verify-only) verify_only=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1 (try --help)" ;;
  esac
  shift
done

[ -f "$lock" ] || die "pin list $lock not found"
proto='=https'
if [ -n "$base" ]; then
  case $base in
    https://*) ;;
    http://127.0.0.1|http://127.0.0.1:*|http://127.0.0.1/*|http://localhost|http://localhost:*|http://localhost/*|'http://[::1]'*) proto='=http,https' ;;
    *) die "--base-url must be https (http only for 127.0.0.1, localhost or [::1])" ;;
  esac
  base=${base%/}
fi
if [ "$verify_only" != "1" ]; then need curl; fi
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
  die "sha256sum or shasum is required"
fi

tab=$(printf '\t')
fetched=0
bad=0
# The lock is read on fd 3 so that curl never sees the list on its stdin.
while IFS=$tab read -r rel url sha bytes licence <&3; do
  case $rel in ''|'#'*) continue ;; esac
  case $rel in
    /*|..|../*|*/..|*/../*|*\\*) die "unsafe destination '$rel' in $lock" ;;
  esac
  case $url in "$pinned_base"*) ;; *) die "url for $rel does not start with $pinned_base" ;; esac
  case $sha in
    *[!0-9a-f]*) die "sha256 for $rel is not lower-case hex" ;;
  esac
  [ ${#sha} -eq 64 ] || die "sha256 for $rel is not 64 digits"
  case $bytes in ''|*[!0-9]*) die "size for $rel is not a number" ;; esac

  target=$dest/$rel
  if [ "$force" != "1" ] && [ -f "$target" ] && [ "$(sha256_of "$target")" = "$sha" ] \
    && [ "$(wc -c < "$target" | tr -d ' ')" = "$bytes" ]; then
    echo "ok       $rel ($bytes bytes, $licence)"
    continue
  fi
  if [ "$verify_only" = "1" ]; then
    echo "MISSING  $rel (absent or digest differs)"
    bad=$((bad + 1))
    continue
  fi

  fetch_url=$url
  if [ -n "$base" ]; then fetch_url=$base/${url#"$pinned_base"}; fi
  mkdir -p "$(dirname "$target")"
  part=$target.part
  rm -f "$part"
  # --max-filesize leaves a small margin so a longer file reaches the digest check instead of being cut short.
  if ! curl -fL -sS --proto "$proto" --proto-redir '=https' --max-filesize $((bytes + 4096)) --max-time 300 \
    -o "$part" "$fetch_url"; then
    rm -f "$part"
    die "download failed for $fetch_url"
  fi
  got=$(sha256_of "$part")
  size=$(wc -c < "$part" | tr -d ' ')
  if [ "$got" != "$sha" ] || [ "$size" != "$bytes" ]; then
    rm -f "$part"
    die "digest mismatch for $rel: expected $sha ($bytes bytes), got $got ($size bytes); the file was discarded"
  fi
  mv -f "$part" "$target"
  echo "fetched  $rel ($bytes bytes, $licence)"
  fetched=$((fetched + 1))
done 3< "$lock"

if [ "$bad" -gt 0 ]; then
  die "$bad pinned file(s) missing or different in $dest"
fi
echo "emulator cores: $fetched downloaded, the rest already verified ($dest)"
