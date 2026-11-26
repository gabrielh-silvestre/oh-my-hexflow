#!/usr/bin/env bash
# Sobe a versão semver do [workspace.package], do plugin.json e atualiza o Cargo.lock.
# Uso: bash scripts/bump-version.sh major|minor|patch
# Só reescreve arquivos: não faz commit, tag nem PR.
set -euo pipefail

kind="${1:-}"
case "$kind" in
  major | minor | patch) ;;
  *) echo "usage: $0 major|minor|patch" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/.."

current=$(awk '/^\[/ { s = ($0 == "[workspace.package]") } s && /^version = "/ { gsub(/^version = "|"$/, ""); print; exit }' Cargo.toml)
if ! [[ "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "error: [workspace.package] version is missing or not X.Y.Z: '$current'" >&2
  exit 1
fi
major=${BASH_REMATCH[1]} minor=${BASH_REMATCH[2]} patch=${BASH_REMATCH[3]}

case "$kind" in
  major) new="$((major + 1)).0.0" ;;
  minor) new="$major.$((minor + 1)).0" ;;
  patch) new="$major.$minor.$((patch + 1))" ;;
esac

awk -v new="$new" '/^\[/ { s = ($0 == "[workspace.package]") } s && /^version = "/ { print "version = \"" new "\""; next } { print }' Cargo.toml > Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml

# A version do plugin.json fixa a instalação do plugin: anda junto com o Cargo.
sed -i "s/^\(  \"version\": \"\)[^\"]*\(\".*\)$/\1$new\2/" plugin/.claude-plugin/plugin.json

# Só reescreve a versão dos membros no lock: sem rede e sem compilar.
cargo update --workspace --offline --quiet

echo "$current -> $new"
