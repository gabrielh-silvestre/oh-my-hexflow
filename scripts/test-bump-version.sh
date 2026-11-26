#!/usr/bin/env bash
# Prova o bump num workspace de brinquedo, sem tocar no repositório.
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/bump-version.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/scripts" "$tmp/crates/toy/src" "$tmp/plugin/.claude-plugin"
cp "$script" "$tmp/scripts/bump-version.sh"
cat > "$tmp/Cargo.toml" <<'TOML'
[workspace]
resolver = "3"
members = ["crates/toy"]

[workspace.package]
edition = "2024"
version = "0.1.9"
TOML
printf '[package]\nname = "toy"\nversion.workspace = true\nedition.workspace = true\n' > "$tmp/crates/toy/Cargo.toml"
: > "$tmp/crates/toy/src/lib.rs"
printf '{\n  "name": "toy",\n  "version": "0.1.9",\n  "description": "toy"\n}\n' > "$tmp/plugin/.claude-plugin/plugin.json"
(cd "$tmp" && cargo generate-lockfile --offline --quiet)

check() { # <kind> <expected>
  (cd "$tmp" && bash scripts/bump-version.sh "$1" > /dev/null)
  grep -q "^version = \"$2\"$" "$tmp/Cargo.toml" || { echo "FAIL $1: Cargo.toml is not $2" >&2; exit 1; }
  grep -A1 '^name = "toy"$' "$tmp/Cargo.lock" | grep -q "^version = \"$2\"$" || { echo "FAIL $1: Cargo.lock is not $2" >&2; exit 1; }
  grep -q "^  \"version\": \"$2\",$" "$tmp/plugin/.claude-plugin/plugin.json" || { echo "FAIL $1: plugin.json is not $2" >&2; exit 1; }
}

check patch 0.1.10
check minor 0.2.0
check major 1.0.0
if bash "$tmp/scripts/bump-version.sh" bogus 2> /dev/null; then echo "FAIL: bogus kind accepted" >&2; exit 1; fi
echo "ok"
