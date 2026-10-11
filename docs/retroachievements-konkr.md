# RetroAchievements for Slot. for KONKR — implementation plan

Status: **phase 2 — first GBA online award verified on real KONKR hardware (11 October 2026)**. Direct RA sign-in, award upload and UI pending/synced notices confirmed via tester and official RA profile. Offline resync, GB/GBC and optional RAOfflineProxy transport still need real-device validation/implementation.
This milestone deliberately does not create or change a RetroAchievements
account, submit awards, modify ROMs/saves, or claim that offline awards have
reached the server. Mainline Cart Studio and emulation remain unaffected.

## Goal and chosen topology

1. **Online — native direct**: official RetroAchievements emulator protocol
   via verified HTTPS, directly from Slot. (no proxy dependency).
2. **Offline — native cache**: evaluate previously authenticated, downloaded
   softcore definitions with official rcheevos on the emulator memory stream.
   Durably record local pending unlocks with their original timestamps,
   and retry verification/upload on reconnection; never mark them as
   server-confirmed before the server actually accepts them.
3. **Optional — RAOfflineProxy**: explicit user-selected alternative transport
   to a verified localhost endpoint; one award ledger owner per session.
   Do not silently flip a native pending ledger to the proxy or vice versa.
4. **Cold offline**: if the ROM was not prepared and the account is not
   previously authenticated, do not claim that achievements can be earned.

Important: RetroAchievements' official FAQ says it does **not** generally
support starting a completely offline playing session, and expects the emulator
to remain open until reconnected after a connectivity loss. Local queueing
must therefore be reported as *pending* and acceptance must be tested with a
separate test account, not treated as guaranteed.

Relevant primary references:
- Native fork: https://github.com/pvaibhav/slot
- Native reference: `crates/slot-achievements` (softcore GBA; rcheevos 12.2.1)
- Optional app: https://github.com/misantronic/RAOfflineProxy
- Official emulator SDK: https://github.com/RetroAchievements/rcheevos
- Official integration docs: https://github.com/RetroAchievements/rcheevos/wiki/rc_client-integration
- RA FAQ: https://docs.retroachievements.org/general/faq.html

## Confirmed from inspected code

- Original pvaibhav implementation has rcheevos evaluation, worker threads,
  bounded frame queue, account-scoped cached definitions and badges, a
  durable unlock journal, exponential retry, rich presence and award sound.
- It explicitly supports **GBA / console ID 5** only. It hardcodes the
  0x58000 GBA RAM layout, GBA ROM scanning and complete ROM MD5 hashing.
  Those are **not** safe to reuse as-is for Game Boy / Game Boy Color.
- KONKR currently runs mGBA for GB/GBC/GBA and gpSP for GBA. Games open
  from cached Android SAF ROMs, with a dedicated `GameSession` controlling
  save, state, rewind and frame execution. There is no achievement evaluation
  wired into `GameSession` today.
- The Android app has `RaEndpoint` read-only discovery of the separate
  RAOfflineProxy app, plus a status action in the Slot menu. This currently
  **does not earn achievements**.
- RAOfflineProxy is a transport/cache server used by an emulator; it does not
  itself evaluate each game's RAM conditions.

## Engineering milestones

### Phase 1 — transport contract and tests (this branch)

- Pure `RaRouting` policy: Automatic, Off or user-selected RAOfflineProxy.
- Automatic picks direct transport only when actual RA HTTPS request success
  confirms reachability; otherwise cached native softcore mode when prepared.
- Strict 127.0.0.1 URL, valid port and fixed endpoint for external proxy;
  never forward RA credentials to an arbitrary HTTP endpoint.
- Tests ensure no double queue ownership, no false online reporting, and no
  claiming unprepared offline achievements.
- **No playable achievements yet, no change to the current UI menu.**

### Phase 2 — native Rust engine (mandatory)

- Port the fork's rcheevos evaluator (respect GPL-3.0 and vendored MIT
  notices) without copying its Linux networking or SD card assumptions.
- Use confirmed RetroAchievements hashing/memory maps for GB, GBC and GBA.
  Test with real mGBA and gpSP memory descriptors and supported No-Intro ROM
  hashes. Never confuse Cart Studio CRC32 with RetroAchievements hashes.
