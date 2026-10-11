# slot. for KONKR Pocket Advance — 0.1.0 RC1

> **Release candidate / Beta** (11 October 2026). Android 12+, arm64-v8a; KONKR Pocket Advance 960×640 (3:2). **Not yet a stable release** and not yet merged into `main`.

## Consolidated source and reproducibility

This RC is **not** a selective cherry-pick. It takes the full `feat/konkr-retroachievements-native` head after both stacked PRs:

- **PR #3** `feat/konkr-dev26-cart-studio-foundation` on top of dev25: Android adaptation of original Slot UI and Cart Studio, library selection, original artwork/editor, native upstream WASM in local WebView, custom shell/label storage, updated menus and Apps/Cart Studio wrappers
- **PR #4** `feat/konkr-retroachievements-native` based on PR #3: GBA native rcheevos, mGBA/gpSP memory maps, RetroAchievements login and account management, Direct HTTPS and durable account-scoped offline queue, Android SAF Smart Cache, badge downloads and Slot-style popups
- `git compare` confirms PR #4 is **108 commits ahead of PR #3 HEAD and zero behind**. Therefore all commits of PR #3 are represented.
- New RC changes: `android/app/build.gradle.kts` uses **versionCode 37** and **versionName 0.1.0-rc1**. App ID stays `fyi.slot.konkr`. Persistent signing key from CI is unchanged; do not ship a build using a temporary debug key.

The build and assets are generated with `.github/workflows/konkr-android.yml`. Signed APK is the `slot-konkr-preview-debug-apk` artifact. Do not assume the workflow passing means hardware usability is fully tested.

## Included

### Fidelity / UI / library
- Original Slot carousels, cartridge appearance, textures, audio animations, menus, icons, wallpaper/visual customization and original controller legends; 3:2 landscape scale.
- Apps page and About, Slot-style native Android text, menus, toasts, login/sign-out dialogs and account dashboard, controller A / B operation with system IME for masked passwords.
- Android SAF indexed multi-ROM library; no ROM write access requested. Native mGBA and gpSP cores; BIOS, full save/state management compatible with configured RetroArch paths; rewind/fast-forward/state UI remain integrated.

### Cart Studio — PR #3
- Authentic upstream Rust/WASM + HTML/CSS/JS Cart Studio bundled in APK; Android local WebView bridge for indexed game list, GB/GBC/GBA, cart labels and shell overrides, manual artwork and PNG selector, per-cart and entire-library Save to slot.
- Upstream GPL and attribution preserved. Official Cart Studio editor is not artificially restyled (Android wrapper uses Slot branding).
- User device validation confirmed opening Cart Studio and insert/eject sounds; full bulk artwork generation, every tab and re-open persistence still require final KONKR validation.

### RetroAchievements — PR #4
- **Native, Direct-to-RA GBA Casual/softcore** on mGBA/gpSP, real achievement awards while online and offline with prepared definitions, account-scoped durable pending journal and reopen/reconnect sync.
- User device validation **CONFIRMED** first-launch offline Kirby award -> Sync Pending -> close Slot offline -> reconnect -> reopen without game -> Achievements synced. One online Super Mario Advance 4 achievement also triggered.
- Pre-download game rules from the indexed GBA ROM library while Slot is foregrounded; unique-game hashes. Badge PNGs download separately. 115 JSON files were observed alongside 115 GBA ROM library entries (number equality alone does not guarantee verified hash-by-hash matches).
- Slot-themed top-left RA popup with badge PNG when available / star fallback, refreshed account dashboard showing known badge counts/progress, safe account sign-out with lossless pending per account, no cross-account upload.
- RA account password is not stored; only private session token. Full-screen RA dashboard; B to return; keyboard/Android IME remains native.

## Known limitations and acceptance blockers (do not claim resolved)

1. **GB/GBC native RetroAchievements not implemented**. GBA Casual/softcore only; do not imply global RA compatibility or Hardcore support.
2. Offline first launch only works if that game's RA definitions were pre-fetched while online. Badge images are a separate best-effort cache; star may appear when image not downloaded. Smart Cache does not continue while app is fully closed.
3. Pending awards are stored per account and can be retried on reopening; remote RetroAchievements acceptance for older awards cannot be guaranteed without RA-server confirmation.
4. Confirm the rebuilt unified Android login/password IME, secure logout with zero/some/unknown pending awards and same/different-account relogin on KONKR. These UI changes have passed CI but are not physically signed off.
5. Confirm the right border and actual PNG image in top-left RA popups on KONKR after install; ensure no clipping and no input interception.
6. Verify full embedded Cart Studio usability, original DAT/matching, custom/import art, shell/finish controls, Save to slot. across library and persistence after restart. This PR remains DRAFT until physical acceptance.
7. Avoid uninstalling, clearing storage, or signing with a different certificate; both actions can destroy private game/RA cache and backups are not yet a guided feature.

## RC hardware test gate

- Install signed RC over prior installed beta with `adb -s BW0308N250009576 install -r <rc1.apk>`; **never uninstall first**. Verify versionCode 37 and versionName 0.1.0-rc1 via `dumpsys package`.
- Slot carousel, GB/GBC/GBA ROM launch (mGBA and gpSP for GBA), BIOS, saves + states + RetroArch share paths, fast-forward/rewind, suspend/resume and controller shortcuts work.
- Apps scroll/content without cutoffs, About accurate and clickable controls; Slot-branded toasts and Cart Studio toolbar/return.
- Cart Studio open via X and menu, read full library, edit preview, apply official/manual artwork, select shells, Save to slot., reboot and verify persistence.
- RetroAchievements connected dashboard with correct account/GBA/badge counters; B back; sign-out confirm with 0 or pending; masked login using controller+IME; no account mixing. Verify no in-game RA notices while disconnected.
- RA direct online unlock; fully offline start with cache -> pending; kill/reopen after online reconnect -> synced; PNG badge appears when cached, otherwise star fallback.
- Check no new errors/ANRs/crashes in Logcat for KONKR device across all above.
- Last step: hardware sign-off for RC1, then decide whether to tag/merge stable. Draft PRs #3/#4 are **not merged automatically**.

## Development branching after RC

- **`release/konkr-0.1.0-rc1`**: freezeable RC snapshot and repeatable CI builds. Fix only candidate blockers here.
- **`develop/konkr`**: continuing integration/development line starting from the validated RC snapshot. Feature PRs target `develop/konkr`, not the unfinished stacked PRs.
- PR #3 and PR #4 remain historical DRAFT references until device validation / final consolidation policy is agreed. Do not destructively rebase them or delete their branches.
- The default branch `main` and existing public stable builds are **unchanged** pending user approval of RC.
