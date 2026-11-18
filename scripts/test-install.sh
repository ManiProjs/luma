#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

FIXTURES="$TEST_ROOT/fixtures"
FAKE_BIN="$TEST_ROOT/fake-bin"
TEST_HOME="$TEST_ROOT/home"
LUMA_HOME="$TEST_HOME/.local/share/luma"
BIN_DIR="$TEST_HOME/.local/bin"
CONFIG_DIR="$TEST_HOME/Library/Application Support/luma"
mkdir -p "$FIXTURES" "$FAKE_BIN" "$TEST_HOME"

cat > "$FIXTURES/luma-core" <<'CORE'
#!/bin/sh
printf 'luma-core installer fixture\n'
CORE
cat > "$FIXTURES/luma-cli.js" <<'CLI'
process.stdout.write("Luma installer fixture\n");
CLI
chmod +x "$FIXTURES/luma-core"

python3 scripts/package-bundle.py \
  --target aarch64-apple-darwin \
  --core "$FIXTURES/luma-core" \
  --cli "$FIXTURES/luma-cli.js" \
  --version 1.0.0-beta.1 \
  --output "$FIXTURES/luma-aarch64-apple-darwin.tar.gz"
sha256sum "$FIXTURES/luma-aarch64-apple-darwin.tar.gz" > "$FIXTURES/luma-aarch64-apple-darwin.tar.gz.sha256"

NODE_DIR="$FIXTURES/node-v22.23.5-darwin-arm64"
mkdir -p "$NODE_DIR/bin"
cat > "$NODE_DIR/bin/node" <<'NODE'
#!/bin/sh
case "${1:-}" in
  --version) printf 'v22.23.5\n' ;;
  -e) printf '1.65.0' ;;
  */playwright/cli.js) exit 0 ;;
  *) exit 0 ;;
esac
NODE
cat > "$NODE_DIR/bin/npm" <<'NPM'
#!/bin/sh
set -eu
prefix=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--prefix" ]; then prefix="$2"; shift 2; else shift; fi
done
[ -n "$prefix" ]
mkdir -p "$prefix/node_modules/.bin" \
  "$prefix/node_modules/playwright" \
  "$prefix/node_modules/@vscode/ripgrep-darwin-arm64/bin"
cat > "$prefix/node_modules/.bin/bun" <<'BUN'
#!/bin/sh
if [ "${1:-}" = "--version" ]; then printf '1.4.4\n'; else printf '%s\n' "$LUMA_VERSION"; fi
BUN
cat > "$prefix/node_modules/playwright/package.json" <<'PLAYWRIGHT'
{"name":"playwright","version":"1.65.0"}
PLAYWRIGHT
cat > "$prefix/node_modules/playwright/cli.js" <<'PLAYWRIGHTCLI'
process.exit(0);
PLAYWRIGHTCLI
cat > "$prefix/node_modules/@vscode/ripgrep-darwin-arm64/bin/rg" <<'RG'
#!/bin/sh
printf 'ripgrep 1.18.0\n'
RG
chmod +x "$prefix/node_modules/.bin/bun" "$prefix/node_modules/@vscode/ripgrep-darwin-arm64/bin/rg"
NPM
chmod +x "$NODE_DIR/bin/node" "$NODE_DIR/bin/npm"
tar -cJf "$FIXTURES/node-v22.23.5-darwin-arm64.tar.xz" -C "$FIXTURES" node-v22.23.5-darwin-arm64
node_hash="$(sha256sum "$FIXTURES/node-v22.23.5-darwin-arm64.tar.xz" | awk '{print $1}')"
printf '%s  node-v22.23.5-darwin-arm64.tar.xz\n' "$node_hash" > "$FIXTURES/SHASUMS256.txt"

cat > "$FAKE_BIN/curl" <<'CURL'
#!/usr/bin/env bash
set -euo pipefail
url=""
output=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    https://*) url="$1"; shift ;;
    *) shift ;;
  esac
done
case "$url" in
  */luma-aarch64-apple-darwin.tar.gz) source="$LUMA_TEST_FIXTURES/luma-aarch64-apple-darwin.tar.gz" ;;
  */luma-aarch64-apple-darwin.tar.gz.sha256) source="$LUMA_TEST_FIXTURES/luma-aarch64-apple-darwin.tar.gz.sha256" ;;
  */SHASUMS256.txt) source="$LUMA_TEST_FIXTURES/SHASUMS256.txt" ;;
  */node-v22.23.5-darwin-arm64.tar.xz) source="$LUMA_TEST_FIXTURES/node-v22.23.5-darwin-arm64.tar.xz" ;;
  *) printf 'unexpected installer URL: %s\n' "$url" >&2; exit 1 ;;
esac
cp "$source" "$output"
CURL
cat > "$FAKE_BIN/uname" <<'UNAME'
#!/bin/sh
case "${1:-}" in
  -s) printf 'Darwin\n' ;;
  -m) printf 'arm64\n' ;;
  *) printf 'Darwin\n' ;;
esac
UNAME
chmod +x "$FAKE_BIN/curl" "$FAKE_BIN/uname"

export HOME="$TEST_HOME"
export SHELL=/bin/zsh
export LUMA_HOME
export LUMA_BIN_DIR="$BIN_DIR"
export LUMA_VERSION=v1.0.0-beta.1
export LUMA_TEST_FIXTURES="$FIXTURES"
export PATH="$FAKE_BIN:$PATH"

bash scripts/install.sh
[[ -x "$LUMA_HOME/bin/luma-core" ]]
[[ -s "$LUMA_HOME/lib/luma-cli.js" ]]
[[ -x "$LUMA_HOME/runtime/node_modules/.bin/bun" ]]
[[ -f "$CONFIG_DIR/skills/ask-question/SKILL.md" ]]
[[ "$("$BIN_DIR/luma" --version)" == "1.0.0-beta.1" ]]
[[ -L "$BIN_DIR/luma-core" ]]
[[ -f "$HOME/.zprofile" ]]

mkdir -p "$HOME/.config/luma"
printf 'keep this user data\n' > "$HOME/.config/luma/config.toml"
bash scripts/install.sh --uninstall
[[ ! -e "$LUMA_HOME" ]]
[[ ! -e "$BIN_DIR/luma" ]]
[[ -f "$HOME/.config/luma/config.toml" ]]
[[ -f "$CONFIG_DIR/skills/ask-question/SKILL.md" ]]
if awk '/Luma PATH/ { found=1 } END { exit found ? 0 : 1 }' "$HOME/.zprofile"; then
  printf 'Luma PATH block was not removed during uninstall\n' >&2
  exit 1
fi

printf 'installer integration test passed\n'