- Keep memory snapshots bounded, off the GL/audio thread, with reset on
  dropped frames, rewind, save-state loads, new ROM/core and eject.
- Avoid blocking frame presentation on network, filesystem or badge decoding.
- Native account-scoped offline journal: atomic writes, failure preserving
  behavior, retry/backoff, idempotent server acknowledgments, no plaintext
  password logging. Keep local state under app-private storage and expose
  secure backup/export later.

### Phase 3 — Android account onboarding and cache preparation

- Slot-native account sign-in using RA emulator login token, with a
  credential-safe UI. Distinguish the emulator token from Web API key.
- Prepare the GBA/GB/GBC library in a bounded background queue, prioritizing
  selected games, and cache definitions plus badges for offline play.
- Make status honest: disabled, sign-in needed, preparing, ready, offline
  pending, server synced, permanent rejection, unsupported ROM.
- Preserve credentials/cache across upgrades and never reset corrupted
  pending-award files.

### Phase 4 — gameplay UI & optional proxy

- Slot-original 1.8-second achievement notice/badge and volume-aware chime,
  synced status indicator, optionally Rich Presence while online.
- Keep achievements **softcore only** while manual saves, undo, rewind,
  speed controls and the existing core picker remain enabled.
- Proxy support may require an explicit preference and session restart.
  When proxy owns the award queue, Slot must not also queue those awards in
  its private native queue. No silent transport switch mid-game.

### Validation before promoting

- CI tests: clean online auth, unavailable internet, cold offline, cached
  offline, restart with pending award, duplicate unlock, rejected upload,
  account switch, ROM variant, GB/GBC memory mapping, mGBA vs gpSP,
  manual state load and rewind, Android lifecycle, no ANR or frame slowdown.
- Real KONKR: first GBA award online; replay without duplicate; network
  interruption and retry; boot fully offline with a prepared game; same for
  GB and GBC; RAOfflineProxy explicitly opted-in; SleepManager sleep/wake.
- Only mark achievements **synced** after confirmed server acknowledgment.
- Keep this work separate from the active Cart Studio/UI PR #3.


## 11 October — direct Android account onboarding (PR #4)

- Root menu **RetroAchievements** now opens an Android account dialog, not just the
  RAOfflineProxy status check. A separate button can still inspect the optional proxy.
- First-time sign-in posts a `login2` request **directly over HTTPS** to
  `https://retroachievements.org/dorequest.php`; no password enters Rust or ROM/SAF
  storage. Redirect following is disabled, timeouts are bounded, and errors are
  sanitized rather than logging credentials, tokens or HTTP bodies.
- A validated successful response produces only `enabled`, `username`, `token`,
  and a blank `password` in Android `filesDir/Config/retroachievements.toml`.
  It is atomically written, owner-readable only (0600); the parent Config directory
  is restricted to 0700, and app backups are disabled. **The session token itself
  is not encrypted with Android Keystore yet**; it is protected by app-private
  filesystem permissions. Never expose it in bug reports or logs.
- Existing Rust `Service::start` discovers this config on the next GBA launch;
  its own `login2` token verification and cache preparation remain responsible
  for determining readiness. A successful Android sign-in alone is **not** an
  achievement unlock or proof that all game memory mappings are correct.
- Account switching/sign-out are intentionally **not exposed yet**: the active
  native network worker and pending award journal must be safely stopped,
  preserved, and re-scoped before credentials can be replaced. The initial
  account dialog reports a configured username and remains read-only afterward.
- New `RaConfigCodecTest` JVM coverage validates token-only TOML output,
  escaping, malformed username and empty-token rejection. The GBA RAM splitter
  tests in `slot-retro` cover full mGBA and split gpSP maps.
- Next hands-on acceptance sequence: sign in on a clean test install/update
  with a test RA account and Wi-Fi, select an exact RA-known uncompressed
  GBA ROM, launch with **mGBA**, inspect sanitized status / hash / patch load
  and confirm a **real softcore award in the RA account**; then retry with gpSP.
  Do not upload the login response or token from Android logcat.
- The direct channel and cache are implemented independently of RAOfflineProxy.
  The original Kotlin `RaRouting` proxy policy is not yet connected to the Rust
  networking worker. Do not advertise proxy selection as working in this build.

