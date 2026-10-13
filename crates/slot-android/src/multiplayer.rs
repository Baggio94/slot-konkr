//! Android LAN transport for Slot multiplayer (not an emulation session yet).
//! Wi-Fi association and WifiManager.MulticastLock are the Kotlin host's job.
//! No blocking socket or DNS-SD work may execute on the GLSurfaceView GL thread.
//! On-wire discovery, greeting and TCP frame packing are the fork's GPL code.

use std::io::{self, Read};
use std::fs::File;
use std::path::{Path, PathBuf};
use ring::digest::{Context, SHA256};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use crate::link_lan::{self, Advertiser, HostInfo, Multicast};
use crate::link_net::{Cancel, TcpLink, LabGreeting};
use crate::link_kind::LinkKind;

const PORT: u16 = 7211;
const BOUND: Duration = Duration::from_secs(30);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role { Host, Join }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Failure { NoNetwork, NobodyCame, IncompatiblePeer, PeerVanished, Cancelled }

pub enum Progress {
    Looking,
    Ready(TcpLink),
    Failed(Failure),
}

fn classify(err: &io::Error) -> Failure {
    match err.kind() {
        io::ErrorKind::Interrupted => Failure::Cancelled,
        io::ErrorKind::TimedOut => Failure::NobodyCame,
        // During the strict KMLB/2 welcome, the Host closes the socket
        // without ACK when ROM/core/BIOS/backend/link mode differs. This
        // arrives on Join as EOF. Never mislabel it as a link that was
        // successfully established and subsequently lost.
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData =>
            Failure::IncompatiblePeer,
        _ if err.to_string().contains("did not accept the greeting") =>
            Failure::IncompatiblePeer,
        _ => Failure::PeerVanished,
    }
}

#[cfg(test)]
mod failure_classification_tests {
    use super::*;

    #[test]
    fn strict_handshake_refusal_is_reported_as_incompatible_not_peer_lost() {
        for kind in [io::ErrorKind::UnexpectedEof, io::ErrorKind::InvalidData] {
            assert_eq!(classify(&io::Error::new(kind, "host refused greeting")),
                Failure::IncompatiblePeer);
        }
        assert_eq!(classify(&io::Error::other("host did not accept the greeting")),
            Failure::IncompatiblePeer);
        assert_eq!(classify(&io::Error::new(io::ErrorKind::TimedOut, "wait")),
            Failure::NobodyCame);
        assert_eq!(classify(&io::Error::new(io::ErrorKind::ConnectionReset, "later")),
            Failure::PeerVanished);
    }
}

/// A canonical GBA ROM header code, not a user-facing filename.
/// Reject wildcards and IDs that could alias another advertised game.
pub fn game_code(raw: &str) -> Option<String> {
    (raw.len() == 4 && raw.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        .then(|| raw.to_owned())
}

/// Keep all identity work off the GL thread. No ROM, core or BIOS bytes
/// are transmitted; a hash mismatch fails before replacing the solo core.
#[derive(Clone, Debug)]
pub struct LinkConfig {
    pub game: String,
    pub rom: PathBuf,
    pub core_file: PathBuf,
    pub bios_file: PathBuf,
    pub backend: Backend,
    pub mode: LinkKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Backend { MgbaSlotCable, GpspNetpacket }

fn sha256_file(path: &Path, max_size: u64) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size == 0 || size > max_size {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
            "Invalid Multiplayer fingerprint file size"));
    }
    let mut digest = Context::new(&SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 { break; }
        digest.update(&buffer[..n]);
    }
    let result = digest.finish();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(result.as_ref());
    Ok(hash)
}

