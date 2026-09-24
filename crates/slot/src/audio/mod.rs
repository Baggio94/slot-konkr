mod alsa;
#[cfg(feature = "host")]
mod host;
mod ring;
mod sfx;
mod sink;
mod stub;
pub mod volume;

pub use alsa::AlsaSink;
#[cfg(feature = "host")]
pub use host::HostAudio;
pub use ring::{ring_capacity, Ring};
pub use sfx::Sfx;
pub use sink::{AudioError, AudioSink};
pub use stub::StubSink;

/// The GBA's own rate. The device is opened for it before there is a core to ask.
pub const GBA_HZ: u32 = 32_768;

/// cpal on a desktop, ALSA on the device. A sink that fails to open is not a boot failure.
#[cfg(feature = "host")]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(HostAudio::new())
}

#[cfg(not(feature = "host"))]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(AlsaSink::new())
}