## 11 October — online hardware success and offline test plan

- The user confirmed that **slot. on the KONKR** showed a local pending
  achievement notification followed by **Achievements synced**.
  RetroAchievements officially shows the GBA game
  `Kirby: Nightmare in Dream Land` with **Kirbyendo (Beam Kirby, 1 Casual point)**
  unlocked and recorded on the user's RA profile. This is our first
  end-to-end **online** achievement acceptance test.
- PR #4 now has a dedicated **compact, upper-left RA notification** variant,
  preserving the existing Slot. condensed bold typeface, tracking, uppercase,
  colors and regular toasts in other parts of the UI. This was requested
  after seeing that default Android feedback appears at bottom-center.
- Previously Android had **no network reavailability signal** to the
  Rust worker, meaning the offline retry backoff could take up to 300 seconds.
  `MainActivity` now registers Android's default `ConnectivityManager`
  callback, invokes a tiny JNI queue on network availability/resume, and
  `Engine` wakes the existing `Service::network_available` retry timer.
  The callback is only a *retry hint*: an available Wi-Fi network is NOT
  proof that the RetroAchievements HTTPS endpoint is reachable.
  Without callback permissions the existing exponential retry remains.
- **Offline test #1 on the already-installed build**: Start the already
  cached/recognized Kirby ROM using mGBA **while online**; do not close
  the game. Disable Wi-Fi while it keeps running. Earn an as-yet-unlocked
  achievement (`Burning Sensation`, Fire Kirby, 1 point), verify the
  local pending notification and no server change. Re-enable Wi-Fi
  **without closing the game**; await `Achievements synced`, then
  confirm on the official RA account. The previously installed build may
  take up to five minutes to retry.
- RetroAchievements' official FAQ **does not support fully offline session
  starts**. The team's native preloaded-cache/offline-start approach
  remains experimental and should be a separate test **only after** the
  interrupted-online-session test is confirmed. Successful locally
  persisted pending unlocks are not guaranteed to be accepted by the
  server; the device must not falsely present them as server-synced.
- The previous installed APK does **not** contain the new top-left notice
  or immediate reconnect callback. Finish the ongoing test before any
  installation upgrade, to avoid terminating the active RA session.


## 11 October — KONKR Smart Cache beta feedback: account/diagnostics safety

Hardware screenshots from KONKR show the Android status dialog at 6–7% while "Engine:
Connecting/preparing cache" and, later, "Engine: Online / Pending work: no".
The percentage was NOT "percent of games cached": `network::cache_progress()`
primarily mixed Linux `Games/GBA` scan and dynamic badge download counts. New
patches add badge identities, increasing the denominator and even decreasing
the percentage. No claim of a stalled cache or 100% completion is justified
by this legacy indicator.

The misleading `Proxy status` button read the separate RAOfflineProxy app's
`pendingAwards` field (which can be null => "unknown"), NOT the native Slot
queue, and the optional proxy was never used by native Rust transport. It is
removed from the normal account panel.

**Implemented in the next draft test build:**

- `RaDiagnostics.kt` reads the existing per-account native `unlocks.json`
  and counts `synced: false` separately, without treating broken files as
  empty. Reads native prepared patch arrays by MD5 and the existing Android
  SAF fingerprint index; reports GBA ROMs in the library, indexed ROM hashes,
  and actual prepared GBA definitions instead of misleading badges %. Reads
  persisted Smart Cache prefetch budget to display requests/30 min and a
  next-batch ETA when exhausted.
- `RaPanelDialog.kt` replaces the logged-in Android `AlertDialog` with a
  lightweight dark Slot-style panel using the ORIGINAL upstream `label.ttf`
  (bundled with its OFL license). Supports controller A/B and touch, focus
  indications and refresh; also displays direct connection, Casual, unsupported
  GB/GBC, pending count and local awards count. The OS sign-in password prompt
  stays native for keyboard and credential safety.
- `RaAccount.kt` two-stage sign-out: the user is warned about pending awards
  and confirms **Keep local achievements**. An atomic sign-out marker is
  written then Slot closes to terminate the old Rust evaluator/HTTP worker.
  On next app launch, the marker removes the account's TOML and `auth.json`
  token BEFORE creating a native RA worker; it leaves
  `Saves/RetroAchievements/<account>/unlocks.json`, cached patches, badges
  and Smart Cache fingerprint index untouched. A different account can then
  sign in with a fresh token. No cross-account upload.
