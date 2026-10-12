//! Android LAN transport for Slot multiplayer (not an emulation session yet).
//! Wi-Fi association and WifiManager.MulticastLock are the Kotlin host's job.
//! No blocking socket or DNS-SD work may execute on the GLSurfaceView GL thread.
//! On-wire discovery, greeting and TCP frame packing are the fork's GPL code.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use crate::link_lan::{self, Advertiser, HostInfo, Multicast};
use crate::link_net::{Cancel, TcpLink};

const PORT: u16 = 7211;
const BOUND: Duration = Duration::from_secs(30);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role { Host, Join }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Failure { NoNetwork, NobodyCame, PeerVanished, Cancelled }

pub enum Progress {
    Looking,
    Ready(TcpLink),
    Failed(Failure),
}

fn classify(err: &io::Error) -> Failure {
    match err.kind() {
        io::ErrorKind::Interrupted => Failure::Cancelled,
        io::ErrorKind::TimedOut => Failure::NobodyCame,
        _ => Failure::PeerVanished,
    }
}

/// A canonical GBA ROM header code, not a user-facing filename.
/// Reject wildcards and IDs that could alias another advertised game.
pub fn game_code(raw: &str) -> Option<String> {
    (raw.len() == 4 && raw.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        .then(|| raw.to_owned())
}

fn connect(role: Role, local: Ipv4Addr, game: &str, cancel: &Cancel) -> io::Result<TcpLink> {
    if local.is_unspecified() || local.is_loopback() || local.is_multicast()
        || local.is_link_local() || local.is_broadcast() {
        return Err(io::Error::new(io::ErrorKind::AddrNotAvailable,
            "no usable Wi-Fi IPv4 address"));
    }
    match role {
        Role::Host => {
            // Validate multicast and join the correct interface before announcing.
            let wire = Multicast::open(local)?;
            let info = HostInfo {
                instance: link_lan::instance_name(),
                addr: local, port: PORT, game: game.to_owned(),
            };
            let mut advert: Option<Advertiser> = None;
            // Bind TCP before publishing mDNS, so discovery never races the listener.
            // The exact SLOT/1 + game-code greeting is validated by TcpLink.
            let result = TcpLink::host_greeted_until(
                &local.to_string(), PORT, BOUND, cancel, game,
                || advert = Some(Advertiser::start_with(Box::new(wire), info)),
            );
            drop(advert); // Publish DNS-SD goodbye on both success and failure.
            result
        }
        Role::Join => {
            let mut wire = Multicast::open(local)?;
            link_lan::find(&mut wire, game, cancel, BOUND, |addr| {
                TcpLink::join_greeted(addr.into(), game)
            })
        }
    }
}

/// Status is polled by the GL engine; the worker never accesses libretro,
/// RetroAchievements, GL, Android UI or save files.
pub struct LinkStarter {
    events: Receiver<Progress>,
    cancel: Cancel,
}

impl LinkStarter {
    pub fn spawn(role: Role, local: Ipv4Addr, game: &str) -> io::Result<Self> {
        let code = game_code(game).ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput, "GBA cart has no valid 4-character game code",
        ))?;
        let (tx, events) = mpsc::channel();
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        thread::spawn(move || {
            let _ = tx.send(Progress::Looking);
            let result = connect(role, local, &code, &worker_cancel);
            let report = match result {
                Ok(link) if !worker_cancel.is_cancelled() => Progress::Ready(link),
                Ok(_) => Progress::Failed(Failure::Cancelled),
                Err(e) if e.kind() == io::ErrorKind::AddrNotAvailable =>
                    Progress::Failed(Failure::NoNetwork),
                Err(e) => Progress::Failed(classify(&e)),
            };
            let _ = tx.send(report);
        });
        Ok(Self { events, cancel })
    }

    /// Android Emulator is behind a NAT that does not support IGMP. Allow
    /// emulator Join to use a physical KONKR's explicitly supplied LAN IP,
    /// while retaining SLOT/1 + ROM-code authentication and all async guards.
    pub fn spawn_direct_join(local: Ipv4Addr, host: Ipv4Addr, game: &str) -> io::Result<Self> {
        let code = game_code(game).ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput, "Game has no valid GBA Link code",
        ))?;
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
                TcpLink::join_greeted(SocketAddrV4::new(host, PORT).into(), &code)
            };
            let report = match result {
                Ok(link) if !worker_cancel.is_cancelled() => Progress::Ready(link),
                Ok(_) => Progress::Failed(Failure::Cancelled),
                Err(error) => Progress::Failed(classify(&error)),
            };
            let _ = tx.send(report);
        });
        Ok(Self { events, cancel })
    }

    pub fn poll(&mut self) -> Option<Progress> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
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
    fn accepts_only_exact_rom_header_codes() {
        assert_eq!(game_code("BPRE"), Some("BPRE".into()));
        assert_eq!(game_code("AMAE"), Some("AMAE".into()));
        for bad in ["", "GAME BOY", "bpre", "BP RE", "A", "ABCD!", "😀"] {
            assert!(game_code(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn direct_join_rejects_public_and_loopback_addresses() {
        for invalid in [Ipv4Addr::LOCALHOST, Ipv4Addr::BROADCAST,
            Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(224, 0, 0, 251)] {
            assert!(LinkStarter::spawn_direct_join(Ipv4Addr::new(10,0,2,16), invalid, "BPRE").is_err());
        }
        assert!(LinkStarter::spawn_direct_join(Ipv4Addr::new(10,0,2,16),
            Ipv4Addr::new(192,168,68,84), "bad").is_err());
    }

    #[test]
    fn connection_never_uses_loopback_or_unspecified_wifi() {
        for local in [Ipv4Addr::LOCALHOST, Ipv4Addr::UNSPECIFIED,
                      Ipv4Addr::new(224,0,0,251), Ipv4Addr::BROADCAST] {
            let err = connect(Role::Host, local, "BPRE", &Cancel::new()).err().unwrap();
            assert_eq!(err.kind(), io::ErrorKind::AddrNotAvailable);
        }
    }
}
