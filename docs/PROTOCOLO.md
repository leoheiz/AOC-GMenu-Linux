# Protocolo AOC G-Menu ↔ monitor

O G-Menu oficial para Windows fala com o monitor via **DDC/CI** (MCCS padrão e uma extensão OEM TPV). Este documento resume o framing e a tabela de features reimplementados aqui, **confirmados com leituras Get** no AOC **U34G4C** (Mstar, VCP 2.2, firmware 0.10) em DP / amdgpu.

## Transporte

| Transporte | Notas | Aplica ao U34G4C? |
|---|---|---|
| **DDC/CI sobre I2C** | Caminho principal do G-Menu Windows (APIs nativas + helper I2C; fallback AMD em alguns sistemas) | **Sim** |
| USB HID (vários chips de hub/periférico) | Presente no app Windows para teclado/mouse/headset/LED em outros modelos | **Não confirmado** para OSD/VCP neste painel |
| Serial / RS232 | Feature E2A0 opcional em alguns monitores | **Não** neste painel (página E2A0 inteira sem suporte) |

### Nota de barramento (amdgpu DP)

No Linux com **AMDGPU**, o symlink DRM `ddc` do conector **não** é o canal que responde DDC/CI:

| Caminho | Bus | Nome | DDC/CI |
|---|---|---|---|
| `/sys/class/drm/card1-DP-1/ddc` → `i2c-6` | 6 | (adapter genérico) | **EIO / não responde** |
| `/sys/class/drm/card1-DP-1/i2c-11` | **11** | `AMDGPU DM aux hw bus 0` | **OK** (DP AUX) |

`aoc-gmenu detect` prefere filhos `i2c-*` do conector e faz probe inofensivo (Get VCP `0x10`). Use `--bus 11` para forçar.

## Framing DDC/CI

Endereço I2C 7-bit **0x37** (8-bit write **0x6E** / read **0x6F**). Host virtual **0x51**.

| Direção | Checksum |
|---|---|
| Write (host → display) | `0x6E` XOR todos os bytes do frame (`51`, length, payload) |
| Read (display → host) | `0x50` XOR source `0x6E` XOR length XOR payload |

Length = `0x80 | n` (n = bytes de payload antes do checksum).

### MCCS padrão (VCP get/set)

| Op | Payload | Frame (write) |
|---|---|---|
| Get | `[0x01, vcp]` | `51 82 01 vv CHK` |
| Set | `[0x03, vcp, val_hi, val_lo]` | `51 84 03 vv vh vl CHK` |

### Resposta Get (confirmada no U34G4C)

```
6E  88  02  rc  vv  type  mh ml  sh sl  CHK
│   │   │   │   │   │     └──┴── máximo
│   │   │   │   │   │              └──┴── atual
│   │   │   │   │   └── tipo MCCS (0x00 continuous / 0x01 momentary)
│   │   │   │   └── código VCP ecoado
│   │   │   └── 0x00 = OK; 0x01 = não suportado
│   │   └── opcode Get VCP Feature Reply
│   └── length = 8
└── source
```

**Exemplo** brightness = 80 / max 100:

```
write: 51 82 01 10 ac
read:  6e 88 02 00 10 00 00 64 00 50 90
                 rc vv ty  mh ml sh sl chk
```

Mensagem nula/busy: `6e 80 be` (length 0) — retry até 3× com 50 ms. Feature sem suporte: `rc=0x01` (ex. sharpness `0x87`).

### Extensão TPV (proprietária)

Além do MCCS, o G-Menu Windows envia Gets/Sets com AppCommand + ACode + subcódigo (`ext`):

| Canal | AppCommand | ACode | Uso no U34G4C |
|---|---|---|---|
| Clássico gaming/OSD | **0xF8** | **0xE1** | **Confirmado** — Overdrive, Shadow, Game Color, … |
| Página E2A0 | **0xE2** | **0xA0** | **Não suportado** (toda query retorna `rc=0x01`) |

| Op | Payload | Frame |
|---|---|---|
| Get | `[0x01, app, acode, ext]` | `51 84 01 app acode ext CHK` |
| Set | `[0x03, app, acode, ext, vh, vl]` | `51 86 03 app acode ext vh vl CHK` |

**Resposta TPV F8/E1** (mesmo layout MCCS de 8 bytes): `vv` ecoa `0xF8`; **`type` ecoa o ext**. Valor/máximo nos últimos 4 bytes.

**Exemplo** Overdrive ext `0x51`, valor Fast=1 / max=3:

```
write: 51 84 01 f8 e1 51 f2
read:  6e 88 02 00 f8 51 00 03 00 01 1f
```

### Escolha F8/E1 vs E2A0

O app Windows **não** escolhe por string de modelo fixa:

1. Lê capabilities MCCS (`vcp(...)`).
2. Consulta a bitmask de suporte F8/E1 via Get com `ext=0x00`.
3. Descobre subcódigos E2A0 por probe.

