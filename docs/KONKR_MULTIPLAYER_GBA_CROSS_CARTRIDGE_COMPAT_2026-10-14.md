# GBA cross-cartridge compatibility inventory — KONKR Multiplayer Lab — 2026-10-14

## Four statuses are NOT interchangeable

1. **Original GBA hardware compatible**: authoritative game/manual/gameplay documentation.
2. **Original-GBA catalogue/discovery family**: `link_compat::compatible_cross_title` records hardware possibilities and shared DNS-SD keys. **This is NOT automatically a runnable connection**.
3. **gpSP backend-eligible**: `runnable_gpsp_cross_title` + `gpsp_runtime_mode_supported` must both accept the chosen hardware protocol before mutual KMLB/3 can return Ready. Same code additionally requires exact ROM SHA-256; mGBA mixed ROM remains denied.
4. **Verified in game on KONKR + Mac AVD**: **NONE of the cross-title combinations below yet**, awaiting physical/AVD QA. Backend eligibility and `LINKED` are not proof of gameplay.

## Same-generation version pairs & special cross-title functions

| Family | GBA pair / grouping | Physical mode | What the games actually do |
|---|---|---|---|
| Pokémon Gen 3 | FireRed, LeafGreen, Emerald, Ruby, Sapphire, same titles or pairwise | RFU only between FR/LG/E; otherwise Cable | Trades, battles, version-dependent events. Progression gates enforced in game |
| Mega Man Battle Network 3 | White ↔ Blue | Cable | Trades, NetBattles |
| Mega Man Battle Network 4 | Red Sun ↔ Blue Moon | Cable | Trades, NetBattles |
| Mega Man Battle Network 5 | Team ProtoMan ↔ Team Colonel | Cable or RFU hardware | Trades, NetBattles |
| Mega Man Battle Network 6 | Gregar ↔ Falzar | Cable or RFU hardware | Trades, NetBattles |
| Medabots | Metabee ↔ Rokusho | Cable | Trade/battle |
| Medabots AX | Metabee ↔ Rokusho | Cable | Battle |
| Golden Sun | Golden Sun → The Lost Age | Cable | One-way saved-data transfer, NOT multiplayer |
| Mega Man Battle Network 1 → 2 | BN1 ↔ BN2 | Cable | One-way Retro Chip Trader, NOT NetBattle across generations |
| Mario Bros. classics | Super Mario Advance 1–4, Mario & Luigi: Superstar Saga in any combination | Cable Multi-Pak | Mario Bros. Classic/Battle ONLY, not their main platform/RPG campaigns |
| Mega Man Zero 3 ↔ Battle Network 4 | Zero 3 with BN4 Red Sun/Blue Moon | Cable | Bonus-chip/unlock transfer; NOT cross-title NetBattle |

### Backend eligibility audit (pinned gpSP core)

