//! Cartridge PNG/embossing raster work, moved off the Android GL thread.
//! Like upstream Slot's CartFaces, CPU raster happens in bounded workers,
//! while texture creation and uploads remain on the GLES owner thread.
use std::collections::HashSet;
use std::sync::{mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError}, Arc, Mutex};
use std::thread;

use slot_store::Cart;
use slot_ui::{cart_face_with_material, cart_face, cart_interior, cart_backing,
    gb_shell_of, is_clear, CartFace};

type Key = (usize, usize);
type WorkKey = (Key, u64);
type Job = (WorkKey, Cart, bool);
/// Three source-faithful clear-cart textures for a *selected* cartridge:
/// original back, PCB, translucent front/label. Side carts keep one face.
pub(crate) type Layers = [CartFace; 3];
type ResultFace = (WorkKey, CartFace, Option<Layers>);

pub(crate) struct CartRender {
    submit: SyncSender<Job>,
    finished: Receiver<ResultFace>,
    pending: HashSet<WorkKey>,
    // A rebuilt Recents shelf may reuse index 0 for a different game.
    // Worker results from the old generation must NEVER paint that cartridge.
    epochs: [u64; 5],
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
                        let Ok((key, cart, include_material)) = job else { break };
                        // All heavy PNG decode, cropping, compositing and native artwork
                        // resampling happens here, never on the GLES render loop.
                        let original = cart_face_with_material(&cart, None);
                        // Keep bounded asynchronous work off the GLES thread.
                        let (rgba, w, h) = slot_ui::native_rgba(
                            &original.rgba, original.w, original.h);
                        let pixels = CartFace { rgba, w, h };
                        // Side-car previews need just the single composited
                        // image. Computing three 960p materials for every
                        // prefetched transparent cart spikes CPU and memory
                        // during fast D-pad scrolling. Build them only when
                        // the current selection explicitly needs depth.
                        let layers = if include_material && is_clear(&cart) {
                            let original = [
                                cart_interior(&cart),
                                cart_backing(gb_shell_of(&cart)),
                                cart_face(&cart),
                            ];
                            Some(original.map(|face| {
                                let (rgba,w,h)=slot_ui::native_rgba(&face.rgba,face.w,face.h);
                                CartFace{rgba,w,h}
                            }))
                        } else {None};
                        if sink.send((key,pixels,layers)).is_err() { break }
                    }
                })
                .map_err(|e| format!("Cannot start cartridge renderer: {e}"))?;
        }
        Ok(Self { submit, finished, pending: HashSet::new(), epochs: [0; 5] })
    }

    pub(crate) fn queued(&self, key: Key) -> bool {
        self.epochs.get(key.0).is_some_and(|epoch| self.pending.contains(&(key, *epoch)))
    }

    pub(crate) fn invalidate_shelf(&mut self, shelf: usize) {
        if let Some(epoch) = self.epochs.get_mut(shelf) {
            *epoch = epoch.wrapping_add(1);
            self.pending.retain(|(key, _)| key.0 != shelf);
        }
    }

    pub(crate) fn request(&mut self, key: Key, cart: &Cart, include_material: bool) {
        let Some(epoch) = self.epochs.get(key.0).copied() else { return };
        let work = (key, epoch);
        if self.pending.len() >= 8 || self.pending.contains(&work) { return }
        match self.submit.try_send((work, cart.clone(), include_material)) {
            Ok(()) => { self.pending.insert(work); }
            Err(TrySendError::Full(_)) => {} // retry next frame; never block input
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub(crate) fn ready(&mut self) -> Option<(Key, CartFace, Option<Layers>)> {
        loop {
            match self.finished.try_recv() {
                Ok(((key, epoch), face, layers))
                    if self.pending.remove(&(key, epoch)) &&
                        self.epochs.get(key.0).copied() == Some(epoch) =>
                    return Some((key, face, layers)),
                Ok(_) => continue,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_is_a_real_render_rack_and_discards_stale_results_on_reorder() {
        let (submit, jobs) = mpsc::sync_channel(3);
        let (completed, finished) = mpsc::channel();
        let mut renderer = CartRender {
            submit, finished, pending: HashSet::new(), epochs: [0; 5],
        };
        // A fresh fifth rack must schedule work, rather than leaving
        // fallback label-colour rectangles visible forever.
        let favorite = slot_store::Cart {
            platform: slot_store::Platform::Gba,
            stem: "Tetris".into(),
            title: String::new(), code: String::new(),
            rom: std::path::PathBuf::from("content://roms/tetris.gba"),
            label: None, shell: None,
        };
        renderer.request((4, 0), &favorite, false);
        assert!(renderer.queued((4, 0)));
        let (((key, epoch), _, _)) = jobs.try_recv().expect("FAVORITES job");
        assert_eq!(key, (4, 0));
        assert_eq!(epoch, 0);
        renderer.invalidate_shelf(4);
        assert!(!renderer.queued((4, 0)));
        renderer.request((4, 0), &favorite, false);
        assert!(renderer.queued((4, 0)));
        let (((new_key, new_epoch), _, _)) = jobs.try_recv().expect("new FAVORITES job");
        assert_eq!((new_key,new_epoch), ((4,0),1));
        let face = || CartFace { rgba: vec![255; 4], w: 1, h: 1 };
        completed.send((((4,0),0),face(),None)).unwrap();
        completed.send((((4,0),1),face(),None)).unwrap();
        assert_eq!(renderer.ready().map(|result|result.0),Some((4,0)));
        assert!(renderer.ready().is_none());
        assert!(!renderer.queued((4,0)));
    }

    #[test]
    fn rebuilding_recents_rejects_an_old_worker_face_with_same_index() {
        let (submit, _jobs) = mpsc::sync_channel(1);
        let (completed, finished) = mpsc::channel();
        let mut renderer = CartRender {
            submit, finished, pending: HashSet::new(), epochs: [0; 5],
        };
        renderer.pending.insert(((3, 0), 0));
        renderer.invalidate_shelf(3);
        assert!(!renderer.queued((3, 0)));
        renderer.pending.insert(((3, 0), 1));
        let face = || CartFace { rgba: vec![255; 4], w: 1, h: 1 };
        completed.send((((3, 0), 0), face(), None)).unwrap();
        completed.send((((3, 0), 1), face(), None)).unwrap();
        let accepted = renderer.ready().expect("fresh Recents face");
        assert_eq!(accepted.0, (3, 0));
        assert!(renderer.ready().is_none());
        assert!(!renderer.queued((3, 0)));
    }
}
