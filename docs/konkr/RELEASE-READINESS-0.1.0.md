# KONKR 0.1.0 — release qualification

**First release candidate:** application version **0.1.0-rc.1**, Android versionCode **90**, Git tag **konkr-v0.1.0-rc.1**. The eventual stable version will be **0.1.0** with a new commit, higher versionCode and tag **konkr-v0.1.0**.

GitHub has a historical Linux/H700 release workflow that triggers on tags starting with v. KONKR tags MUST use **konkr-v** instead, so the Android release creates the right signed universal APK.

## Acceptance checklist before candidate publication

- [ ] Full Android CI successful on the exact tagged SHA, including Rust native continuity and JVM unit tests.
- [ ] App package **fyi.slot.konkr**; ARM64 and x86_64 cores embedded in the same APK.
- [ ] Permanent signing certificate matches previously deployed update-compatible builds (SHA256 fingerprint: **4f42b8f0ac98f6a0102804ab51020ea0e7e2de7685c6ce09400a744de0b0fa2f**).
- [ ] Mac visual compare of Settings, Library, Apps, RetroAchievements; dialog A/B, Z/X, CANCEL and input field editability.
- [ ] Whole-folder ROM indexing including unusual roots, additions/deletions, cache and no interruption during gameplay.
- [ ] License notices and full corresponding source availability for third-party binaries.

## Additional checks before stable promotion

- [ ] Real KONKR screen scaling and controller D-pad/A/B/Start/Select.
- [ ] Saved-game persistence and RetroArch bridge on non-precious test saves (do not use valuable LeafGreen save for destructive QA).
- [ ] RetroAchievements offline/online and Cart Studio/Apps smoke checks.
- [ ] Link Cable/compatible RFU smoke tests; no save corruption on Link End and solo return.
- [ ] Frame pacing, audio underruns and input latency baseline measured for Lot C.
- [ ] CI success and physical approval on the exact stable commit; versionName 0.1.0 and monotonic new versionCode.

## Tagging

The connected GitHub integration does not expose a release/tag creation operation. After qualification, the repository owner can use a trusted Git checkout and the **exact verified commit SHA**:

    git fetch origin feat/konkr-multiplayer-lab
    git log -1 --oneline origin/feat/konkr-multiplayer-lab
    git tag -a konkr-v0.1.0-rc.1 VERIFIED_SHA -m "slot. for KONKR Pocket Advance RC1"
    git push origin refs/tags/konkr-v0.1.0-rc.1

Pushing the tag triggers a full Android build. Only a successful **release-signed APK** with expected signing fingerprint, both ABIs and passing continuity tests is published in GitHub Releases; candidate tags are automatically marked **pre-release**.

## Data safeguards

Do not use adb uninstall, pm clear, app-private file deletion, or save migration when updating. Standard **adb install -r** requires no automatic backup. Keep RC4/main untouched while the experimental PR #29 is still a draft.
