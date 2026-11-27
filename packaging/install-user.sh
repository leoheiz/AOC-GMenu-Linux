#!/usr/bin/env bash
# Install binaries + .desktop files for the current user (no root).
# Works from:
#   1) a git checkout (binaries in $ROOT/target/release/, desktop in $ROOT/packaging/)
#   2) an extracted release tarball (binaries + .desktop + this script in the same dir)
#
# Optional env:
#   PREFIX   — if set, BIN_DIR=$PREFIX/bin and APP_DIR=$PREFIX/applications
#   BIN_DIR  — default ~/.local/bin
#   APP_DIR  — default ~/.local/share/applications
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

if [[ -n "${PREFIX:-}" ]]; then
  BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
  APP_DIR="${APP_DIR:-$PREFIX/applications}"
else
  BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
  APP_DIR="${APP_DIR:-$HOME/.local/share/applications}"
fi

BINS=(aoc-gmenu aoc-gmenu-gui aoc-crosshair aoc-gmenu-tray)

# Detect layout: tarball (bins next to this script) vs git checkout.
if [[ -x "$HERE/aoc-gmenu" ]]; then
  BIN_SRC="$HERE"
  DESKTOP_SRC="$HERE"
elif [[ -x "$HERE/../target/release/aoc-gmenu" ]]; then
  ROOT="$(cd "$HERE/.." && pwd)"
  BIN_SRC="$ROOT/target/release"
  DESKTOP_SRC="$HERE"
else
  echo "erro: binaries not found next to this script nor in ../target/release/" >&2
  echo "  From a release tarball: extract and run ./install-user.sh" >&2
  echo "  From a git checkout: cargo build --release --workspace, then bash packaging/install-user.sh" >&2
  exit 1
fi

mkdir -p "$BIN_DIR" "$APP_DIR"

for b in "${BINS[@]}"; do
  src="${BIN_SRC}/${b}"
  if [[ ! -x "$src" ]]; then
    echo "erro: falta ${src}" >&2
    if [[ "$BIN_SRC" == *"/target/release" ]]; then
      echo "  rode: cargo build --release --workspace" >&2
    fi
    exit 1
  fi
  install -m 755 "$src" "${BIN_DIR}/${b}"
  echo "instalado ${BIN_DIR}/${b}"
done

for d in aoc-gmenu.desktop aoc-gmenu-tray.desktop; do
  src="${DESKTOP_SRC}/${d}"
  if [[ ! -f "$src" ]]; then
    echo "erro: falta ${src}" >&2
    exit 1
  fi
  install -m 644 "$src" "${APP_DIR}/${d}"
  echo "instalado ${APP_DIR}/${d}"
done

echo
echo "OK — binários em ${BIN_DIR}, .desktop em ${APP_DIR}"
echo "Atualize o cache do lançador:"
echo "  update-desktop-database ${APP_DIR} 2>/dev/null || true"
echo "  kbuildsycoca6   # KDE / Plasma"
echo "Garanta que ${BIN_DIR} esteja no PATH."
echo "Tray (aoc-gmenu-tray) é opcional — a GUI é o app principal."
