# KONKR-native texture sampling: design and acceptance

**Scope:** draft U1 visual port (not a public release); existing signed APK RC1
and all ROM/RTC/SRAM/RZIP/Multiplayer logic remain untouched.

## Source fidelity

The Git PNG objects in `assets/` are **identical** to upstream Slot v1.7.0.
Scaling does not create missing photographic detail. The Mitchell-Netravali
filter B=C=1/3 interpolates texels in premultiplied alpha, preventing white
halos at transparent cart edges and slot decals. Cartesian visual positions
and timing remain the original 720x480 coordinate space.

## Physical texture sizes

- Native KONKR render target: **960×640** (already supported).
- GBA front: 360×212 logical -> **480×283** GPU texture.
- GB/GBC fronts: 312×337 -> **416×449**.
- Slot front/back: 720×58 -> **960×77**.
- Built-in wallpaper: 720×480 -> **960×640**.
- User-provided high-resolution PNG wallpaper: decoded **directly to 960×640**.

## Performance and safe behaviour

The 2-worker `CartRender` queue does the CPU resampling before sending
completed RGBA to the GL thread. The existing 42-GPU-texture quota remains.
Physical GPU storage per cached cartridge increases approximately 1.78×,
so the next QA pass must measure peak RAM, texture cache hit-rate,
frame pacing on rapid D-pad navigation, and cartridge insertion/ejection.

Only visual buffers are resampled; core video, high-res Slot text already
rendered at 3×, stored save files and Link packets are untouched.
Do not replace source PNGs with artificially enlarged masters; the original
Git blobs must remain verifiable.

## Automated tests

`cargo test -p slot-ui --test native_resolution` verifies exact new
dimensions, uniform colour fidelity, alpha safety and invalid buffer guards.

## Still outstanding

Exact Dark/Light and Wallpaper Tint menu parity, translucent 3D layers,
native KONKR screenshots compared to upstream, and actual device frame-pacing
measurements. Do not merge PR #30 solely on CI success.
