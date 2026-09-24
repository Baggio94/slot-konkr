//! Cuts the two cartridge noises out of a real recording into the committed
//! `crates/slot/assets/*.pcm`. Not part of the device build. It finds the transients, cuts a
//! fixed window around the chosen one, and aligns it to the cartridge animation.

mod wav;

pub use wav::{read_wav, WavError};

pub const HZ: f32 = 48_000.0;

/// Clip lengths the frontend expects. `Sfx::tail` derives from them, so changing these
/// changes the animation.
pub const INSERT_LEN: usize = 11_520;
pub const EJECT_LEN: usize = 15_120;

/// Where the transient must land in each clip, mirroring `Sfx::lead`.
pub const INSERT_LEAD: f32 = 0.097;
pub const EJECT_LEAD: f32 = 0.021;

/// Peak levels matching the frontend's existing loudness against game audio.
pub const INSERT_PEAK: f32 = 11_397.0;
pub const EJECT_PEAK: f32 = 12_000.0;

/// A sharp event in the recording: one cart going in, or one coming out.
#[derive(Clone, Copy, Debug)]
pub struct Take {
    /// Seconds from the start of the recording.
    pub at: f32,
    /// Loudest sample in the 30 ms around it, 0.0 to 1.0.
    pub peak: f32,
    /// Silence before it. A take with a neighbour too close cannot be cut cleanly.
    pub clear_before: f32,
    /// Room after it, to the next take or the end of the recording.
    pub clear_after: f32,
}

impl Take {
    /// Whether a clip of `len` samples leading by `lead` fits around this take without
    /// running into the next one or off either end.
    pub fn fits(&self, lead: f32, len: usize) -> bool {
        let after = len as f32 / HZ - lead;
        self.clear_before >= lead && self.clear_after >= after
    }
}

/// Every sharp event in the recording, in time order. It also finds coughs, so listen to the
/// pick rather than trusting the list.
pub fn takes(pcm: &[f32]) -> Vec<Take> {
    let rms: Vec<f32> = (0..pcm.len().saturating_sub(WIN))
        .step_by(HOP)
        .map(|i| {
            let w = &pcm[i..i + WIN];
            (w.iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt()
        })
        .collect();

    let mut hits: Vec<usize> = Vec::new();
    for (k, &now) in rms.iter().enumerate().skip(HISTORY) {
        if now < FLOOR {
            continue;
        }
        let before = &rms[k - HISTORY..k];
        let quiet = before.iter().fold(f32::MAX, |m, &v| m.min(v)).max(1.0e-6);
        if 20.0 * (now / quiet).log10() < JUMP_DB {
            continue;
        }
        // One cart makes several hops loud: keep the first per 80 ms.
        if hits
            .last()
            .is_some_and(|&p| (k - p) * HOP < (0.080 * HZ) as usize)
        {
            continue;
        }
        hits.push(k);
    }

    let peak_around = |centre: usize| {
        let lo = centre.saturating_sub((0.015 * HZ) as usize);
        let hi = (centre + (0.015 * HZ) as usize).min(pcm.len());
        pcm[lo..hi].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };

    let onsets: Vec<usize> = hits.iter().map(|&k| onset(pcm, k * HOP)).collect();

    // One insert is several transients (rails, contacts, stop). Group them and anchor on the
    // loudest, the contact, which is what `lead` is measured to.
    let mut grouped: Vec<(usize, usize)> = Vec::new(); // (anchor, last onset in the group)
    for &sample in &onsets {
        match grouped.last_mut() {
            Some((anchor, last)) if (sample - *last) as f32 / HZ < GROUP => {
                *last = sample;
                if peak_around(sample) > peak_around(*anchor) {
                    *anchor = sample;
                }
            }
            _ => grouped.push((sample, sample)),
        }
    }
    let mut anchors: Vec<usize> = grouped.into_iter().map(|(a, _)| a).collect();

    // Handling the device makes small noises that clear the detector but are not takes.
    let loudest = anchors
        .iter()
        .map(|&a| peak_around(a))
        .fold(0.0f32, f32::max);
    anchors.retain(|&a| peak_around(a) >= loudest * 0.10);

    let total = pcm.len() as f32 / HZ;
    anchors
        .iter()
        .enumerate()
        .map(|(n, &sample)| {
            let at = sample as f32 / HZ;
            Take {
                at,
                peak: peak_around(sample),
                clear_before: match n {
                    0 => at,
                    _ => at - anchors[n - 1] as f32 / HZ,
                },
                clear_after: match anchors.get(n + 1) {
                    Some(&next) => next as f32 / HZ - at,
                    None => total - at,
                },
            }
        })
        .collect()
}

const HOP: usize = 240; // 5 ms
const WIN: usize = 480; // 10 ms
const HISTORY: usize = 20; // 100 ms of hops
const JUMP_DB: f32 = 12.0;
const FLOOR: f32 = 0.004; // about -48 dBFS, below which it is room tone
const GROUP: f32 = 0.400; // transients closer than this are one cart action

/// The sample the rise starts on: the first near a flagged hop crossing a third of the local
/// peak. The 5 ms hop grid is too coarse against a 21 ms lead.
fn onset(pcm: &[f32], hop_start: usize) -> usize {
    let lo = hop_start.saturating_sub(WIN);
    let hi = (hop_start + 3 * WIN).min(pcm.len());
    if lo >= hi {
        return hop_start;
    }
    let gate = pcm[lo..hi].iter().fold(0.0f32, |m, v| m.max(v.abs())) / 3.0;
    (lo..hi)
        .find(|&i| pcm[i].abs() >= gate)
        .unwrap_or(hop_start)
}

#[derive(Debug)]
pub enum CutError {
    /// The window would start before the recording does.
    NotEnoughBefore { want: f32, have: f32 },
    /// The window would run off the end.
    NotEnoughAfter { want: f32, have: f32 },
}

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughBefore { want, have } => write!(
                f,
                "needs {:.0} ms before the transient, the take has {:.0} ms",
                want * 1000.0,
                have * 1000.0
            ),
            Self::NotEnoughAfter { want, have } => write!(
                f,
                "needs {:.0} ms after the transient, the take has {:.0} ms",
                want * 1000.0,
                have * 1000.0
            ),
        }
    }
}