No U34G4C (controller **Mstar**, capabilities só anunciam `FF` como manufacturer), o canal útil é **F8/E1**. E2A0 (Adaptive-Sync, MBR, …) responde `não suportado` via DDC — Adaptive-Sync no OSD provavelmente só local / outro caminho **não confirmado** por DDC.

CLI: `aoc-gmenu support` = Get F8/E1/00 (puro Get). Bitmask observada: **`0xDD3F`** =
ECO, Gamma, DCR, DCB, OverDrive, HDR, LowInputLag, GameColor, ShadowControl, LowBlue, DialPoint, FrameCounter.

## Capabilities (U34G4C)

```
vcp(02 04 05 08 10 12 14(01 05 06 08 0B) 16 18 1A 52 60(11 12 0F)
    62 8D(01 02) AC AE B6 C0 C6 C8 C9 CA(01 02) CC(...) 86(...)
    D6(01 05) DC(00 0B 0C 0D 0E 0F 10) DF FF)
```

Sem `0x87` (sharpness). Game Mode `DC` valores `00,0B–10`.

## Seleção por modelo

- Sem literal `U34G4C` embutido no app Windows; EDID name **`U34G4C`**, fabricante **AOC**.
- Features montadas por capabilities + bitmask TPV + probes E2A0.

## Tabela de features

Legenda **U34G4C**: `sim` = Get válido confirmado; `não suportado` = `rc=0x01` / ausente em caps; `não confirmado` = sem prova live.

| Feature (UI) | Tipo | Código | Valores | R/W | U34G4C | Notas |
|---|---|---|---|---|---|---|
| Brightness | std | `0x10` | 0…100 | RW | **sim** (ex. 80) | MCCS luminance |
| Contrast | std | `0x12` | 0…100 | RW | **sim** (ex. 60) | |
| Color temp | std | `0x14` | sRGB=1, Warm=5, Normal=6, Cool=8, User=11 | RW | **sim** (ex. Warm=5) | |
| Input source | std | `0x60` | DP1=15, HDMI1=17, … | RW | **sim** (ex. DP1=15) | |
| Volume | std | `0x62` | 0…100 | RW | **sim** (ex. 50) | |
| Sharpness | std | `0x87` | — | RW | **não suportado** | ausente em caps |
| Game Mode | std | `0xDC` | Off=0, FPS=11…Gamer3=16 | RW | **sim** (ex. 0) | |
| ECO Mode | tpv-f8e1 | ext **17** | standard…uniformity 0…7 | RW | **sim** (ex. 0) | |
| Gamma (TPV) | tpv-f8e1 | ext **18** | gamma1/2/3 | RW | **sim** (ex. 0) | |
| DCR | tpv-f8e1 | ext **19** | off/on | RW | **sim** (ex. 0) | |
| Overdrive | tpv-f8e1 | ext **81** | **off/fast/faster/fastest** (0…3) | RW | **sim** (ex. Fast=1) | max=3 neste painel |
| HDR Mode | tpv-f8e1 | ext **82** | 0…max | RW | **sim** (ex. 0, max=3) | |
| Low Input Lag | tpv-f8e1 | ext **85** | off/on | RW | **sim** (ex. 1) | |
| Game Color | tpv-f8e1 | ext **87** | 0…20 | RW | **sim** (ex. 10) | |
| Shadow Control | tpv-f8e1 | ext **88** | 0…20 | RW | **sim** (ex. 0) | |
| Low Blue Mode | tpv-f8e1 | ext **89** | off…reading 0…4 | RW | **sim** (ex. 0) | |
| Dial Point | tpv-f8e1 | ext **90** | off/on | RW | **sim** (ex. 0) | OSD crosshair |
| Frame Counter | tpv-f8e1 | ext **91** | off / cantos 0…4 | RW | **sim** (ex. 0) | |
| MBR / MPRT | tpv-e2a0 | ext **0x02** | — | RW | **não suportado** | E2A0 rc=0x01 |
| MBR Sync | tpv-e2a0 | ext **0x03** | — | RW | **não suportado** | idem |
| Smart Crosshair | tpv-e2a0 | ext **0x04** | — | RW | **não suportado** | idem |
| Shadow Boost | tpv-e2a0 | ext **0x05** | — | RW | **não suportado** | idem |
| Low Input Lag (E2A0) | tpv-e2a0 | ext **0x07** | — | RW | **não suportado** | idem |
| Low Blue (E2A0) | tpv-e2a0 | ext **0x24** | — | RW | **não suportado** | idem |
| Adaptive-Sync | tpv-e2a0 | ext **0x40** | — | RW | **não suportado** via DDC (OSD pode estar On) | idem |

`aoc-gmenu list` mostra só features com Get válido; `list --all` inclui erros / não suportado.

## Incertezas restantes

- Como o OSD expõe Adaptive-Sync neste Mstar se E2A0 não responde (outro VCP / só OSD).
- Significado exato de todos os bits altos da bitmask além dos 16 bits retornados em `current`.
- Comportamento fino de alguns Sets TPV em painéis que não sejam U34G4C.