Source of truth for automatic serial protocols: [libretro/gpsp `gba_over.h` pinned to 5819380](https://github.com/libretro/gpsp/blob/5819380c2ffb0900219d700a382ee68c464ebb99/gba_over.h), and [`serial.c`](https://github.com/libretro/gpsp/blob/5819380c2ffb0900219d700a382ee68c464ebb99/serial.c). The available settings are `auto`, `rfu`, `mul_poke`, `mul_aw1`, `mul_aw2`.

| Game group | Original GBA | Runtime gpSP in Lab after audit | Remaining proof |
|---|---|---|---|
| Pokémon FR/LG/Emerald | Cable + RFU | Cable `mul_poke`, RFU `auto` | Pairwise actual gameplay |
| Pokémon Ruby/Sapphire | Cable only | Cable `auto` → gpSP's serial POKE path | Real cable exchanges with FR/LG/Emerald |
| Advance Wars 1/2 | Cable | `mul_aw1`/`mul_aw2`; same-ROM, game-specific | Reliability and in-game matches |
| Battle Network 5/6 | Cable + RFU | RFU only; US auto override, EU/JP explicit `rfu` (not in pinned automatic override) | In-game regional/version pairs |
| BN1–4, Golden Sun, Medabots, Mario Bros., Zero 3 bonus | Various Cable | **Unavailable with pinned gpSP** (no generic compatible serial backend). Catalogue retained for future mGBA mixed-ROM/generic-Cable work | New backend + actual exchange |

The new KMLB/3 runtime guard fails closed for unsupported gpSP Cable protocols even when the pair is genuinely compatible on original hardware. It also refuses a bogus same-ROM gpSP Cable mode. The SELECT Cable/Wireless toggle is only offered when *both* emulated modes have a real backend (e.g. compatible Pokémon). It remains possible to use mGBA Cable with the same exact ROM, which was already validated on selected titles; mGBA mixed-ROM cannot work with the current SLK1 engine. **None of these cross-ROM gpSP scenarios is gameplay-validated yet.** The printed game headers and regional combinations still require an independently verified retail ROM database; matching a four-character header is not proof of retail authenticity.

**Source references**:
- Nintendo GBA single- vs multi-pak distinction: https://www.nintendo.com/fr-fr/Assistance/Consoles-plus-anciennes/Quelle-est-la-difference-entre-un-Link-une-cartouche-et-un-Link-multi-cartouches-619409.html
- Nintendo cross-region link warning: https://www.nintendo.com/fr-fr/Assistance/Consoles-plus-anciennes/Le-Game-Boy-Advance-SP-est-il-compatible-avec-les-materiels-et-logiciels-etrangers-619405.html
- Super Mario Advance 4 Mario Bros FAQ (cross-title Mario & Luigi): https://gamefaqs.gamespot.com/gba/589393-super-mario-advance-4-super-mario-bros-3/faqs/26800
- Mega Man Zero 3 ↔ BN4 direct-game link: https://gamefaqs.gamespot.com/gba/920055-mega-man-zero-3/faqs/33770
- BN1→BN2 Retro Chip Trader: https://gamefaqs.gamespot.com/gba/561218-mega-man-battle-network-2/faqs/55333/chip-trader
- Capcom Japanese Boktai crossover contents: https://news.capcomusa.com/2023/03/09/connect-with-the-latest-mega-man-battle-network-legacy-collection-info/

## Additional documented or regional-only candidates NOT auto-approved

- **Boktai 1 ↔ Battle Network 4, Boktai 2 ↔ Battle Network 5, Boktai 3 ↔ Battle Network 6**: authentic GBA JP crossovers, with region/accessory/content restrictions, sometimes RFU. Do NOT group Western versions / hacked English ROMs into a working compatibility family without checking regional game headers and serial choices.
- GBA→GameCube or GBA→Nintendo DS-slot link content cannot be supported by two-GBA peer networking.
- Single-Pak downloadable multiboot is not implemented by current mGBA LAN backend and must never be advertised as Multi-Pak.
- A same-game Link capability or same franchise alone is NOT evidence of inter-title compatibility.

## Pause menu improvements — A6 experimental

- GameMenu and Multiplayer connected adapter send out-of-band TCP PAUSE, not fake serial bytes; remote core does not progress while a pause flag is observed. Back/Resume unpauses. The protocol still lacks a full frame-boundary REQUEST/ACK/RELEASE barrier.
- GameMenu Multiplayer while linked reopens connected adapter; only explicit End Link terminates session.
- Tests: no core advance during mock gpSP pause; real TCP control delivery; connection peer mismatch rejection; same-ROM and cross-ROM handshake refusal gates. TCP reader/writer queues now have finite capacity (512 frames each); saturation terminates the link rather than silently dropping packets. Peer END/EOF are detected without stepping the core when the pause overlay is open.
- On Android Suspend/screen sleep link ends fail-closed rather than manufacturing solo progress.
- Hardware QA required: gpSP RFU and Cable and mGBA Cable, menu open/close asymmetric timing, disconnection and recovery with isolated saves.

**Safety:** branch `feat/konkr-multiplayer-lab`, PR #29 DRAFT. Do not merge into RC4; don't overwrite users' regular SRAM/RTC/RetroArch folders.
