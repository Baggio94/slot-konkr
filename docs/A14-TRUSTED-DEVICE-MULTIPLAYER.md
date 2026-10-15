# A14 — Slot-themed mixed-ROM consent and signed remembered devices

- Experimental PR #29 only, versionCode 87, universal ARM64 / x86_64 APK.
- User-requested deferral: no broad Run-Ahead game audits or additional
  Pokémon Share Saves work in this lot. Existing flash guard and per-core
  SRAM protection remain unchanged.
- mGBA mixed-ROM greeting is now **KMLB/5**. It deliberately refuses older
  KMLB/4 clients before ROM exchange; install A14 on *both* devices. gpSP
  KMLB/3 and same-ROM mGBA gameplay/pacing remain unchanged.
- Before any ROM byte is transferred, both peers create/reuse an app-private
  Ed25519 identity, exchange public keys and 256-bit random challenges
  over the pre-existing bounded TCP channel and prove key ownership by
  signing the complete host/join transcript (including the negotiated
  cartridge codes and ROM SHA256).
- First unknown peer: SlotModalDialog **LINK — SHARE CARTRIDGE** with
  **ALLOW ONCE / REMEMBER DEVICE / CANCEL**, compatible with controller A/B.
  Previously provisioned a13-trusted-mixed-rom.flag is NOT an identity
  and never bypasses this prompt in A14.
- Remembered peer keys are stored only after successfully verifying the
  full peer-ROM exchange. Future signed connections with that key skip
  the dialog, regardless of IP address. No device can be trusted by merely
  claiming another device's public key.
- Trust-on-first-use is NOT transport encryption or protection from
  interception on an untrusted LAN. Use only a trusted local network.
- Existing private save data, Share Saves choice, native core boot, Link
  recovery and KMLB/4 ROM payload verification are unchanged.
- Tests cover persisted key identity, signature substitution/tampering,
  first-use authorization, trust-store corruption, KMLB/4 downgrade refusal,
  and existing connection state tests. Actual Android ↔ Android signed
  pairing remains a required physical QA validation, as does Mac bridge QA.