/// Cut `len` samples putting the transient at `at` exactly `lead` in, normalise to `peak` and
/// fade both ends. `lift_db` raises the quiet lead-in (the rails) before the transient; zero
/// leaves it alone.
pub fn cut(
    pcm: &[f32],
    at: f32,
    lead: f32,
    len: usize,
    peak: f32,
    lift_db: f32,
) -> Result<Vec<i16>, CutError> {
    let start = (at - lead) * HZ;
    if start < 0.0 {
        return Err(CutError::NotEnoughBefore {
            want: lead,
            have: at,
        });
    }
    let start = start as usize;
    if start + len > pcm.len() {
        return Err(CutError::NotEnoughAfter {
            want: len as f32 / HZ - lead,
            have: (pcm.len() - start.min(pcm.len())) as f32 / HZ - lead,
        });
    }
    let mut buf = pcm[start..start + len].to_vec();
    lift(&mut buf, lead, lift_db);
    Ok(finish(&mut buf, peak))
}

/// Raise everything before the transient by `db`, easing to unity over the last 15 ms.
/// Applied before normalising, so the transient still decides the gain.
fn lift(buf: &mut [f32], lead: f32, db: f32) {
    if db == 0.0 {
        return;
    }
    let gain = 10.0f32.powf(db / 20.0);
    let at = (lead * HZ) as usize;
    let ramp = (0.015 * HZ) as usize;
    for (i, s) in buf.iter_mut().enumerate().take(at) {
        let u = ((at - i) as f32 / ramp as f32).min(1.0);
        *s *= 1.0 + (gain - 1.0) * raised_cosine(u);
    }
}

/// Fade both ends, scale the loudest sample to `peak`, and convert to i16. The fades stop a
/// cut through room tone clicking.
fn finish(buf: &mut [f32], peak: f32) -> Vec<i16> {
    const IN: usize = 96; // 2 ms
    const OUT: usize = 900; // 19 ms

    let n = buf.len();
    for (i, s) in buf.iter_mut().enumerate() {
        let head = (i as f32 / IN as f32).min(1.0);
        let tail = ((n - 1 - i) as f32 / OUT as f32).min(1.0);
        *s *= raised_cosine(head) * raised_cosine(tail);
    }

    let loudest = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let gain = if loudest > 0.0 { peak / loudest } else { 0.0 };
    buf.iter()
        .map(|v| (v * gain).round().clamp(-32_768.0, 32_767.0) as i16)
        .collect()
}

/// 0 to 1 with both ends flat, so a fade neither starts nor stops abruptly.
fn raised_cosine(u: f32) -> f32 {
    0.5 * (1.0 - (std::f32::consts::PI * u).cos())
}

/// Mono signed 16 bit little endian, as the frontend's `include_bytes!` expects.
pub fn to_le_bytes(pcm: &[i16]) -> Vec<u8> {
    pcm.iter().flat_map(|s| s.to_le_bytes()).collect()
}
