#!/usr/bin/env bash
# Install a prebuilt grok-companion binary (no Rust/cargo required).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/babyRess/grok-plugin-cc/master/scripts/install-companion.sh | bash
#   ./scripts/install-companion.sh                 # from repo checkout
#   ./scripts/install-companion.sh --version v0.1.2
#   GROK_COMPANION_DIR=/path/to/plugin ./scripts/install-companion.sh
#
# Places binary at:
#   <plugin-root>/bin/grok-companion
# Default plugin-root = repo plugins/grok next to this script, or CWD/plugins/grok

set -euo pipefail

REPO="${GROK_COMPANION_REPO:-babyRess/grok-plugin-cc}"
VERSION="${GROK_COMPANION_VERSION:-}"
DEST_DIR="${GROK_COMPANION_DIR:-}"
FORCE=0

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    -f|--force) FORCE=1; shift ;;
    --version) VERSION="${2:-}"; shift 2 ;;
    --dir) DEST_DIR="${2:-}"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

# Resolve install directory
if [[ -z "$DEST_DIR" ]]; then
  if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
    if [[ -d "$REPO_ROOT/plugins/grok" ]]; then
      DEST_DIR="$REPO_ROOT/plugins/grok/bin"
    fi
  fi
fi
if [[ -z "$DEST_DIR" ]]; then
  if [[ -d "./plugins/grok" ]]; then
    DEST_DIR="$(pwd)/plugins/grok/bin"
  elif [[ -n "${CLAUDE_PLUGIN_ROOT:-}" ]]; then
    DEST_DIR="${CLAUDE_PLUGIN_ROOT}/bin"
  else
    DEST_DIR="$(pwd)/bin"
  fi
fi

mkdir -p "$DEST_DIR"
TARGET_BIN="$DEST_DIR/grok-companion"

# Detect OS/arch → rust target triple used in release assets
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS" in
  darwin)
    case "$ARCH" in
      arm64|aarch64) TRIPLE="aarch64-apple-darwin" ;;
      x86_64) TRIPLE="x86_64-apple-darwin" ;;
      *) echo "Unsupported macOS arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  linux)
    case "$ARCH" in
      x86_64|amd64) TRIPLE="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) TRIPLE="aarch64-unknown-linux-gnu" ;;
      *) echo "Unsupported Linux arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS (need macOS or Linux). Use the Node companion instead:" >&2
    echo "  node plugins/grok/scripts/grok-companion.mjs setup" >&2
    exit 1
    ;;
esac

ASSET="grok-companion-${TRIPLE}"
if [[ "$OS" == "darwin" || "$OS" == "linux" ]]; then
  ASSET_TGZ="${ASSET}.tar.gz"
fi

# Resolve version (latest release tag if unset)
if [[ -z "$VERSION" ]]; then
  if command -v gh >/dev/null 2>&1; then
    VERSION="$(gh release view --repo "$REPO" --json tagName -q .tagName 2>/dev/null || true)"
  fi
fi
if [[ -z "$VERSION" ]]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
fi
if [[ -z "$VERSION" ]]; then
  echo "Could not determine latest release. Pass --version v0.1.2" >&2
  exit 1
fi

echo "Installing grok-companion ${VERSION} for ${TRIPLE}"
echo "  → ${TARGET_BIN}"

if [[ -f "$TARGET_BIN" && "$FORCE" -ne 1 ]]; then
  echo "Binary already exists. Use --force to overwrite."
  "$TARGET_BIN" --version 2>/dev/null || true
  exit 0
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
URL_TGZ="${BASE_URL}/${ASSET_TGZ}"
URL_BIN="${BASE_URL}/${ASSET}"

download() {
  local url="$1" out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
  else
    echo "Need curl or wget" >&2
    exit 1
  fi
}

if download "$URL_TGZ" "$TMP/asset.tgz" 2>/dev/null; then
  tar -xzf "$TMP/asset.tgz" -C "$TMP"
  # tarball should contain grok-companion
  if [[ -f "$TMP/grok-companion" ]]; then
    SRC="$TMP/grok-companion"
  else
    SRC="$(find "$TMP" -type f -name 'grok-companion' | head -1)"
  fi
  if [[ -z "${SRC:-}" || ! -f "$SRC" ]]; then
    echo "tarball missing grok-companion binary" >&2
    exit 1
  fi
  install -m 755 "$SRC" "$TARGET_BIN"
elif download "$URL_BIN" "$TMP/grok-companion" 2>/dev/null; then
  install -m 755 "$TMP/grok-companion" "$TARGET_BIN"
else
  echo "Failed to download prebuilt binary for ${TRIPLE} from ${VERSION}." >&2
  echo "Tried:" >&2
  echo "  $URL_TGZ" >&2
  echo "  $URL_BIN" >&2
  echo "" >&2
  echo "Fallback options:" >&2
  echo "  1) Build locally:  npm run install:rust-bin" >&2
  echo "  2) Use Node:       node plugins/grok/scripts/grok-companion.mjs setup" >&2
  exit 1
fi

echo "Installed:"
"$TARGET_BIN" --version || true
echo ""
echo "Next (Claude Code):"
echo "  /plugin marketplace add babyRess/grok-plugin-cc"
echo "  /plugin install grok@xai-grok"
echo "  /reload-plugins"
echo "  # then copy binary into the installed plugin, or re-run this script with:"
echo "  #   GROK_COMPANION_DIR=\"\$CLAUDE_PLUGIN_ROOT/bin\" $0"
echo "  /grok:setup"
