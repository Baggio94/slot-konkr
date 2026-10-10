#!/usr/bin/env python3
"""Package pinned original Cart Studio with only the KONKR SAF adapter.

Must run after official wasm-pack compilation. No emulation/runtime files,
original renderer, fonts, sprites, or Studio UI logic are rewritten.
"""
from pathlib import Path
import argparse
import shutil

def checked_replace(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError("Official Studio changed; review Android adapter: " + old[:90])
    return text.replace(old, new)

def package(root: Path, built: Path):
    source = root / "third_party/slot-cart-studio/web"
    target = root / "android/app/src/main/assets/studio"
    assert (source / "studio.js").is_file()
    assert (built / "pkg/slot_cart_studio_bg.wasm").is_file()
    target.mkdir(parents=True, exist_ok=True)

    for file in source.glob("*"):
        if file.is_file() and file.suffix in {".js", ".css", ".html"}:
            shutil.copy2(file, target / file.name)
    (target / "pkg").mkdir(exist_ok=True)
    for name in ("slot_cart_studio.js", "slot_cart_studio_bg.wasm"):
        shutil.copy2(built / "pkg" / name, target / "pkg" / name)
    shutil.copy2(root / "android/studio/android-card.js", target / "android-card.js")
    # Only the Android-packaged web app gets compact embedded CSS.
    shutil.copy2(root / "android/studio/embedded.css", target / "embedded.css")

    jsfile = target / "studio.js"
    js = jsfile.read_text("utf-8")
    js = "import { fromAndroid } from './android-card.js';\n" + js
    js = checked_replace(js, "  const crc = new Crc32();",
                         "  if (file.slotCrc != null) return file.slotCrc >>> 0;\n  const crc = new Crc32();")
    # Bypass the single-cart catalogue step on KONKR: open its real editor
    # immediately. The original matching/painting work continues async and
    # refreshes the editor when ROM identity and artwork become available.
    js = checked_replace(js,
                         "  if (s !== session) return;\n  progress(total, total, '');",
                         """  if (s !== session) return;
  // ROM header, existing label, and shell have all been read by now.
  // Open the selected editor BEFORE remote artwork/database requests,
  // so offline or slow Wi-Fi never blocks native Cart Studio controls.
  if (window.AndroidStudio && s.carts.length === 1) {
    editor.open(s.carts[0]);
  }
  progress(total, total, '');""")
    js = checked_replace(js, "  document.body.dataset.ready = 'true';" ,
        """  document.body.dataset.ready = 'true';
  if (window.AndroidStudio) {
    $('pick').hidden = true;
    $('pick-files-label').hidden = true;
    try {
      await open(fromAndroid(window.AndroidStudio));
    } catch (error) {
      console.error('KONKR Cart Studio:', error);
      banner('Could not open selected cartridge: ' + error.message);
    }
  }""")
    js = js.replace("'Write to card'", "'Save to Slot'")
    jsfile.write_text(js, encoding="utf-8")

    htmlfile = target / "index.html"
    html = htmlfile.read_text("utf-8")
    html = checked_replace(html, '<meta charset="utf-8">',
        """<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="
 default-src 'self';
 script-src 'self' 'wasm-unsafe-eval';
 style-src 'self' 'unsafe-inline' https://fonts.googleapis.com;
 font-src 'self' https://fonts.gstatic.com data:;
 connect-src 'self' https://raw.githubusercontent.com https://art.slot-cfw.fyi;
 img-src 'self' data: blob: https://raw.githubusercontent.com https://art.slot-cfw.fyi;
 object-src 'none'; frame-src 'none'; base-uri 'self'; form-action 'none';
">""")
    html = checked_replace(html, '<link rel="stylesheet" href="studio.css">',
        '<link rel="stylesheet" href="studio.css">\n<link rel="stylesheet" href="embedded.css">')
    htmlfile.write_text(html, encoding="utf-8")
    assert 'editor.open(s.carts[0])' in js
    assert 'embedded.css' in html
    assert (target / 'embedded.css').is_file()
    print("Original Cart Studio packaged:", target,
          (target / "pkg/slot_cart_studio_bg.wasm").stat().st_size, "WASM bytes")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default=".")
    ap.add_argument("--built", required=True)
    args = ap.parse_args()
    package(Path(args.source), Path(args.built))
