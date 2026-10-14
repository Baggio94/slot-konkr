# GBA cross-cartridge compatibility inventory — KONKR Multiplayer Lab — 2026-10-14

## Three statuses are NOT interchangeable

1. **Original GBA hardware compatible**: authoritative game/manual/gameplay documentation.
2. **Slot LAN candidate**: discovery group plus mutual KMLB/3 handshake allows the different title codes on **gpSP**, matching backend, BIOS, mode and native core. Same title code still requires exactly the same ROM SHA-256. mGBA mixed ROM remains denied.
3. **Verified in game on KONKR + Mac AVD**: **NONE of the cross-title combinations below yet**, awaiting physical/AVD QA. `LINKED` alone is not verification of game protocol.

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

These entries are candidates for the *game-specific* link protocol. The pinned gpSP core's `gpsp_serial` options are `auto`, `rfu`, `mul_poke`, `mul_aw1`, `mul_aw2`. **There is no guaranteed generic Cable emulation.** Some hardware-compatible games above may fail core link registration, in-game pairing or timing. Do not mark them shipped until verified per mode in actual games. mGBA dual-GBA assumes one ROM and cannot run a true mixed-ROM pair yet.

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

- GameMenu and Multiplayer connected adapter send out-of-band TCP PAUSE, not fake serial bytes; remote core does not progress while either peer is paused. Back/Resume unpauses.
- GameMenu Multiplayer while linked reopens connected adapter; only explicit End Link terminates session.
- Tests: no core advance during mock gpSP pause; real TCP control delivery; connection peer mismatch rejection; same-ROM and cross-ROM handshake refusal gates.
- On Android Suspend/screen sleep link ends fail-closed rather than manufacturing solo progress.
- Hardware QA required: gpSP RFU and Cable and mGBA Cable, menu open/close asymmetric timing, disconnection and recovery with isolated saves.

**Safety:** branch `feat/konkr-multiplayer-lab`, PR #29 DRAFT. Do not merge into RC4; don't overwrite users' regular SRAM/RTC/RetroArch folders.
