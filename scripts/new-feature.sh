#!/usr/bin/env bash
# Scaffold a feature module (FTR-CORE-001): `just new-feature lounge.my_thing`.
#
# Creates crates/sr-features/src/modules/<name>.rs from the template, declares the
# module and registers it in modules/mod.rs. Everything else (settings form, per-server
# toggle, runtime delivery) is generic. See docs/dev/writing-a-feature.md.
set -euo pipefail

id="${1:-}"
if ! [[ "$id" =~ ^[a-z]+\.[a-z][a-z0-9_]*$ ]]; then
  echo "usage: new-feature <section>.<snake_name>   e.g. lounge.birthday_roles" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mods="$root/crates/sr-features/src/modules"
name="${id#*.}"
file="$mods/$name.rs"

if [ -e "$file" ]; then
  echo "error: $file already exists" >&2
  exit 1
fi
if grep -q "\"$id\"" "$mods"/*.rs; then
  echo "error: feature id $id is already used" >&2
  exit 1
fi

# snake_case -> CamelCase, and a human title.
camel="$(echo "$name" | awk -F_ '{ for (i = 1; i <= NF; i++) printf "%s%s", toupper(substr($i,1,1)), substr($i,2) }')"
title="$(echo "$name" | awk -F_ '{ for (i = 1; i <= NF; i++) printf "%s%s", (i>1?" ":""), (i==1?toupper(substr($i,1,1)) substr($i,2):$i) }')"
const="$(echo "${name}_ID" | tr '[:lower:]' '[:upper:]')"

sed -e "s/__ID__/$id/g" -e "s/__TYPE__/$camel/g" -e "s/__CONST__/$const/g" -e "s/__NAME__/$title/g" \
  "$root/crates/sr-features/templates/feature.rs.tmpl" > "$file"

# Declare the module and add it to register_all (marker comments in mod.rs).
sed -i "s|^// new-feature:mod|pub mod $name;\n// new-feature:mod|" "$mods/mod.rs"
sed -i "s|^        // new-feature:register|        $name::$camel,\n        // new-feature:register|" "$mods/mod.rs"

echo "created $file"
echo "next: fill in the TODOs, then cargo test -p sr-features $name"
