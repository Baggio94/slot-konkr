//! Build provenance for the about label. Every probe fails soft to `unknown`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

fn main() {
    // A new commit changes the hash, so the label has to be rebuilt with it.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    // HEAD only names the branch; this log moves with every commit and checkout.
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");

    // Git refuses a repo owned by another uid ("dubious ownership") unless it is marked safe:
    // /src in the device container, /__w/slot/slot in CI. Only on refusal, so a desk build
    // never touches the global config.
    let hash = git(&["rev-parse", "--short", "HEAD"])
        .or_else(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let root = root.canonicalize().ok()?;
            git(&["config", "--global", "--add", "safe.directory", root.to_str()?])?;
            git(&["rev-parse", "--short", "HEAD"])
        })
        .unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    // UTC, like every date in the project, so container and host agree on the day.
    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=SLOT_GIT_HASH={hash}");
    println!("cargo:rustc-env=SLOT_GIT_DIRTY={}", u8::from(dirty));
    println!("cargo:rustc-env=SLOT_BUILD_DATE={date}");
}
