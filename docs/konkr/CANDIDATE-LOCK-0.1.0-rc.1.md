# Frozen Android 0.1.0 RC1 candidate

This branch freezes the consolidated A14r2 successor on the exact tested ancestry for the signed Android release-candidate build. The source commit at branch creation was e0f424b782c6f73cf91766380057e798d21c3104.

- Package: fyi.slot.konkr
- Version: 0.1.0-rc.1, versionCode 90
- Supported ABIs in the same APK: arm64-v8a and x86_64
- Required CI: Android build/JVM/Rust plus real patched mGBA continuity validation
- Signing: permanent release-compatible fingerprint must be confirmed by CI
- Publish status: NOT YET TAGGED; NOT YET PUBLIC; hardware QA required for stable release
- No ROM or save data was altered for this branch.
- Main/RC4 and the earlier release/konkr-0.1.0-rc1 branch are preserved intact.
