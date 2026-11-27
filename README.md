> ⚠ **Experimental / only tested on CachyOS + KDE Plasma 6 (Wayland) with an AOC U34G4C.** Other distros, desktop environments, compositors, or monitors may not work.

# AOC G-Menu Linux

Port **não oficial** da comunidade: controles DDC/CI, mira de software e perfis para monitores AOC no Linux (KDE/Wayland).

**Não é afiliado, endossado ou relacionado à AOC ou à TPV.** O G-Menu para Windows é produto da AOC; este projeto reimplementa DDC/CI e um overlay de software a partir do comportamento público do protocolo e de análise independente. Nenhum binário, arte ou asset oficial está incluído.

Testado no **AOC U34G4C** (3440×1440@180, DisplayPort, amdgpu / CachyOS, Plasma 6 Wayland). Outros painéis AOC/TPV podem funcionar para VCP MCCS padrão; códigos proprietários TPV variam por modelo.

| Binário | Função |
|---|---|
| `aoc-gmenu` | CLI (`detect`, `list`, `get`, `set`, `support`, `profile`, `crosshair`) |
| `aoc-gmenu-gui` | GUI principal (eframe/egui) — app do lançador |
| `aoc-crosshair` | Overlay de mira (wlr-layer-shell + tiny-skia) |
| `aoc-gmenu-tray` | Tray opcional (StatusNotifierItem) |

Protocolo: [`docs/PROTOCOLO.md`](docs/PROTOCOLO.md). GUI / mira / perfis: [`docs/GUI-MIRA-PERFIS.md`](docs/GUI-MIRA-PERFIS.md).

## Download / Release

Binários pré-compilados (sem precisar compilar):

- Releases: <https://github.com/leoheiz/AOC-GMenu-Linux/releases>
- Tag v0.1.0: <https://github.com/leoheiz/AOC-GMenu-Linux/releases/tag/v0.1.0>
- Artefato: `aoc-gmenu-0.1.0-x86_64-linux.tar.gz` (+ `SHA256SUMS`)

Instalação a partir do tarball:

```bash
tar -xzf aoc-gmenu-0.1.0-x86_64-linux.tar.gz
# confira os hashes na mesma pasta do tar.gz e do SHA256SUMS (depois de extrair)
sha256sum -c SHA256SUMS
cd aoc-gmenu-0.1.0-x86_64-linux
./install-user.sh            # ou: bash install-user.sh
update-desktop-database ~/.local/share/applications
kbuildsycoca6                # KDE / Plasma
```

O script copia os quatro binários para `~/.local/bin` e os `.desktop` para `~/.local/share/applications` (sem `cargo`). Garanta que `~/.local/bin` esteja no `PATH`.

**Udev (opcional, precisa de root):** o tarball inclui `99-aoc-gmenu-i2c.rules` só como exemplo. Copiar para `/etc/udev/rules.d/` exige root e o usuário deve estar no grupo `i2c` (ou usar `uaccess`). Este projeto **não** executa esse passo por você — veja os comentários no arquivo.

## Requisitos

- Acesso a `/dev/i2c-*` (módulo `i2c-dev`; grupo `i2c` e/ou udev/ACL tipo uaccess)
- `ddcutil` opcional (fallback só para VCP MCCS padrão; não cobre framing TPV)
- Para **compilar a partir do fonte**: toolchain Rust (`rustc` / `cargo`)

