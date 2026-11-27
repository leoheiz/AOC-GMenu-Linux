# GUI, mira de software, perfis e atalhos

## GUI (`aoc-gmenu-gui`)

Escolha de toolkit: **eframe/egui** (glow) por ser leve, nativo em Wayland via winit, e adequado ao Plasma 6 sem depender de QML/Slint.

Visual: tema escuro (palette do G-Menu Electron: fundos `#101113`/`#16181a`/`#1f2225`, accent `#c91b1b`), **sidebar** à esquerda com ícones desenhados, cards arredondados, sliders com fill e status bar. Janela padrão ~1100×720, título «AOC G-Menu».

Seções: Imagem, Jogo, Low Blue, Entrada, Mira, Perfis. Só features `confirmed` no U34G4C. Leituras/escritas DDC passam por um worker em thread (gap ~45 ms entre features; Get TPV espera ~90 ms); sliders com debounce ~150 ms. Erros na UI são amigáveis («Não consegui ler…»); detalhe técnico vai para stderr / `$XDG_CACHE_HOME/aoc-gmenu/ddc.log`.

### Mira (aba)

- Seletor **HSV** (`color_edit_button_srgba`) + swatches (vermelho, verde, ciano, amarelo, magenta, branco, laranja).
- Hex só em «Avançado».
- Pré-visualização ao vivo via `aoc_crosshair::render_crosshair` (mesma rasterização do overlay).

## Mira (`aoc-crosshair`)

Overlay **wlr-layer-shell** (`zwlr_layer_shell_v1`, suportado pelo KWin):

| Parâmetro | Valor |
|---|---|
| Layer | `Overlay` |
| Anchor | vazio (centrado pelo compositor) |
| Exclusive zone | `-1` |
| Teclado | `None` |
| Input region | vazia (click-through) |
| Render | tiny-skia → `wl_shm` Argb8888 (sem GPU) |

Output padrão: `DP-1` (`crosshair.output` em `config.toml`). Recarrega config a cada 500 ms ou com **SIGUSR1**.

### Limitações Wayland / jogos

- Jogos em **fullscreen exclusivo** / **gamesscope** / “Block compositing” no KWin podem desenhar **acima** do overlay ou desligar o compositor — a mira some.
- Screenshots/gravação podem ou não capturar layer surfaces, conforme ferramenta.
- Fallback X11/XWayland (override-redirect + shape input) **não implementado**; documentado como alternativa se o compositor não expuser layer-shell.

## Perfis

Em `$XDG_CONFIG_HOME/aoc-gmenu/config.toml` (defaults se ausente): `jogo`, `leitura`, `original`.

```bash
aoc-gmenu profile list
aoc-gmenu profile show jogo
aoc-gmenu profile apply jogo --yes   # escreve no monitor
```

`crosshair_enabled` no perfil liga/desliga a mira de software (não o Dial Point do OSD).

## Tray (`aoc-gmenu-tray`)

Processo separado com **ksni** (StatusNotifierItem): Perfil Jogo/Leitura/Original, Mira on/off, Abrir G-Menu, Recarregar, Sair. Separado da GUI para não exigir janela eframe só pelo ícone.

## Atalhos KDE

1. Instale templates (ver `packaging/install-user.sh`) **ou** aponte comandos absolutos para `target/release/…`.
2. Sistema → Atalhos → Atalhos personalizados → Comando:
   - `aoc-gmenu profile apply jogo --yes`
   - `aoc-gmenu profile apply leitura --yes`
   - `aoc-gmenu crosshair toggle`
   - `aoc-gmenu-gui`
3. Alternativa: ações do `.desktop` (`packaging/aoc-gmenu.desktop`) no lançador.
