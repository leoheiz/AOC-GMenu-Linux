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

## Requisitos

- Toolchain Rust (`rustc` / `cargo`)
- Módulo `i2c-dev` e acesso a `/dev/i2c-*` (grupo `i2c` e/ou udev/ACL tipo uaccess)
- `ddcutil` opcional (fallback só para VCP MCCS padrão; não cobre framing TPV)

## Compilar

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --workspace
# binários em target/release/
```

## Instalação de usuário (sem sudo)

```bash
bash packaging/install-user.sh
```

O script copia os binários para `~/.local/bin` e os `.desktop` para `~/.local/share/applications`. Em seguida:

```bash
update-desktop-database ~/.local/share/applications
# no KDE / Plasma:
kbuildsycoca6
```

Depois disso, **AOC G-Menu** aparece no lançador de aplicações (categoria Settings / Hardware). Garanta que `~/.local/bin` esteja no `PATH` (em geral já está em desktops modernos).

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

**Unofficial** community port: DDC/CI controls, a software crosshair overlay, and profiles for AOC monitors on Linux (KDE/Wayland).

**Not affiliated with, endorsed by, or related to AOC or TPV.** Windows G-Menu is AOC’s product; this project reimplements DDC/CI plus a software overlay from public protocol behavior and independent analysis. No official binaries or artwork are included.

Tested on **AOC U34G4C** (3440×1440@180, DisplayPort, amdgpu / CachyOS, Plasma 6 Wayland). Other AOC/TPV panels may work for standard MCCS VCP; proprietary TPV codes vary by model.

### Requirements

- Rust toolchain
- `i2c-dev` module and access to `/dev/i2c-*` (`i2c` group and/or udev/ACL such as uaccess)
- `ddcutil` optional (fallback for standard VCP only)

### Build

```bash
cargo build --release --workspace
```

### User install (no sudo)

```bash
bash packaging/install-user.sh
update-desktop-database ~/.local/share/applications
kbuildsycoca6   # KDE
```

Binaries go to `~/.local/bin`, desktop files to `~/.local/share/applications`. **AOC G-Menu** then appears in the application launcher (Settings / Hardware). Keep `~/.local/bin` on your `PATH`.

The tray (`aoc-gmenu-tray`) is **optional**; the GUI is the main app.

### Usage

CLI: `detect` / `list` / `get` / `set --yes` / `profile` / `crosshair`, plus the GUI and software crosshair. **`set` and `profile apply --yes` write to the monitor.**

### Limitations

Exclusive fullscreen / gamescope may hide the overlay; Adaptive-Sync/MBR are not exposed via TPV E2A0 on U34G4C; I2C can rare-flake (retries + flock are in place).

### License

MIT — see [`LICENSE`](LICENSE).