## Compilar a partir do fonte

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --workspace
# binários em target/release/
bash packaging/install-user.sh
update-desktop-database ~/.local/share/applications
kbuildsycoca6   # KDE
```

## Instalação de usuário (sem sudo)

Funciona a partir do tarball **ou** de um checkout git (após `cargo build --release --workspace`):

```bash
# tarball:
./install-user.sh
# checkout:
bash packaging/install-user.sh
```

Variáveis opcionais: `PREFIX`, ou `BIN_DIR` / `APP_DIR` (padrão `~/.local/bin` e `~/.local/share/applications`).

O tray (`aoc-gmenu-tray` / «AOC G-Menu Tray») é **opcional** e não é necessário para uso normal — a GUI é o app principal.

## Uso

```bash
aoc-gmenu detect
aoc-gmenu --bus 11 list
aoc-gmenu get brightness
aoc-gmenu set brightness 40 --yes          # ESCREVE no monitor
aoc-gmenu profile list
aoc-gmenu profile show jogo
aoc-gmenu profile apply jogo --yes         # ESCREVE no monitor
aoc-gmenu crosshair toggle                 # mira de software on/off
aoc-gmenu-gui                              # GUI
```

**Aviso:** `set` e `profile apply --yes` gravam no firmware/OSD do monitor. Prefira `get` / `list` para exploração.

Config: `$XDG_CONFIG_HOME/aoc-gmenu/config.toml` (criado sob demanda; defaults com perfis `jogo` / `leitura` / `original`).

## Limitações

- Fullscreen exclusivo / gamescope / “Block compositing” no KWin podem esconder o overlay da mira
- Adaptive-Sync e MBR **não** são expostos via TPV E2A0 no U34G4C (Get retorna não suportado)
- I2C pode falhar ocasionalmente; o código faz retries e usa `flock` entre processos (`$XDG_RUNTIME_DIR/aoc-gmenu.lock`)

## Licença

MIT — ver [`LICENSE`](LICENSE).

---

## English

> ⚠ **Experimental / only tested on CachyOS + KDE Plasma 6 (Wayland) with an AOC U34G4C.** Other distros, desktop environments, compositors, or monitors may not work.

**Unofficial** community port: DDC/CI controls, a software crosshair overlay, and profiles for AOC monitors on Linux (KDE/Wayland).

**Not affiliated with, endorsed by, or related to AOC or TPV.** Windows G-Menu is AOC’s product; this project reimplements DDC/CI plus a software overlay from public protocol behavior and independent analysis. No official binaries or artwork are included.

Tested on **AOC U34G4C** (3440×1440@180, DisplayPort, amdgpu / CachyOS, Plasma 6 Wayland). Other AOC/TPV panels may work for standard MCCS VCP; proprietary TPV codes vary by model.

### Download / Release

Pre-built binaries (no compile needed):

- Releases: <https://github.com/leoheiz/AOC-GMenu-Linux/releases>
- Tag v0.1.0: <https://github.com/leoheiz/AOC-GMenu-Linux/releases/tag/v0.1.0>
- Artifact: `aoc-gmenu-0.1.0-x86_64-linux.tar.gz` (+ `SHA256SUMS`)

```bash
tar -xzf aoc-gmenu-0.1.0-x86_64-linux.tar.gz
# verify checksums in the same folder as the tar.gz and SHA256SUMS (after extracting)
sha256sum -c SHA256SUMS
cd aoc-gmenu-0.1.0-x86_64-linux
./install-user.sh            # or: bash install-user.sh
update-desktop-database ~/.local/share/applications
kbuildsycoca6                # KDE / Plasma
```

Optional udev example (`99-aoc-gmenu-i2c.rules`) is **shipped only as an example** — installing into `/etc/udev/rules.d/` needs root and the `i2c` group (or `uaccess`). This project does not run that step for you.

### Requirements

- Access to `/dev/i2c-*` (`i2c-dev`; `i2c` group and/or udev/ACL such as uaccess)
- `ddcutil` optional (fallback for standard VCP only)
- Rust toolchain only if building from source

### Build from source

```bash
cargo build --release --workspace
bash packaging/install-user.sh
update-desktop-database ~/.local/share/applications
kbuildsycoca6   # KDE
```

### User install (no sudo)

From the tarball or a git checkout (after a release build). Optional env: `PREFIX`, or `BIN_DIR` / `APP_DIR`. Binaries → `~/.local/bin`, desktop files → `~/.local/share/applications`. The tray is **optional**; the GUI is the main app.

### Usage

CLI: `detect` / `list` / `get` / `set --yes` / `profile` / `crosshair`, plus the GUI and software crosshair. **`set` and `profile apply --yes` write to the monitor.**

### Limitations

Exclusive fullscreen / gamescope may hide the overlay; Adaptive-Sync/MBR are not exposed via TPV E2A0 on U34G4C; I2C can rare-flake (retries + flock are in place).

### License

MIT — see [`LICENSE`](LICENSE).
