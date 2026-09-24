//! The open cart's faces, built off the render thread: a board takes ~0.5 s to rasterise on the
//! H700. Only the newest request matters.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slot_store::Cart;
use slot_ui::{board_face, cart_face, padded, CartFace, TURN_PAD};

pub struct BuiltFaces {
    pub stem: String,
    pub board: CartFace,
    pub lid: CartFace,
}

pub struct FaceBuilder {
    requests: Sender<Cart>,
    built: Receiver<BuiltFaces>,
}

impl FaceBuilder {
    pub fn spawn() -> Self {
        let (requests, inbox) = mpsc::channel::<Cart>();
        let (outbox, built) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("slot-faces".into())
            .spawn(move || {
                while let Ok(mut cart) = inbox.recv() {
                    // Skip to the newest: earlier requests are for carts the caret has left.
                    while let Ok(newer) = inbox.try_recv() {
                        cart = newer;
                    }
                    let faces = BuiltFaces {
                        stem: cart.stem.clone(),
                        board: board_face(&cart),
                        lid: padded(&cart_face(&cart), TURN_PAD),
                    };
                    if outbox.send(faces).is_err() {
                        return;
                    }
                }
            });
        // Not fatal: `App` gives up waiting after `FACES_WAIT_MS`, same as a slow worker.
        if let Err(e) = spawned {
            eprintln!("slot: faces: worker thread failed to start: {e}");
        }
        FaceBuilder { requests, built }
    }

    pub fn request(&self, cart: Cart) {
        // A worker that has gone has nothing to build with; the picker's own wait gives up.
        let _ = self.requests.send(cart);
    }

    /// The newest build finished since the last call, if any.
    pub fn take(&self) -> Option<BuiltFaces> {
        let mut newest = None;
        while let Ok(faces) = self.built.try_recv() {
            newest = Some(faces);
        }
        newest
    }
}
