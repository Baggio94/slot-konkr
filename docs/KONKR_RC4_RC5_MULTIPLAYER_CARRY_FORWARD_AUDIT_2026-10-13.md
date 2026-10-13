# slot. for KONKR Pocket Advance — RC4 → RC5 → Multiplayer Lab carry-forward audit

Date: 2026-10-13. Source of truth: Git history, PRs, workflows and source files, not APK names alone.

## Immutable/review boundaries
- Stable RC4: `release/konkr-0.1.0-rc4-clean` @ `2897b14d0a212139e7a171ca170e8e86084c7cb2`. **Never write/merge to it**.
- RC5 integration: `integration/konkr-rc4-next` @ `5e29b35d407c044f39650f38528d67234fd3fbf6`, PR #23 DRAFT.
- Current experimental branch: `feat/konkr-multiplayer-lab`, PR #29 DRAFT; `0.1.0-rc5-mlab-a3`, versionCode **73**.
- Physical KONKR Beta 5 (code 69) must not be upgraded without separate authorization and verified backup. Mac AVD is the isolated test target.
- A green CI is a build/test signal, **not a validated multiplayer gameplay or perfect-audio claim**.

## Git ancestry audit: what is already carried forward
The following history is **strictly ancestral** to the Multiplayer Lab branch (`compare(base, lab)` = `ahead`, `behind_by=0`), so do not re-import it blindly:
- Earlier native app/dev, menu, audio, save/RTC/RetroArch/Cart Studio and RetroAchievements work that culminated in RC4.
- RC4 Beta 1–12 branch PR #6 `feat/konkr-emulation-latency-diagnostics`: Show Stats; STANDARD, RESPONSIVE and DISPLAY SYNC; game-specific frame pacing/Run-Ahead/Frame Delay/GPU Sync/audio latency; input timing, reusable GLES buffers, mGBA/gpSP-speculative audio and tests. Historical intermediate choices were superseded intentionally: preserve the final RC4 behavior.
- RA icon/header PR #7; validated PSG-channel Run-Ahead rollback incorporated by PR #11; Beta18 interface/RA navigation PR #12; Beta19 compact RA notice and unbadged trophy PR #13; Beta19b launch notification/eject race repair PR #14; clean signed RC4 PR #15.
- GB/GBC Run-Ahead, RA online/offline, proper GB/GBC memory mappings and RA cache tests from PR #19.
- GBA shared saves, opt-in Wi-Fi/Bluetooth/app status and permission model, RA connectivity diagnosis and GB/GBC boxart warming from feature PR #17 / merge PR #20.
- Mac Android 12/API32 960×640 AVD and emulator-only key mapping from feature PR #18 / merge PR #21.
- gpSP LAN Host/Join, netpacket Cable/RFU, mDNS and direct-IP NAT join with original Slot artwork/animations from feature PR #16 / merge PR #22.
- RC5 early integration PR #23 and merged PRs:
  - #24 / `a085d803596053933f9d1cf6e9049e6ff041a509`: empty ROM library crash protection.
  - #25 / `5ca15014c704095690b226e1ac16191c4ced18af`: Beta3 initial SRAM directory staging, Mac Z/X mapping, Apps Back, native onboarding, Cart Studio shell save progress, selected cart preservation.
  - #26 / `5924460a5563d0e040999e73340343c44583baca`: Beta4 emulator-only A/B short keypress latch until a **real emulated frame**.
  - #27 / `5e29b35d407c044f39650f38528d67234fd3fbf6`: Beta5 native Slot shelf messaging, SlotModalDialog direct-IP, Apps/Cart Studio typography/progress, original keycaps, RA styling, no carousel flash.
All merge commits #20–22 and #24–27 have also been checked as ancestors of the Lab branch. PRs still labelled Draft upstream are not proof of physical validation; this only proves code ancestry.

## The actual missing RC5 Beta6 changes — PR #28
PR #28 (`fix/konkr-rc5-beta6-shared-gba-audio-diagnostics`) diverged from the integration history and was **not merged** into PR #23, which caused Lab a1 to reintroduce the `Game saved locally. mGBA/gpSP copy deferred.` message for some save capacities.

