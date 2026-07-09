#!/usr/bin/env bash
# Install prebuilt grok-companion (no Rust / no clone required).
#
# Simplest:
#   curl -fsSL https://raw.githubusercontent.com/babyRess/grok-plugin-cc/master/scripts/install-companion.sh | bash
#
# Installs to: ~/.grok/bin/grok-companion  (on PATH if you already use Grok CLI)
# The Claude plugin finds it automatically via resolve-companion.mjs

set -euo pipefail

REPO="${GROK_COMPANION_REPO:-babyRess/grok-plugin-cc}"
VERSION="${GROK_COMPANION_VERSION:-}"
# Default: global location every Claude session can share
DEFAULT_DIR="${HOME}/.grok/bin"
DEST_DIR="${GROK_COMPANION_DIR:-$DEFAULT_DIR}"
FORCE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      cat <<'EOF'
Install prebuilt grok-companion (no Rust).

  curl -fsSL https://raw.githubusercontent.com/babyRess/grok-plugin-cc/master/scripts/install-companion.sh | bash

Options:
  --force              Overwrite existing binary
  --version v0.1.2     Pin release tag (default: latest)
  --dir PATH           Install directory (default: ~/.grok/bin)
EOF
      exit 0
      ;;
    -f|--force) FORCE=1; shift ;;
    --version) VERSION="${2:-}"; shift 2 ;;
    --dir) DEST_DIR="${2:-}"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

mkdir -p "$DEST_DIR"
TARGET_BIN="$DEST_DIR/grok-companion"

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
    echo "Unsupported OS: $OS. The Claude plugin still works via Node (no binary needed)." >&2
    exit 1
    ;;
esac

if [[ -z "$VERSION" ]]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1 || true)"
fi
if [[ -z "$VERSION" ]]; then
  echo "Could not find latest release for ${REPO}" >&2
  exit 1
fi

ASSET="grok-companion-${TRIPLE}"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
URL_TGZ="${BASE_URL}/${ASSET}.tar.gz"
URL_BIN="${BASE_URL}/${ASSET}"

echo "→ ${VERSION}  ${TRIPLE}"
echo "→ ${TARGET_BIN}"

if [[ -f "$TARGET_BIN" && "$FORCE" -ne 1 ]]; then
  echo "Already installed (use --force to overwrite):"
  "$TARGET_BIN" --version 2>/dev/null || true
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if curl -fsSL -o "$TMP/a.tgz" "$URL_TGZ" 2>/dev/null; then
  tar -xzf "$TMP/a.tgz" -C "$TMP"
  SRC="$(find "$TMP" -type f -name grok-companion | head -1)"
  [[ -n "$SRC" ]] || { echo "bad tarball" >&2; exit 1; }
  install -m 755 "$SRC" "$TARGET_BIN"
elif curl -fsSL -o "$TMP/grok-companion" "$URL_BIN" 2>/dev/null; then
  install -m 755 "$TMP/grok-companion" "$TARGET_BIN"
else
  echo "No prebuilt binary for ${TRIPLE} in ${VERSION}." >&2
  echo "Plugin still works without it (Node fallback)." >&2
  echo "Or build: cargo build -p grok-companion --release" >&2
  exit 1
fi

# Ensure ~/.grok/bin is on PATH for future shells (idempotent)
MARKER='export PATH="$HOME/.grok/bin:$PATH"'
for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.zprofile"; do
  if [[ -f "$rc" ]] || [[ "$rc" == "$HOME/.zshrc" ]]; then
    if [[ -f "$rc" ]] && grep -qF '.grok/bin' "$rc" 2>/dev/null; then
      break
    fi
    # only touch if file exists or is zshrc (create light hint)
    if [[ -f "$rc" ]]; then
      echo "" >>"$rc"
      echo "# grok-companion (grok-plugin-cc)" >>"$rc"
      echo "$MARKER" >>"$rc"
      echo "Added PATH to $rc"
      break
    fi
  fi
done

echo "OK: $($TARGET_BIN --version 2>/dev/null || echo installed)"
echo ""
echo "Done. In Claude Code (if not already):"
echo "  /plugin marketplace add babyRess/grok-plugin-cc"
echo "  /plugin install grok@xai-grok"
echo "  /reload-plugins && /grok:setup"