impl LinkConfig {
    fn fingerprint(&self) -> io::Result<LabGreeting> {
        let code = game_code(&self.game).ok_or_else(||
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid Multiplayer GBA header code"))?;
        if self.backend == Backend::MgbaSlotCable && self.mode != LinkKind::Cable {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "mGBA Slot Link supports Cable only"));
        }
        let game: [u8; 4] = code.as_bytes().try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid game code"))?;
        let bios_sha256 = if self.bios_file.is_file() {
            sha256_file(&self.bios_file, 1024 * 1024)?
        } else { [0u8; 32] };
        Ok(LabGreeting {
            game,
            backend: match self.backend {
                Backend::MgbaSlotCable => 1, Backend::GpspNetpacket => 2,
            },
            mode: match self.mode {
                LinkKind::Cable => 1, LinkKind::Wireless => 2,
            },
            rom_sha256: sha256_file(&self.rom, 128 * 1024 * 1024)?,
            core_sha256: sha256_file(&self.core_file, 64 * 1024 * 1024)?,
            bios_sha256,
        })
    }
}

fn connect(role: Role, local: Ipv4Addr, config: &LinkConfig,
           cancel: &Cancel) -> io::Result<TcpLink> {
    if local.is_unspecified() || local.is_loopback() || local.is_multicast()
        || local.is_link_local() || local.is_broadcast() {
        return Err(io::Error::new(io::ErrorKind::AddrNotAvailable,
            "no usable Wi-Fi IPv4 address"));
    }
    let fingerprint = config.fingerprint()?;
    match role {
        Role::Host => {
            // Validate multicast and join the correct interface before announcing.
            let wire = Multicast::open(local)?;
            let info = HostInfo {
                instance: link_lan::instance_name(),
                addr: local, port: PORT, game: config.game.clone(),
            };
            let mut advert: Option<Advertiser> = None;
            // Bind TCP before publishing mDNS, so discovery never races the listener.
            // New Lab v2 handshake is isolated from legacy SLOT/1 on Beta5.
            let result = TcpLink::host_lab_greeted_until(
                &local.to_string(), PORT, BOUND, cancel, &fingerprint,
                || advert = Some(Advertiser::start_with(Box::new(wire), info)),
            );
            drop(advert); // Publish DNS-SD goodbye on both success and failure.
            result
        }
        Role::Join => {
            let mut wire = Multicast::open(local)?;
            link_lan::find(&mut wire, &config.game, cancel, BOUND, |addr| {
                TcpLink::join_lab_greeted(addr.into(), &fingerprint)
            })
        }
    }
}

/// Status is polled by the GL engine; the worker never accesses libretro,
/// RetroAchievements, GL, Android UI or save files.
pub struct LinkStarter {
    events: Receiver<Progress>,
    cancel: Cancel,
    // Report unexpected worker exit exactly once, rather than spinner forever.
    done: bool,
}

impl LinkStarter {
    pub fn spawn(role: Role, local: Ipv4Addr, config: LinkConfig) -> io::Result<Self> {
        if game_code(&config.game).is_none() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid GBA Link code"));
        }
        let (tx, events) = mpsc::channel();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        thread::spawn(move || {
            let _ = tx.send(Progress::Looking);
            let result = connect(role, local, &config, &worker_cancel);
            let report = match result {
                Ok(link) if !worker_cancel.is_cancelled() => Progress::Ready(link),
                Ok(_) => Progress::Failed(Failure::Cancelled),
                Err(e) if e.kind() == io::ErrorKind::AddrNotAvailable =>
                    Progress::Failed(Failure::NoNetwork),
                Err(e) => Progress::Failed(classify(&e)),
            };
            let _ = tx.send(report);
        });
        Ok(Self { events, cancel, done: false })
    }

    /// The Emulator NAT does not support IGMP. Direct-IP Join uses the
    /// same strict Lab v2 fingerprint as mDNS and cannot downgrade to SLOT/1.
    pub fn spawn_direct_join(local: Ipv4Addr, host: Ipv4Addr,
                             config: LinkConfig) -> io::Result<Self> {
        if game_code(&config.game).is_none() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid GBA Link code"));
        }
        if local.is_unspecified() || local.is_loopback() || local.is_multicast() ||
            local.is_link_local() || host.is_unspecified() || host.is_loopback() ||
            host.is_link_local() || host.is_multicast() || host.is_broadcast() ||
            !host.is_private() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Direct Join requires a private LAN IPv4 address"));
        }
        let (tx, events) = mpsc::channel();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        thread::spawn(move || {
            let _ = tx.send(Progress::Looking);
            let result = if worker_cancel.is_cancelled() {
                Err(io::Error::new(io::ErrorKind::Interrupted, "Join cancelled"))
            } else {
                config.fingerprint().and_then(|fingerprint|
                    TcpLink::join_lab_greeted(SocketAddrV4::new(host, PORT).into(), &fingerprint))
            };
            let report = match result {
                Ok(link) if !worker_cancel.is_cancelled() => Progress::Ready(link),
                Ok(_) => Progress::Failed(Failure::Cancelled),
                Err(error) => Progress::Failed(classify(&error)),
            };
            let _ = tx.send(report);
        });
        Ok(Self { events, cancel, done: false })
    }

    pub fn poll(&mut self) -> Option<Progress> {
        if self.done { return None; }
        match self.events.try_recv() {
            Ok(event) => {
                self.done = matches!(&event, Progress::Ready(_) | Progress::Failed(_));
                Some(event)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                Some(Progress::Failed(Failure::PeerVanished))
            }
        }
    }

    pub fn cancel(&self) { self.cancel.cancel(); }
}

