# A13r1 — rollback of A13 second-frame pacing experiment

A13 field testing on KONKR + Mac AVD at 23:03–23:04 (14 Oct 2026)
showed a major regression against A12's previous linked mGBA Cable
recovery. The prior first-frame-cost gate is removed in A13r1.
The exact A12/A11 recovery policy is restored:
- bounded emulation frame debt (up to 2.5 frames);
- up to two confirmed network synchronized frames per callback;
- no speculative frames during Link;
- no extra mutex/queue read before the second frame.

A13 mixed-ROM preauthorization for explicitly trusted test devices,
fail-closed Pokémon FLASH1M checks before and after Link,
and silent End Link remain. This build's versionCode 84 deliberately differs
from the now-deprecated A13 build 83.

### KONKR excerpt
- A12 recovery good baseline: 59.54 mean effective Link FPS, 46
  AudioTrack underruns across approximately 214 seconds (in-game).
- A13: 52.14 mean Link FPS, 103 underruns in around 55 seconds,
  and ~46 `wait_peer` per 5 seconds (versus ~1.7 in A12).
- A13 budget deferrals were logged but do not explain all waits: peer
  timing/network conditions may contribute; root cause is not proven.

### Acceptance
1. Save and exit. Back up both private app Saves/States before install.
2. Confirm the A13r1 APK has versionCode 84, versionName 0.1.0-rc5-mlab-a13r1,
   the same artifact on KONKR and Android Mac AVD.
3. Share Saves OFF. Run F-Zero mGBA Cable 5–10 minutes.
4. Capture the A12 read-only diagnostic and compare Link wait_peer,
   emulation FPS, underruns, >25ms display gaps, step counts.
5. Continue with disposable FireRed/LeafGreen sessions ONLY after
   F-Zero is acceptable. Keep independent backups of valid solo SRAM
   before entering Link or granting mixed-ROM transfers.
6. Never merge to stable without physical tests and user approval.
