#!/usr/bin/env bash
# Build a user-facing release tarball under _local/dist/ (gitignored).
set -euo pipefail

export PATH="${HOME}/.cargo/bin:${HOME}/.local/bin:${PATH}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
ARCH="x86_64-linux"
NAME="aoc-gmenu-${VERSION}-${ARCH}"
DIST="${ROOT}/_local/dist"
STAGE="${DIST}/${NAME}"
TARBALL="${DIST}/${NAME}.tar.gz"

BINS=(aoc-gmenu aoc-gmenu-gui aoc-crosshair aoc-gmenu-tray)

echo "==> cargo build --release --workspace"
cargo build --release --workspace

rm -rf "$STAGE"
mkdir -p "$STAGE"

for b in "${BINS[@]}"; do
  src="${ROOT}/target/release/${b}"
  if [[ ! -x "$src" ]]; then
    echo "erro: falta ${src}" >&2
    exit 1
  fi
  install -m 755 "$src" "${STAGE}/${b}"
done

install -m 644 "${ROOT}/packaging/aoc-gmenu.desktop" "${STAGE}/aoc-gmenu.desktop"
install -m 644 "${ROOT}/packaging/aoc-gmenu-tray.desktop" "${STAGE}/aoc-gmenu-tray.desktop"
install -m 755 "${ROOT}/packaging/install-user.sh" "${STAGE}/install-user.sh"
install -m 644 "${ROOT}/packaging/99-aoc-gmenu-i2c.rules" "${STAGE}/99-aoc-gmenu-i2c.rules"
install -m 644 "${ROOT}/README.md" "${STAGE}/README.md"
install -m 644 "${ROOT}/LICENSE" "${STAGE}/LICENSE"

echo "==> creating ${TARBALL}"
tar -C "$DIST" -czf "$TARBALL" "$NAME"

echo "==> SHA256SUMS"
(
  cd "$DIST"
  sha256sum "${NAME}.tar.gz" > SHA256SUMS
  for b in "${BINS[@]}"; do
    sha256sum "${NAME}/${b}" >> SHA256SUMS
  done
)

echo "==> ldd / glibc notes -> _local/ldd-notes.txt"
{
  echo "# ldd notes for ${NAME} ($(date -Iseconds))"
  echo
  for b in "${BINS[@]}"; do
    echo "## ${b}"
    ldd "${STAGE}/${b}" || true
    echo
    echo "GLIBC_* symbols (max):"
    # shellcheck disable=SC2016
    objdump -T "${STAGE}/${b}" 2>/dev/null \
      | grep -oE 'GLIBC_[0-9.]+' \
      | sort -Vu \
      | tail -n 5 \
      || true
    echo
  done
} | tee "${ROOT}/_local/ldd-notes.txt"

echo
echo "Artifacts:"
echo "  ${TARBALL}"
echo "  ${DIST}/SHA256SUMS"
echo "  ${STAGE}/"
ls -lh "$TARBALL"
cat "${DIST}/SHA256SUMS"