impl Drop for LinkStarter {
    fn drop(&mut self) { self.cancel(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropped_network_worker_fails_once_instead_of_spinning_forever() {
        let (tx, events) = mpsc::channel::<Progress>();
        let mut starter = LinkStarter { events, cancel: Cancel::new(), done: false };
        assert!(starter.poll().is_none(), "an idle worker may still connect");
        drop(tx);
        assert!(matches!(starter.poll(), Some(Progress::Failed(Failure::PeerVanished))));
        assert!(starter.poll().is_none(), "terminal failure must occur once");
    }

    #[test]
    fn completed_network_worker_cannot_trigger_duplicate_errors() {
        let (tx, events) = mpsc::channel::<Progress>();
        assert!(tx.send(Progress::Looking).is_ok());
        assert!(tx.send(Progress::Failed(Failure::Cancelled)).is_ok());
        drop(tx);
        let mut starter = LinkStarter { events, cancel: Cancel::new(), done: false };
        assert!(matches!(starter.poll(), Some(Progress::Looking)));
        assert!(matches!(starter.poll(), Some(Progress::Failed(Failure::Cancelled))));
        assert!(starter.poll().is_none(),
            "worker channel closing after a terminal status is not another failure");
    }

    #[test]
    fn accepts_only_exact_rom_header_codes() {
        assert_eq!(game_code("BPRE"), Some("BPRE".into()));
        assert_eq!(game_code("AMAE"), Some("AMAE".into()));
        for bad in ["", "GAME BOY", "bpre", "BP RE", "A", "ABCD!", "😀"] {
            assert!(game_code(bad).is_none(), "{bad}");
        }
    }

    fn test_config(code: &str) -> LinkConfig {
        LinkConfig { game: code.into(), rom: PathBuf::from("unused.gba"),
            core_file: PathBuf::from("unused.so"),
            bios_file: PathBuf::from("unused.bin"), backend: Backend::MgbaSlotCable,
            mode: LinkKind::Cable }
    }
    #[test]
    fn direct_join_rejects_public_and_loopback_addresses() {
        for invalid in [Ipv4Addr::LOCALHOST, Ipv4Addr::BROADCAST,
            Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(224, 0, 0, 251)] {
            assert!(LinkStarter::spawn_direct_join(Ipv4Addr::new(10,0,2,16), invalid, test_config("BPRE")).is_err());
        }
        assert!(LinkStarter::spawn_direct_join(Ipv4Addr::new(10,0,2,16),
            Ipv4Addr::new(192,168,68,84), test_config("bad")).is_err());
    }

    #[test]
    fn connection_never_uses_loopback_or_unspecified_wifi() {
        for local in [Ipv4Addr::LOCALHOST, Ipv4Addr::UNSPECIFIED,
                      Ipv4Addr::new(224,0,0,251), Ipv4Addr::BROADCAST] {
            let err = connect(Role::Host, local, &test_config("BPRE"), &Cancel::new()).err().unwrap();
            assert_eq!(err.kind(), io::ErrorKind::AddrNotAvailable);
        }
    }
}