- Sign-out is blocked during gameplay and if the native worker reports award
  records still **only in memory** due to storage write failure. An unreadable
  ledger triggers an explicit warning with the file preserved, never a silent
  count of zero.
- A damaged/unsupported ROM prefetch no longer retries the same hash every
  second and monopolizes the quota. It can be reconsidered after relaunch.
- Only **GBA Casual** supported. Android automatic sync while Slot is fully
  closed, user-initiated deletion/export of account data, and GB/GBC remain
  separate backlog items. Offline awards can still hit server age limits
  and should never be promised as confirmed until uploaded.

**Acceptance matrix before release:** screen layout at native KONKR 960x640;
A/B/DPAD focus and scroll; async status progress accuracy; zero and multiple
pending awards; logging out with pending awards and closing Slot; same-account
relogin and upload; different-account login without cross-account data;
no logout while a game is open; corrupted ledger never erased; offline
logout/online re-login; before/after signature update preserving user data.
PR remains DRAFT until physical validation.

### Safety refinement: sign-out flush barrier

The confirmation now asynchronously calls `nativeRaFlushReady()` on the actual
GL thread via `GLSurfaceView.queueEvent`. Rust invokes the existing
`Service::flush_ready()` control barrier and also verifies
`!has_unsaved_awards()`, ensuring queued achievement frames have been
evaluated and the durable ledger was written BEFORE saving a sign-out marker.
The Android UI retries without blocking for up to ~2.5 s; if storage
never becomes ready, sign-out is cancelled and no account records are changed.
Game-active/logout collision is blocked. This covers recent unlocks that
arrive just before eject. Re-login requires an online RA credential exchange.

## 11 October — RetroArch-inspired on-screen achievement widget

After the user's GBA index showed 115 ROMs and 115 cached patch JSON files, the
first-party target is a genuinely RetroArch-inspired popup, not Android's stock
Toast. Upstream RetroArch's `rcheevos_show_achievement_popup` composes a
localized unlock title, achievement name/points and badge art (recent builds
optionally add rarity %). See the original implementation:
https://github.com/libretro/RetroArch/blob/master/cheevos/cheevos.c

**Native + Android implementation (PR #4, hardware acceptance pending):**

- The existing Rust `slot_achievements::Notice` already provides `badge: Option<PathBuf>`
  from a strict `BadgeName` validator; JNI now includes this as a separate
  optional field in its bounded JSON notice. Achievement title/description
  and original sync-pending meaning remain unchanged.
- `RaAchievementOverlay.kt` replaces the short Android system Toast with a
  non-focusable, non-clickable **top-left card inside the Activity's own
  FrameLayout**. It shows the cached achievement badge (if present), original
  Slot OFL `label.ttf`, soft Slot charcoal card, an animated arrival/exit,
  achievement title (+points), description, and explicit **SYNC PENDING**.
  Non-earned status notices use a smaller header and no unrelated image.
- Badge decoder works on a dedicated low-priority I/O executor (never UI/GL or
  emulation worker), validates canonical path under app-private
  `Saves/RetroAchievements`, only allows direct badge PNGs and enforces
  128 KiB/256px limits; when the badge isn't cached the card gracefully uses
  a placeholder, not a network fetch on the renderer. Offline achievement
  generation and pending-award persistence are untouched.
- UI queue is bounded, gives priority to earned rewards if multiple status
  messages arrive, drains at most four notices per 100ms pulse, and disposes
  bitmap loader/callbacks on Activity destroy. No notification channel,
  overlay/special permission or background service.
- No rarity percentage is shown: the current native schema does not include
  rarity. **Never fabricate one**. Future feature after RA API verification.

**Tests on real KONKR required:** award in mGBA/gpSP online and offline,
confirm art when badge cache has actual PNG and graceful placeholder when not;
correct top-left placement and legibility at 960x640; no controller input loss;
multiple queued wins/rapid sync; app background/foreground and destroy;
verify the same earned award is still written to the account ledger and uploads
once on connectivity restore. Keep PR DRAFT until accepted.
