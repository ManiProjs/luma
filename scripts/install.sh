#!/usr/bin/env bash
set -Eeuo pipefail

readonly REPOSITORY="ManiProjs/luma"
readonly NODE_VERSION="22.23.5"
readonly BUN_VERSION="1.4.4"
readonly PLAYWRIGHT_VERSION="1.65.0"
readonly RIPGREP_VERSION="1.18.0"
readonly API="https://api.github.com/repos/${REPOSITORY}"

LUMA_HOME="${LUMA_HOME:-${HOME}/.local/share/luma}"
BIN_DIR="${LUMA_BIN_DIR:-${HOME}/.local/bin}"
VERSION="${LUMA_VERSION:-}"

fail() { printf 'Luma installer: %s\n' "$*" >&2; exit 1; }
log() { printf '\n==> %s\n' "$*"; }
download() { curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 "$1" --output "$2"; }

verify_sha256() {
  local file="$1" expected="$2" actual
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  fi
  [[ "$actual" == "$expected" ]] || fail "SHA-256 verification failed for $(basename "$file")."
}

resolve_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}/${arch}" in
    Darwin/arm64|Darwin/aarch64) TARGET="aarch64-apple-darwin" ;;
    Darwin/x86_64) TARGET="x86_64-apple-darwin" ;;
    Linux/x86_64|Linux/amd64) TARGET="x86_64-unknown-linux-gnu" ;;
    Linux/aarch64|Linux/arm64) TARGET="aarch64-unknown-linux-gnu" ;;
    *) fail "unsupported platform: ${os}/${arch}. Supported: macOS and glibc Linux on x64/ARM64." ;;
  esac
}

install_node() {
  local runtime="$1" archive checksums expected extract_dir
  archive="node-v${NODE_VERSION}-${NODE_PLATFORM}.tar.xz"
  checksums="${TMP_DIR}/SHASUMS256.txt"
  log "Installing Node.js ${NODE_VERSION} (required by browser automation)"
  download "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" "$checksums"
  expected="$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1; exit }' "$checksums")"
  [[ -n "$expected" ]] || fail "Node.js checksum not found for ${archive}."
  download "https://nodejs.org/dist/v${NODE_VERSION}/${archive}" "${TMP_DIR}/${archive}"
  verify_sha256 "${TMP_DIR}/${archive}" "$expected"
  extract_dir="${TMP_DIR}/node-extract"
  mkdir -p "$extract_dir"
  tar -xJf "${TMP_DIR}/${archive}" -C "$extract_dir"
  rm -rf "$runtime/node"
  mv "$extract_dir/node-v${NODE_VERSION}-${NODE_PLATFORM}" "$runtime/node"
}

install_path_hint() {
  local profile marker shell_name
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name/$(uname -s)" in
    zsh/*) profile="${HOME}/.zprofile" ;;
    bash/Darwin) profile="${HOME}/.bash_profile" ;;
    *) profile="${HOME}/.profile" ;;
  esac
  marker="# >>> Luma PATH >>>"
  if [[ ! -f "$profile" ]] || ! awk -v marker="$marker" 'index($0, marker) { found=1 } END { exit !found }' "$profile"; then
    {
      printf '\n%s\n' "$marker"
      printf 'export LUMA_HOME=%q\n' "$LUMA_HOME"
      # shellcheck disable=SC2016
      printf 'case ":$PATH:" in *":%s:"*) ;; *) export PATH="%s:$PATH" ;; esac\n' "$BIN_DIR" "$BIN_DIR"
      printf '%s\n' '# <<< Luma PATH <<<'
    } >> "$profile"
  fi
}

[[ -n "${HOME:-}" ]] || fail 'HOME is not set.'
[[ "$LUMA_HOME" != "$HOME" && "$LUMA_HOME" != "/" ]] || fail 'LUMA_HOME must be a dedicated directory, not HOME or the filesystem root.'
case "$LUMA_HOME" in */luma|*/Luma) ;; *) fail 'LUMA_HOME must be a dedicated directory ending in luma.' ;; esac

uninstall() {
  log "Removing Luma program files"
  rm -rf "$LUMA_HOME"
  rm -f "$BIN_DIR/luma" "$BIN_DIR/luma-core"
  for profile in "${HOME}/.zprofile" "${HOME}/.profile" "${HOME}/.bash_profile"; do
    if [[ -f "$profile" ]]; then
      temporary="$(mktemp)"
      awk '
        /^# >>> Luma PATH >>>$/ { skip=1; next }
        /^# <<< Luma PATH <<<$/{ skip=0; next }
        !skip { print }
      ' "$profile" > "$temporary"
      mv "$temporary" "$profile"
    fi
  done
  printf 'Removed Luma binaries and runtimes. User configuration, installed skills, and browser caches were preserved.\n'
  exit 0
}

