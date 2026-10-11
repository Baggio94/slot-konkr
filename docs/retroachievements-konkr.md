# RetroAchievements for Slot. for KONKR — implementation plan

Status: **phase 1 backend routing only, no working in-game achievements yet**.
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
