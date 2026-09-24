/// Widest resampler correction. Must sit above the 0.456% surplus of the 60 Hz panel over the
/// GBA's 59.7275 Hz, or the buffer climbs unchecked.
const MAX_CORRECTION: f64 = 0.005;

/// Resampling ratio from buffer error: above 1.0 stretches for a starved device, below 1.0
/// compresses to drain a full one.
pub fn drc_ratio(queued_frames: usize, target_frames: usize) -> f64 {
    if target_frames == 0 {
        return 1.0;
    }
    let error = target_frames as f64 - queued_frames as f64;
    let ratio = 1.0 + MAX_CORRECTION * error / target_frames as f64;
    ratio.clamp(1.0 - MAX_CORRECTION, 1.0 + MAX_CORRECTION)
}

/// Half the buffer: equal room for a late emulator frame and a late device callback.
pub fn drc_target(capacity_frames: usize) -> usize {
    capacity_frames / 2
}