if [[ "${1:-}" == "--uninstall" ]]; then uninstall; fi

command -v curl >/dev/null 2>&1 || fail 'curl is required.'
command -v tar >/dev/null 2>&1 || fail 'tar is required.'
command -v awk >/dev/null 2>&1 || fail 'awk is required.'
resolve_target
case "$(uname -s)" in
  Darwin) NODE_PLATFORM="darwin-${TARGET%%-*}" ;;
  Linux) NODE_PLATFORM="linux-${TARGET%%-*}" ;;
esac
# Rust target architecture names already match Node's archive names.
NODE_PLATFORM="${NODE_PLATFORM/arm64/arm64}"
NODE_PLATFORM="${NODE_PLATFORM/aarch64/arm64}"
NODE_PLATFORM="${NODE_PLATFORM/x86_64/x64}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ -z "$VERSION" ]]; then
  log "Finding the latest Luma release"
  VERSION="$(curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    -H 'Accept: application/vnd.github+json' "$API/releases?per_page=1" \
    | sed -n 's/^[[:space:]]*"tag_name": "\([^"]*\)",$/\1/p' | head -n 1)"
fi
[[ "$VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || fail "could not resolve a valid release tag (got '${VERSION:-empty}'). Set LUMA_VERSION explicitly."

asset="luma-${TARGET}.tar.gz"
log "Downloading Luma ${VERSION} for ${TARGET}"
download "https://github.com/${REPOSITORY}/releases/download/${VERSION}/${asset}" "${TMP_DIR}/${asset}"
download "https://github.com/${REPOSITORY}/releases/download/${VERSION}/${asset}.sha256" "${TMP_DIR}/${asset}.sha256"
expected="$(awk '{print $1; exit}' "${TMP_DIR}/${asset}.sha256")"
[[ "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || fail 'release checksum file is invalid.'
verify_sha256 "${TMP_DIR}/${asset}" "$expected"

log "Preparing the verified Luma bundle"
tar -xzf "${TMP_DIR}/${asset}" -C "$TMP_DIR"
[[ -x "${TMP_DIR}/luma/bin/luma-core" ]] || fail 'release bundle does not contain an executable luma-core.'
[[ -s "${TMP_DIR}/luma/lib/luma-cli.js" ]] || fail 'release bundle does not contain the Ink client.'

runtime="$TMP_DIR/runtime"
mkdir -p "$runtime"
install_node "$runtime"

log "Installing pinned Bun and Playwright runtimes"
"$runtime/node/bin/npm" install --prefix "$runtime" --no-save --no-audit --no-fund \
  "bun@${BUN_VERSION}" "playwright@${PLAYWRIGHT_VERSION}" "@vscode/ripgrep@${RIPGREP_VERSION}"
[[ "$("$runtime/node/bin/node" --version)" == "v${NODE_VERSION}" ]] || fail 'Node.js installation did not match the pinned version.'
[[ "$("$runtime/node_modules/.bin/bun" --version)" == "$BUN_VERSION" ]] || fail 'Bun installation did not match the pinned version.'
playwright_version="$("$runtime/node/bin/node" -e 'process.stdout.write(require(process.argv[1]).version)' "$runtime/node_modules/playwright/package.json")"
[[ "$playwright_version" == "$PLAYWRIGHT_VERSION" ]] || fail 'Playwright installation did not match the pinned version.'

if ! command -v git >/dev/null 2>&1; then
  if [[ "$(uname -s)" == "Linux" ]] && command -v apt-get >/dev/null 2>&1; then
    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
      apt-get update
      apt-get install -y git
    elif command -v sudo >/dev/null 2>&1; then
      sudo apt-get update
      sudo apt-get install -y git
    else
      fail 'Git is required. Install Git or re-run with sudo available.'
    fi
  elif [[ "$(uname -s)" == "Darwin" ]]; then
    fail 'Git is required. Install the Xcode Command Line Tools with: xcode-select --install'
  else
    fail 'Git is required. Install Git with your Linux distribution package manager, then re-run this installer.'
  fi
fi
case "$TARGET" in
  aarch64-apple-darwin) RIPGREP_PLATFORM="darwin-arm64" ;;
  x86_64-apple-darwin) RIPGREP_PLATFORM="darwin-x64" ;;
  aarch64-unknown-linux-gnu) RIPGREP_PLATFORM="linux-arm64" ;;
  x86_64-unknown-linux-gnu) RIPGREP_PLATFORM="linux-x64" ;;
esac
rg_binary="$runtime/node_modules/@vscode/ripgrep-${RIPGREP_PLATFORM}/bin/rg"
[[ -x "$rg_binary" ]] || fail 'the platform-specific ripgrep binary was not installed.'
"$rg_binary" --version >/dev/null
git --version >/dev/null

export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/.cache/ms-playwright}"
mkdir -p "$PLAYWRIGHT_BROWSERS_PATH"
if [[ "$(uname -s)" == "Linux" ]] && command -v apt-get >/dev/null 2>&1; then
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    "$runtime/node/bin/node" "$runtime/node_modules/playwright/cli.js" install-deps chromium
  elif command -v sudo >/dev/null 2>&1; then
    sudo -E "$runtime/node/bin/node" "$runtime/node_modules/playwright/cli.js" install-deps chromium
  else
    fail 'Linux browser dependencies require root or sudo. Re-run with sudo available.'
  fi
elif [[ "$(uname -s)" == "Linux" ]]; then
  log 'Skipping automatic Chromium system-library installation on this Linux distribution; install its Playwright Chromium dependencies before using browser automation.'
fi
"$runtime/node/bin/node" "$runtime/node_modules/playwright/cli.js" install chromium

log "Installing Luma Core, Ink client, and embedded dashboard"
mkdir -p "$LUMA_HOME" "$BIN_DIR"
rm -rf "${LUMA_HOME:?}/bin" "${LUMA_HOME:?}/lib" "${LUMA_HOME:?}/runtime"
cp -R "${TMP_DIR}/luma/bin" "$LUMA_HOME/bin"
cp -R "${TMP_DIR}/luma/lib" "$LUMA_HOME/lib"
mv "$runtime" "$LUMA_HOME/runtime"
cp "${TMP_DIR}/luma/bundle.json" "$LUMA_HOME/bundle.json"
cp "${TMP_DIR}/luma/LICENSE" "$LUMA_HOME/LICENSE"
config_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/luma"
if [[ "$(uname -s)" == "Darwin" ]]; then
  config_dir="${HOME}/Library/Application Support/luma"
fi
mkdir -p "$config_dir/skills"
if [[ -d "${TMP_DIR}/luma/skills/ask-question" && ! -e "$config_dir/skills/ask-question" ]]; then
  cp -R "${TMP_DIR}/luma/skills/ask-question" "$config_dir/skills/ask-question"
fi

cat > "$BIN_DIR/luma" <<'WRAPPER'
#!/bin/sh
set -eu
LUMA_HOME="${LUMA_HOME:-${HOME}/.local/share/luma}"
export LUMA_HOME
export LUMA_BINARY="${LUMA_HOME}/bin/luma-core"
export LUMA_NODE_BIN="${LUMA_HOME}/runtime/node/bin/node"
export LUMA_PLAYWRIGHT_MODULE="${LUMA_HOME}/runtime/node_modules/playwright"
export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/.cache/ms-playwright}"
export DEV=false
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64|Darwin/aarch64) RIPGREP_PLATFORM="darwin-arm64" ;;
  Darwin/x86_64) RIPGREP_PLATFORM="darwin-x64" ;;
  Linux/arm64|Linux/aarch64) RIPGREP_PLATFORM="linux-arm64" ;;
  Linux/x86_64|Linux/amd64) RIPGREP_PLATFORM="linux-x64" ;;
  *) echo "Luma: unsupported ripgrep platform" >&2; exit 1 ;;
esac
export PATH="${LUMA_HOME}/runtime/node/bin:${LUMA_HOME}/runtime/node_modules/.bin:${LUMA_HOME}/runtime/node_modules/@vscode/ripgrep-${RIPGREP_PLATFORM}/bin:${PATH}"
export LUMA_VERSION="$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "${LUMA_HOME}/bundle.json" | head -n 1)"
exec "${LUMA_HOME}/runtime/node_modules/.bin/bun" "${LUMA_HOME}/lib/luma-cli.js" "$@"
WRAPPER
chmod 755 "$BIN_DIR/luma" "$LUMA_HOME/bin/luma-core"
ln -sfn "$LUMA_HOME/bin/luma-core" "$BIN_DIR/luma-core"
install_path_hint

log "Luma ${VERSION} installed"
printf 'Commands: luma, luma teammates, luma dashboard\n'
printf 'Runtime: Bun %s, Node.js %s, Playwright %s + Chromium, ripgrep %s, Git\n' "$BUN_VERSION" "$NODE_VERSION" "$PLAYWRIGHT_VERSION" "$RIPGREP_VERSION"
printf 'Open a new terminal, then run: luma setup\n'
