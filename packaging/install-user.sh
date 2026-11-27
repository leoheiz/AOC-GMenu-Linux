#!/usr/bin/env bash
# Copia binários para ~/.local/bin e .desktop para ~/.local/share/applications.
# NÃO execute este script durante desenvolvimento automatizado — é um template
# para o usuário instalar localmente.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"

mkdir -p "$BIN_DIR" "$APP_DIR"

for b in aoc-gmenu aoc-gmenu-gui aoc-crosshair aoc-gmenu-tray; do
  src="${ROOT}/target/release/${b}"
  if [[ ! -x "$src" ]]; then
    echo "erro: falta ${src} — rode: cargo build --release --workspace" >&2
    exit 1
  fi
  install -m 755 "$src" "${BIN_DIR}/${b}"
  echo "instalado ${BIN_DIR}/${b}"
done

install -m 644 "${ROOT}/packaging/aoc-gmenu.desktop" "${APP_DIR}/aoc-gmenu.desktop"
install -m 644 "${ROOT}/packaging/aoc-gmenu-tray.desktop" "${APP_DIR}/aoc-gmenu-tray.desktop"
echo "instalado .desktop em ${APP_DIR}"
echo "Atualize o cache: update-desktop-database ${APP_DIR} 2>/dev/null || true"