Reintegrated into Lab **without replacing the multiplayer-specific Rust session logic**:
1. `RetroArchStorage.kt` from Beta6: `sameGbaRam`, `gbaRamAtSize`; accepts only erased `0xFF` padding when gpSP reports 128 KiB vs mGBA's true save size. Preflight *both* destinations and private peer before mirrored writes; preserve original file size, backups and canonical 128-KiB journal. Abort on non-erased tails, unknown capacities and genuine divergent histories.
2. `SharedGbaSavesTest.kt` from Beta6: 512 B / 8 / 32 / 64 / 128 KiB, erased suffix equivalence, changed progress, non-erased tails, unsupported capacities, concurrent conflicts.
3. `MainActivity.kt` from Beta6: metadata-only AudioTrack sample rate/buffer/empty-read/underrun counters, no change to actual audio buffer/timing engine.
4. `runtime.rs` targeted carry-over: clearer gpSP unsupported-link diagnostic with ROM code/title/header; retains experimental Lab's mGBA path.
5. **Intentionally not transplanted:** Beta6's buildCode=70/versionName and Beta6-named *release artifact* workflow, which are package-specific and would downgrade/break Lab's unique a3 identity. Other Beta6 source changes are accounted for above.

Commits on PR #29 carrying Beta6:
- `fc7f230c3fcb6b90969115d4f3412cb1d4e86965` — save bridge.
- `5264079953b306288dd9d915f8e85b455fd9023f` — save bridge tests.
- `40b95336fdd72292788fc935b08f7529e0ca38bd` — AudioTrack diagnostics.
- `caa75e1f3b916566148c5b2140f6b43d600e1592` — gpSP unsupported protocol diagnostic.
- `b2a99ed59c73f1320e3fa044eb976bd35c3c335c` — a3/code 73 (supersedes a1/code71 and a2/code72).

## Historical experimental branches intentionally not mixed into shipping code
- PR #8 / #9 opt-in WAV/PCM sampling and 120-frame comparison were **diagnostics**, not permanent runtime functionality.
- PR #10's transient PSG fix was selectively carried into validated PR #11/RC4, but old diagnostic capture code was not; importing it wholesale would add overhead and risk regression.
- Unverified mGBA Wi-Fi Link fork, split-screen local, 3–4 players, and cross-core gpSP↔mGBA interoperability **remain research/experimental goals**, not working features.

## Multiplayer Lab additions beyond integrated RC5
- Branch PR #29: mGBA linked core rebuild with 7 upstream patches; `SLK1` linked-state parsing; isolated `Saves/Multiplayer/mGBA/<ROM>/<host|join>` SRAM/RTC and linked state directories; priority restore of preserved solo save/state.
- `KMLB/v2` no-downgrade handshake checks exact mode/backend and SHA-256 of ROM, core and BIOS; rejects legacy/mixed binaries.
- mGBA host merges two distinct single-player states into synchronized pair and both wait for readiness before advancing; dedicated mock tests for distinct saves, bad BIOS, malformed states. gpSP Netpacket engine kept separate.
- No confirmed real Mac↔KONKR gameplay or post-disconnect SRAM/RTC persistence as of this audit. Do not merge PR #29 or install on physical KONKR as a release.

## Non-regression checklist — executable + manual
**CI**: Rust cargo tests and pinned mGBA/gpSP Android ARM64 builds, RA memory-map tests, libretro ABI/link options, Kotlin `SharedGbaSavesTest`/RZIP, Cart Studio WASM packaging, Android APK build and signature.
**Mac AVD**: Z/X tap and hold, home/menu/SELECT/back, first-run empty shelf, Apps label/button/footer, Cart Studio/RA dialogs with no carousel flash, artwork/GB/GBC switching, icon permissions opt-in only.
**Emulation/audio**: GBA Wario Land 4/Kirby/Advance Wars 2/Virtua Tennis and GB/GBC; both cores where applicable, Run-Ahead OFF/1/2, STANDARD/RESPONSIVE/DISPLAY SYNC and FPS stats, PSG/PCM; actual gameplay/audio verification is still needed.
**Saves**: backups; SRM/RTC restart, mGBA↔gpSP pad-size equivalence, independent SRAM conflict stays blocked, no silent overwrite, RetroArch RZIP/auto/manual Polaroids, Undo, offline RA awards and sync. The warning toast may still be legitimate for real conflicts/SAF failures; never hide it blindly.
**Multiplayer**: exact ROM/core/BIOS and mode, AVD direct IP vs physical Wi-Fi Host, version handshake, distinct saves each side, mGBA peer ACK and frame progress, gpSP Cable/RFU, disconnect/eject/restart retention, pacing and sound. Physical KPA tests require explicit consent and verified backups.
**Release gate**: remove/compile-exclude AVD-only keyboard adapters from final KONKR-only production APK, but retain on Mac QA builds. Preserve the permanent signing certificate and do not modify frozen RC4.

## Acceptance
Statuses must remain distinct: **IN GIT HISTORY**, **REINTEGRATED**, **CI PASSED**, **TESTED ON MAC**, **TESTED ON KONKR**, **RELEASE APPROVED**. Never infer a later status from an earlier one.
