//! Cartridge PNG/embossing raster work, moved off the Android GL thread.
//! Like upstream Slot's CartFaces, CPU raster happens in bounded workers,
//! while texture creation and uploads remain on the GLES owner thread.
use std::collections::HashSet;
use std::sync::{mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError}, Arc, Mutex};
use std::thread;

use slot_store::Cart;
use slot_ui::{cart_face_with_material, CartFace};

type Key = (usize, usize);
type Job = (Key, Cart);
type ResultFace = (Key, CartFace);

pub(crate) struct CartRender {
    submit: SyncSender<Job>,
    finished: Receiver<ResultFace>,
    pending: HashSet<Key>,
}

impl CartRender {
    pub(crate) fn start() -> Result<Self, String> {
        // Never schedule dozens of 9 MiB PNG decodes when the user rapidly
        // spins through a 115-game GBA shelf. Backpressure keeps RAM bounded.
        let (submit, jobs) = mpsc::sync_channel::<Job>(8);
        let (ready, finished) = mpsc::channel::<ResultFace>();
        let shared = Arc::new(Mutex::new(jobs));
        for index in 0..2 {
            let source = Arc::clone(&shared);
            let sink = ready.clone();
            thread::Builder::new()
                .name(format!("slot-kpa-cart-{index}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let receiver = source.lock().unwrap_or_else(|e| e.into_inner());
                            receiver.recv()
                        };
                        let Ok((key, cart)) = job else { break };
                        // All heavy PNG decode, cropping, compositing and satin
                        // shading happens here, never on the GLES render loop.
                        let pixels = cart_face_with_material(&cart, None);
                        if sink.send((key, pixels)).is_err() { break }
                    }
                })
                .map_err(|e| format!("Cannot start cartridge renderer: {e}"))?;
        }
        Ok(Self { submit, finished, pending: HashSet::new() })
    }

    pub(crate) fn queued(&self, key: Key) -> bool {
        self.pending.contains(&key)
    }

    pub(crate) fn request(&mut self, key: Key, cart: &Cart) {
        if self.pending.len() >= 8 || self.pending.contains(&key) { return }
        match self.submit.try_send((key, cart.clone())) {
            Ok(()) => { self.pending.insert(key); }
            Err(TrySendError::Full(_)) => {} // retry next frame; never block input
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub(crate) fn ready(&mut self) -> Option<ResultFace> {
        loop {
            match self.finished.try_recv() {
                Ok((key, face)) if self.pending.remove(&key) => return Some((key, face)),
                Ok(_) => continue,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
            }
        }
    }
}
