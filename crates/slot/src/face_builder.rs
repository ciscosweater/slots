//! The open cart's faces, built off the render thread. On the H700 a board takes the better part
//! of half a second to rasterise, which on the frame loop is half a second of frozen shelf, so
//! the frontend asks for the highlighted cart's faces as the caret lands and uploads them when
//! they come back. Only the newest request matters: a caret that has moved on has no use for the
//! cart it passed.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slot_store::Cart;
use slot_ui::{
    board_face, cart_face, cart_face_with_artwork, clean_label, padded, title_face, word_face,
    CartFace, UndoFace, TURN_PAD,
};

pub struct BuiltFaces {
    pub stem: String,
    pub board: CartFace,
    pub lid: CartFace,
}

pub struct FaceBuilder {
    requests: Sender<Cart>,
    built: Receiver<BuiltFaces>,
}

/// A shelf cart's complete visual payload. Rasterising a cart (and its two captions) touches
/// the label image and font for every entry, so it must never happen on the render thread.
pub struct BuiltShelfFace {
    pub platform: slot_store::Platform,
    pub stem: String,
    pub face: CartFace,
    pub complete_artwork: bool,
    pub group: UndoFace,
    pub title: UndoFace,
}

/// Builds the library faces away from the frame loop. Requests are deliberately FIFO: the
/// frontend chooses their order, putting the carts visible at the two ends of the ring first.
pub struct ShelfFaceBuilder {
    requests: Sender<Cart>,
    built: Receiver<BuiltShelfFace>,
}

impl ShelfFaceBuilder {
    pub fn spawn() -> Self {
        let (requests, inbox) = mpsc::channel::<Cart>();
        let (outbox, built) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("slot-shelf-faces".into())
            .spawn(move || {
                while let Ok(cart) = inbox.recv() {
                    let clean = clean_label(&cart.stem);
                    let group = clean
                        .chars()
                        .next()
                        .map(|c| c.to_ascii_uppercase().to_string())
                        .unwrap_or_else(|| "#".to_string());
                    let (face, complete_artwork) = cart_face_with_artwork(&cart);
                    let faces = BuiltShelfFace {
                        platform: cart.platform,
                        stem: cart.stem.clone(),
                        face,
                        complete_artwork,
                        group: word_face(&group),
                        title: title_face(&clean),
                    };
                    if outbox.send(faces).is_err() {
                        return;
                    }
                }
            });
        if let Err(e) = spawned {
            eprintln!("slot: shelf faces: worker thread failed to start: {e}");
        }
        ShelfFaceBuilder { requests, built }
    }

    /// Returns false when the worker has already gone away, so the caller can clear its
    /// in-flight bit instead of waiting forever for a result that cannot arrive.
    pub fn request(&self, cart: Cart) -> bool {
        self.requests.send(cart).is_ok()
    }

    pub fn take(&self) -> Option<BuiltShelfFace> {
        self.built.try_recv().ok()
    }
}

impl FaceBuilder {
    pub fn spawn() -> Self {
        let (requests, inbox) = mpsc::channel::<Cart>();
        let (outbox, built) = mpsc::channel();
        // Named and built the way every other worker in this crate is (`link_start`,
        // `rewind`, `emu`, `audio/host`): a `Builder` rather than the bare `thread::spawn`, so
        // a `ps`/`top` on the device names this thread instead of just another anonymous one.
        let spawned = thread::Builder::new()
            .name("slot-faces".into())
            .spawn(move || {
                while let Ok(mut cart) = inbox.recv() {
                    // Straight to the newest: the requests before it were for carts the caret
                    // has already left.
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
        // A thread that never started leaves both ends of `inbox`/`outbox` dropped with it, so
        // `request` below sends into a channel nobody drains and `take` only ever sees it
        // disconnected. `App` can already open with the shelf face as a fallback, so this is
        // reported rather than turned into a panic that would take the whole frontend down.
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
