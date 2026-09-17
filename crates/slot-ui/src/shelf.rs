use std::collections::BTreeSet;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;

use crate::cart::{
    label_colour, label_text, CART_H, CART_W, GB_CART_H, GB_CART_W, GB_LABEL_W, GB_LABEL_X,
    GB_LABEL_Y, LABEL_W, LABEL_X, LABEL_Y,
};
use crate::hud::Millis;
use crate::slot_chrome::draw_empty_slot;

/// Distance between cart centres. Wider than a cart so the neighbours peek in at both
/// edges and the row reads as continuing past them.
/// Chosen so the outer two carts sit fully on screen with the margin at the edge equal to
/// the gap beside the centre cart. At 286 the side carts were clipped 24px off each edge.
const PITCH: f32 = 240.0;
const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
/// The top of a full-size cart centred in the output. Platform-specific shelves use this for
/// their own cartridge height; side carts keep the same foot while shrinking upward.
pub fn rest_y(h: f32) -> f32 {
    (OUT_H as f32 - h) / 2.0
}

pub fn foot_y(h: f32) -> f32 {
    rest_y(h) + h
}
/// Critically damped, so a flick lands on a cart instead of bouncing past and returning.
const OMEGA: f32 = 20.0;
/// How far the cart next to the selection is pushed aside as the chosen one goes in. Enough
/// to clear the frame from where it stands.
const PART: f32 = 130.0;

/// Slots considered either side of the selection. Two reach the edges of a 720 row, the
/// third covers the lag while the spring is still catching up with a flick.
const SLOTS: i32 = 3;

/// Before the first repeat. Long enough that a press meaning one cart cannot become two.
const REPEAT_DELAY_MS: Millis = 400;
/// Between repeats after that. Fast enough to cross a thirty cart library, slow enough to
/// stop on one.
const REPEAT_MS: Millis = 110;
const CATEGORY_COUNT: usize = 5;

fn cart_size(cart: &Cart) -> (u32, u32) {
    match cart.platform {
        slot_store::Platform::Gba => (CART_W, CART_H),
        slot_store::Platform::Gb | slot_store::Platform::Gbc => (GB_CART_W, GB_CART_H),
    }
}

fn same_cart(a: &Cart, b: &Cart) -> bool {
    a.platform == b.platform && a.stem == b.stem
}

/// Printed on the case when `Games/` is empty. Same type as a cart title, not a dialog.
pub const EMPTY_SHELF: &str = "no carts in Games/";

pub struct Shelf {
    pub carts: Vec<Cart>,
    all_carts: Vec<Cart>,
    pub index: usize,
    pub scroll: f32,
    faces: Vec<Option<TexId>>,
    all_faces: Vec<Option<TexId>>,
    /// Pixel dimensions of the uploaded face textures. Generated faces use the platform's
    /// normal geometry; complete artwork keeps its source aspect ratio after fitting width.
    face_sizes: Vec<(u32, u32)>,
    all_face_sizes: Vec<(u32, u32)>,
    /// Whether each uploaded face came from decodable complete artwork rather than the generated
    /// fallback. This remains separate from `Cart::artwork`, whose path may be malformed.
    complete_artwork: Vec<bool>,
    all_complete_artwork: Vec<bool>,
    platform_filter: Option<slot_store::Platform>,
    category: usize,
    /// The cart silhouette in black, drawn under a dimmed cart. One texture for the whole
    /// row: every cart is the same shape.
    shadow: Option<TexId>,
    gb_shadow: Option<TexId>,
    /// Platform-shaped placeholders used while the real face is being built off-thread. They
    /// are silhouettes rather than rectangles, so the ring remains legible while it hydrates.
    placeholder: Option<TexId>,
    gb_placeholder: Option<TexId>,
    /// The presses added up, in the same continuous coordinate `scroll` lives in, so it counts
    /// laps rather than wrapping. This is what the spring aims at — see `scroll_target` — because
    /// it is the only thing that remembers which button was pressed once the row has wrapped.
    ride: f32,
    vel: f32,
    /// The direction being held, when it next repeats, and the repeat count for acceleration.
    held: Option<(i32, Millis, u32)>,
    favorites: BTreeSet<String>,
    recents: Vec<String>,
    favorite_mark: Option<(TexId, u32, u32)>,
}

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        let sizes: Vec<(u32, u32)> = carts.iter().map(cart_size).collect();
        let cart_count = carts.len();
        Shelf {
            all_carts: carts.clone(),
            carts,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            all_faces: Vec::new(),
            face_sizes: sizes.clone(),
            all_face_sizes: sizes,
            complete_artwork: Vec::new(),
            all_complete_artwork: vec![false; cart_count],
            platform_filter: None,
            category: 0,
            shadow: None,
            gb_shadow: None,
            placeholder: None,
            gb_placeholder: None,
            ride: 0.0,
            vel: 0.0,
            held: None,
            favorites: BTreeSet::new(),
            recents: Vec::new(),
            favorite_mark: None,
        }
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.
    pub fn set_shadow(&mut self, face: TexId) {
        self.shadow = Some(face);
    }

    pub fn set_gb_shadow(&mut self, face: TexId) {
        self.gb_shadow = Some(face);
    }

    pub fn set_placeholder(&mut self, face: TexId) {
        self.placeholder = Some(face);
    }

    pub fn set_gb_placeholder(&mut self, face: TexId) {
        self.gb_placeholder = Some(face);
    }

    pub fn set_favorite_mark(&mut self, face: TexId, w: u32, h: u32) {
        self.favorite_mark = Some((face, w, h));
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        let faces = faces.into_iter().map(Some).collect::<Vec<_>>();
        self.all_faces = faces.clone();
        self.all_face_sizes = self.all_carts.iter().map(cart_size).collect();
        self.all_complete_artwork = vec![false; self.all_carts.len()];
        self.faces = faces;
        self.face_sizes = self.all_face_sizes.clone();
        self.complete_artwork = vec![false; self.carts.len()];
        self.set_category(self.category);
    }

    /// Publish one library face while the rest of the shelf still uses cheap colour
    /// placeholders. The compatibility helper resolves the first matching stem; asynchronous
    /// callers should use `set_face_for` so equal stems on different platforms stay distinct.
    pub fn set_face(&mut self, stem: &str, face: TexId) {
        let Some(cart) = self.all_carts.iter().find(|cart| cart.stem == stem) else {
            return;
        };
        self.set_face_for(cart.platform, stem, face, cart_size(cart), false);
    }

    /// Publish a face with its actual texture dimensions. The width remains the platform slot
    /// width, while complete artwork may be taller or shorter than the generated shell.
    pub fn set_face_with_size(&mut self, stem: &str, face: TexId, size: (u32, u32)) {
        self.set_face_with_size_and_artwork(stem, face, size, false);
    }

    /// Publish a face with its dimensions and whether it is a decodable complete-artwork image.
    /// The latter controls shell shadows and favorite-mark placement on the shelf.
    pub fn set_face_with_size_and_artwork(
        &mut self,
        stem: &str,
        face: TexId,
        size: (u32, u32),
        complete_artwork: bool,
    ) {
        let Some(platform) = self
            .all_carts
            .iter()
            .find(|cart| cart.stem == stem)
            .map(|cart| cart.platform)
        else {
            return;
        };
        self.set_face_for(platform, stem, face, size, complete_artwork);
    }

    /// Publish a face for the cart identified by both its platform and stem. Stems are allowed to
    /// repeat across platform folders, so the platform has to travel with an asynchronous build
    /// result rather than being reconstructed from the current shelf.
    pub fn set_face_for(
        &mut self,
        platform: slot_store::Platform,
        stem: &str,
        face: TexId,
        size: (u32, u32),
        complete_artwork: bool,
    ) {
        let Some(all_index) = self
            .all_carts
            .iter()
            .position(|cart| cart.platform == platform && cart.stem == stem)
        else {
            return;
        };
        if self.all_faces.len() < self.all_carts.len() {
            self.all_faces.resize(self.all_carts.len(), None);
        }
        if self.all_face_sizes.len() < self.all_carts.len() {
            self.all_face_sizes = self.all_carts.iter().map(cart_size).collect();
        }
        if self.all_complete_artwork.len() < self.all_carts.len() {
            self.all_complete_artwork = vec![false; self.all_carts.len()];
        }
        self.all_faces[all_index] = Some(face);
        self.all_face_sizes[all_index] = size;
        self.all_complete_artwork[all_index] = complete_artwork;

        // Face building happens asynchronously while the user can already browse. Do not
        // rebuild the category here: set_category also snaps the spring, clears its velocity,
        // and cancels button-repeat. A late texture upload must only replace the face at the
        // matching slot, never restart the motion currently visible on screen.
        if self.faces.len() < self.carts.len() {
            self.faces.resize(self.carts.len(), None);
        }
        if self.face_sizes.len() < self.carts.len() {
            self.face_sizes = self.carts.iter().map(cart_size).collect();
        }
        if self.complete_artwork.len() < self.carts.len() {
            self.complete_artwork = vec![false; self.carts.len()];
        }
        if let Some(current_index) = self
            .carts
            .iter()
            .position(|cart| cart.platform == platform && cart.stem == stem)
        {
            self.faces[current_index] = Some(face);
            self.face_sizes[current_index] = size;
            self.complete_artwork[current_index] = complete_artwork;
        }
    }

    pub fn all_carts(&self) -> &[Cart] {
        &self.all_carts
    }

    pub fn has_platform(&self, platform: slot_store::Platform) -> bool {
        self.all_carts.iter().any(|cart| cart.platform == platform)
    }

    /// Show one platform while retaining the complete library and its hydrated faces underneath.
    /// The application uses this for shoulder-based platform shelves; direct Shelf users keep the
    /// historical unfiltered view until they opt in.
    pub fn set_platform(&mut self, platform: Option<slot_store::Platform>) {
        if self.platform_filter == platform {
            return;
        }
        self.platform_filter = platform;
        self.set_category(0);
    }

    pub fn category(&self) -> usize {
        self.category
    }

    pub fn is_linear(&self) -> bool {
        self.category == 1
    }

    pub fn category_available(&self, category: usize) -> bool {
        match category {
            0 | 1 => true,
            2 => self
                .all_carts
                .iter()
                .any(|cart| cart.platform == slot_store::Platform::Gba),
            3 => self
                .all_carts
                .iter()
                .any(|cart| cart.platform == slot_store::Platform::Gb),
            4 => self
                .all_carts
                .iter()
                .any(|cart| cart.platform == slot_store::Platform::Gbc),
            _ => false,
        }
    }

    pub fn previous_category(&mut self) {
        for category in (0..self.category).rev() {
            if self.category_available(category) {
                self.set_category(category);
                return;
            }
        }
    }

    pub fn next_category(&mut self) {
        for category in (self.category + 1)..CATEGORY_COUNT {
            if self.category_available(category) {
                self.set_category(category);
                return;
            }
        }
    }

    /// Filename stems in most-recent-first order. Rebuild the current category because a
    /// game can become recent while its cart is still away from the shelf.
    pub fn set_recents(&mut self, mut recents: Vec<String>) {
        recents.truncate(slot_store::RECENTS_MAX);
        self.recents = recents;
        self.set_category(self.category);
    }

    /// Rebuild the visible row for a category index. Used by boot restore and by the
    /// shoulder category controls.
    pub fn set_category(&mut self, category: usize) {
        let selected = self.carts.get(self.index).map(|c| c.stem.clone());
        self.category = category;
        let wanted = |cart: &Cart| {
            self.platform_filter
                .is_none_or(|platform| cart.platform == platform)
                && (category == 0
                    || match category {
                        1 => self.recents.contains(&cart.stem),
                        2 => cart.platform == slot_store::Platform::Gba,
                        3 => cart.platform == slot_store::Platform::Gb,
                        4 => cart.platform == slot_store::Platform::Gbc,
                        _ => false,
                    })
        };
        self.carts = self
            .all_carts
            .iter()
            .filter(|cart| wanted(cart))
            .cloned()
            .collect();
        self.faces = self
            .all_carts
            .iter()
            .enumerate()
            .filter(|(_, cart)| wanted(cart))
            .map(|(i, _)| self.all_faces.get(i).copied().flatten())
            .collect();
        self.face_sizes = self
            .all_carts
            .iter()
            .enumerate()
            .filter(|(_, cart)| wanted(cart))
            .map(|(i, cart)| {
                self.all_face_sizes
                    .get(i)
                    .copied()
                    .unwrap_or_else(|| cart_size(cart))
            })
            .collect();
        self.complete_artwork = self
            .all_carts
            .iter()
            .enumerate()
            .filter(|(_, cart)| wanted(cart))
            .map(|(i, _)| self.all_complete_artwork.get(i).copied().unwrap_or(false))
            .collect();
        self.sort_by_favorites(&self.favorites.clone());
        self.index = selected
            .and_then(|stem| self.carts.iter().position(|cart| cart.stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
        self.ride = self.index as f32;
        self.vel = 0.0;
        self.held = None;
    }

    /// In `hints` order.
    pub fn find(&self, stem: &str) -> Option<(&Cart, Option<TexId>)> {
        self.find_with_size(stem)
            .map(|(cart, face, _)| (cart, face))
    }

    /// Finds a cart and reports the dimensions of the texture that will be drawn for it. A
    /// complete artwork face can have a different height after being fitted to slot width.
    pub fn find_with_size(&self, stem: &str) -> Option<(&Cart, Option<TexId>, (u32, u32))> {
        let i = self.carts.iter().position(|c| c.stem == stem)?;
        let cart = &self.carts[i];
        let uploaded = self.faces.get(i).copied().flatten();
        let face = uploaded.or_else(|| {
            Some(match cart.platform {
                slot_store::Platform::Gba => self.placeholder?,
                slot_store::Platform::Gb | slot_store::Platform::Gbc => self.gb_placeholder?,
            })
        });
        let size = if uploaded.is_some() {
            self.face_sizes
                .get(i)
                .copied()
                .unwrap_or_else(|| cart_size(cart))
        } else {
            cart_size(cart)
        };
        Some((cart, face, size))
    }

    /// Carts nearest the current selection first, then the rest of the library. The shelf is a
    /// ring: the visible tail when index zero is selected is the last cart, so a plain FIFO
    /// leaves exactly the end of the row as rectangles during boot.
    pub fn face_upload_order(&self) -> Vec<Cart> {
        let mut carts = Vec::with_capacity(self.all_carts.len());
        if !self.carts.is_empty() {
            let n = self.carts.len() as i32;
            let mut offsets = Vec::with_capacity(self.carts.len());
            offsets.push(0);
            let max_dist = if self.is_linear() { n } else { n / 2 };
            for distance in 1..=max_dist {
                offsets.push(-distance);
                offsets.push(distance);
            }
            for offset in offsets {
                let Some(index) = self.cart_at_offset(offset) else {
                    continue;
                };
                let cart = &self.carts[index];
                if !carts.iter().any(|seen: &Cart| same_cart(seen, cart)) {
                    carts.push(cart.clone());
                }
            }
        }
        for cart in &self.all_carts {
            if !carts.iter().any(|seen| same_cart(seen, cart)) {
                carts.push(cart.clone());
            }
        }
        carts
    }

    /// Priority order for asset hydration: visible carts in the ring around the current
    /// selection first (alternating left/right), followed by off-screen carts. Zero-alloc.
    pub fn cart_upload_priority(&self, stem: &str) -> usize {
        self.cart_upload_priority_for(None, stem)
    }

    /// Priority for a specific cart. The optional platform disambiguates equal stems when the
    /// card contains the same filename in more than one platform folder.
    pub fn cart_upload_priority_for(
        &self,
        platform: Option<slot_store::Platform>,
        stem: &str,
    ) -> usize {
        if let Some(pos) = self
            .carts
            .iter()
            .position(|c| c.stem == stem && platform.is_none_or(|p| c.platform == p))
        {
            let n = self.carts.len() as i32;
            if n <= 1 {
                return 0;
            }
            if self.is_linear() {
                let diff = (pos as i32 - self.index as i32).abs();
                return (diff * 2) as usize;
            }
            let diff = (pos as i32 - self.index as i32).rem_euclid(n);
            let signed = if diff * 2 > n { diff - n } else { diff };
            if signed == 0 {
                0
            } else if signed < 0 {
                (-signed * 2 - 1) as usize
            } else {
                (signed * 2) as usize
            }
        } else if let Some(all_pos) = self
            .all_carts
            .iter()
            .position(|c| c.stem == stem && platform.is_none_or(|p| c.platform == p))
        {
            self.carts.len() * 2 + all_pos
        } else {
            usize::MAX
        }
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
    }

    /// Select by current cart index. This is useful to deterministic callers that already own
    /// the shelf order; application code should prefer `select_stem` because filters reorder it.
    pub fn select(&mut self, index: usize) {
        if index >= self.carts.len() {
            return;
        }
        self.index = index;
        self.scroll = index as f32;
        self.ride = index as f32;
        self.vel = 0.0;
        self.held = None;
    }

    /// Select a cart by its stable filename stem. Indices can change when a category or the
    /// favourite order is rebuilt, so callers restoring the shelf must use the stem instead.
    pub fn select_stem(&mut self, stem: &str) -> bool {
        let Some(index) = self.carts.iter().position(|cart| cart.stem == stem) else {
            return false;
        };
        self.index = index;
        self.scroll = index as f32;
        self.ride = index as f32;
        self.vel = 0.0;
        self.held = None;
        true
    }

    /// Move to the first cart whose initial differs from the selected cart's. Carts are
    /// sorted by stem, so this skips the rest of the current letter in one press. The row is
    /// circular just like ordinary browsing: right from the last letter reaches the first,
    /// and left from the first reaches the last.
    pub fn next_letter(&mut self) {
        self.jump_letter(1);
    }

    pub fn previous_letter(&mut self) {
        self.jump_letter(-1);
    }

    /// Put favourites first without losing the texture paired with each cart or changing
    /// which cart is selected. The jump is immediate because the row itself was reordered;
    /// animating through every cart between the old and new indices would imply browsing.
    pub fn sort_by_favorites(&mut self, favorites: &BTreeSet<String>) {
        let selected = self.carts.get(self.index).map(|cart| cart.stem.clone());
        let have_faces = self.faces.len() == self.carts.len();
        let faces = have_faces.then(|| std::mem::take(&mut self.faces));
        let sizes = std::mem::take(&mut self.face_sizes);
        let complete_artwork = std::mem::take(&mut self.complete_artwork);
        let mut paired: Vec<_> = std::mem::take(&mut self.carts)
            .into_iter()
            .enumerate()
            .map(|(i, cart)| {
                let size = sizes.get(i).copied().unwrap_or_else(|| cart_size(&cart));
                let complete_artwork = complete_artwork.get(i).copied().unwrap_or(false);
                (
                    cart,
                    faces
                        .as_ref()
                        .and_then(|faces| faces.get(i).copied())
                        .flatten(),
                    size,
                    complete_artwork,
                )
            })
            .collect();
        paired.sort_by(|(a, _, _, _), (b, _, _, _)| {
            if self.category == 1 {
                let rank = |stem: &str| {
                    self.recents
                        .iter()
                        .position(|recent| recent == stem)
                        .unwrap_or(usize::MAX)
                };
                rank(&a.stem).cmp(&rank(&b.stem))
            } else {
                favorites
                    .contains(&b.stem)
                    .cmp(&favorites.contains(&a.stem))
                    .then_with(|| a.stem.cmp(&b.stem))
            }
        });
        if self.category == 1 {
            paired.truncate(slot_store::RECENTS_MAX);
        }
        self.carts = paired.iter().map(|(cart, _, _, _)| cart.clone()).collect();
        if have_faces {
            self.faces = paired.iter().map(|(_, face, _, _)| *face).collect();
        }
        self.face_sizes = paired.iter().map(|(_, _, size, _)| *size).collect();
        self.complete_artwork = paired
            .iter()
            .map(|(_, _, _, complete_artwork)| *complete_artwork)
            .collect();
        self.index = selected
            .and_then(|stem| self.carts.iter().position(|cart| cart.stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
        self.ride = self.index as f32;
        self.vel = 0.0;
        self.held = None;
        self.favorites = favorites.clone();
    }

    pub fn hold_left(&mut self, now: Millis) {
        self.hold(-1, now);
    }

    pub fn hold_right(&mut self, now: Millis) {
        self.hold(1, now);
    }

    /// The press moves a cart itself, so the repeat is what the delay is measured from
    /// rather than what it produces.
    fn hold(&mut self, by: i32, now: Millis) {
        self.step(by);
        self.held = Some((by, now + REPEAT_DELAY_MS, 0));
    }

    pub fn release_left(&mut self) {
        self.release(-1);
    }

    pub fn release_right(&mut self) {
        self.release(1);
    }

    /// Only the direction that is being held stops it. Letting go of the other one is a
    /// change of direction the shelf has already acted on.
    fn release(&mut self, by: i32) {
        if matches!(self.held, Some((held, _, _)) if held == by) {
            self.held = None;
        }
    }

    /// Whatever is held, let go of. Nothing on screen is holding it.
    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Fires the repeat. Due from `now` rather than from the deadline it passed, so a frame
    /// the app was late for costs one cart instead of a burst of catching up.
    /// Progressively accelerates the repeat rate as the direction continues to be held.
    pub fn tick(&mut self, now: Millis) {
        let Some((by, due, count)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        let next_interval = match count {
            0..=1 => REPEAT_MS,
            2..=3 => 85,
            4..=6 => 65,
            _ => 50,
        };
        self.held = Some((by, now + next_interval, count + 1));
    }

    fn step(&mut self, by: i32) {
        let n = self.carts.len();
        if n == 0 {
            return;
        }
        if self.is_linear() {
            let next = (self.index as i32 + by).clamp(0, n as i32 - 1);
            self.index = next as usize;
            self.ride = self.index as f32;
        } else {
            self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
            self.ride += by as f32;
        }
    }

    fn jump_letter(&mut self, by: i32) {
        let n = self.carts.len();
        if n < 2 {
            return;
        }
        if self.is_linear() {
            let target = if by < 0 { 0 } else { n - 1 };
            self.index = target;
            self.ride = target as f32;
            self.held = None;
            return;
        }
        let initial = |i: usize| {
            self.carts[i]
                .stem
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
        };
        let current = initial(self.index);
        for distance in 1..n {
            let mut candidate =
                (self.index as i32 + by * distance as i32).rem_euclid(n as i32) as usize;
            if initial(candidate) != current {
                // Going left first encounters the end of the previous letter's run. Land on
                // its beginning, matching the first cart R1 reaches when travelling right.
                if by < 0 {
                    let letter = initial(candidate);
                    loop {
                        let before = (candidate + n - 1) % n;
                        if before == self.index || initial(before) != letter {
                            break;
                        }
                        candidate = before;
                    }
                }
                self.index = candidate;
                self.held = None;
                // Keep `ride` on the short arc after D-pad wraps have carried it off the
                // bare index, or L1/R1 would send the spring the long way round the row.
                let from = self.ride;
                let n_f = n as f32;
                let shortest =
                    (candidate as f32 - from + n_f / 2.0).rem_euclid(n_f) - n_f / 2.0;
                self.ride = from + shortest;
                return;
            }
        }
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row is a
    /// ring, so the selected cart has an image every `n` slots, and the one to head for is the
    /// one a single press away in the direction that press asked for — never a lap of the row.
    ///
    /// Adding the presses up answers it for every length at once. `ride` counts laps instead of
    /// wrapping, so one press is one slot the way it was pressed whatever the row is doing at the
    /// time, and the row still never unwinds: a single step round a ring *is* the short way round.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        if self.is_linear() {
            return self.index as f32;
        }
        let from = self.ride;
        let n = n as f32;
        from + (self.index as f32 - from + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection, or `None` when the row is empty or when this
    /// slot falls off the end of a row too short to reach it.
    ///
    /// A ring of two fills every slot, which means one of the two carts is drawn twice at once.
    /// One cart stays alone in the middle — repeating it would put three identical faces across a
    /// row that cannot scroll.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        if self.is_linear() {
            let target = self.index as i32 + off;
            return (target >= 0 && target < n).then_some(target as usize);
        }
        let at = |off: i32| (self.index as i32 + off).rem_euclid(n) as usize;
        if n == 2 {
            return Some(at(off));
        }
        let r = off.rem_euclid(n);
        let nearest = if r * 2 > n { r - n } else { r };
        (nearest == off).then(|| at(off))
    }

    pub fn update(&mut self, dt: f32) {
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
        let target = self.scroll_target();
        if (self.scroll - target).abs() < 1e-4 && self.vel.abs() < 1e-3 {
            self.scroll = target;
            self.vel = 0.0;
        }
    }

    /// The shelf screen: the row of carts and the slot under it. What is printed on the case
    /// is drawn after this, by whoever holds the type.
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_with_hold(shake, 0.0, out);
    }

    pub fn draw_with_hold(&self, shake: f32, hold_progress: f32, out: &mut Vec<Draw>) {
        self.draw_row_with_hold(None, shake, 0.0, 1.0, hold_progress, out);
        draw_empty_slot(out);
    }

    /// The row alone. The cart on its way into the slot is drawn by the chrome, at the same
    /// place the row would draw it; leaving it in the row as well puts two of one cart on
    /// screen and the travel then reads as a copy sliding away from the original.
    ///
    /// `shake` displaces the carts and nothing else. On the shelf the frame is mostly
    /// backdrop, so shaking that slides the letterbox in at the edges rather than reading as
    /// a refusal.
    ///
    /// `recede` clears the row for the cart going into the slot: 0.0 leaves it alone, 1.0
    /// has every other cart gone. They part outwards rather than fading in place, so the row
    /// reads as making way for the one that was chosen.
    ///
    /// `dim` darkens the faces further and nothing else: 1.0 leaves them as `recede` has them.
    /// The black under a dimmed cart stays as `recede` alone makes it, so a dimmed cart reads
    /// as a cart in shadow rather than a ghost over the wallpaper.
    pub fn draw_row(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        out: &mut Vec<Draw>,
    ) {
        self.draw_row_with_hold(hidden, shake, recede, dim, 0.0, out);
    }

    pub fn draw_row_with_hold(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        hold_progress: f32,
        out: &mut Vec<Draw>,
    ) {
        let recede = recede.clamp(0.0, 1.0);
        let dim = dim.clamp(0.0, 1.0);
        let target = self.scroll_target();
        for slot in -SLOTS..=SLOTS {
            let Some(i) = self.cart_at_offset(slot) else {
                continue;
            };
            let cart = &self.carts[i];
            if hidden == Some(cart.stem.as_str()) {
                continue;
            }
            let offset = target + slot as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let scale = 1.0 + (SIDE_SCALE - 1.0) * t;
            let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (base_w, base_h) = cart_size(cart);
            let uploaded = self.faces.get(i).copied().flatten().is_some();
            let complete_artwork =
                uploaded && self.complete_artwork.get(i).copied().unwrap_or(false);
            let face_h = if uploaded {
                self.face_sizes.get(i).map_or(base_h, |(_, h)| *h)
            } else {
                base_h
            };
            let (w, h) = (base_w as f32 * scale, face_h as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * PITCH - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            let x = x + shake;
            let foot_y = (OUT_H + base_h) as f32 / 2.0;
            let dip = if slot == 0 {
                (hold_progress * 6.0).round()
            } else {
                0.0
            };
            let y = foot_y - h + dip;
            // Black in the cart's own shape, under the dimmed face. Without it the dimming is
            // transparency, and over a wallpaper the row reads as ghosts of carts.
            // Complete artwork carts have their own arbitrary outline, so the standard shell
            // silhouette does not match and would bleed out around the image.
            if !complete_artwork && alpha < 1.0 {
                if let Some(tex) = if cart.platform == slot_store::Platform::Gba {
                    self.shadow
                } else {
                    self.gb_shadow
                } {
                    out.push(Draw::Tex {
                        x,
                        y,
                        w,
                        h,
                        tex,
                        alpha: recede_alpha(alpha),
                    });
                }
            }
            let placeholder = match cart.platform {
                slot_store::Platform::Gba => self.placeholder,
                slot_store::Platform::Gb | slot_store::Platform::Gbc => self.gb_placeholder,
            };
            let (draw_w, draw_h) = if uploaded {
                (w, h)
            } else {
                (base_w as f32 * scale, base_h as f32 * scale)
            };
            out.push(match self.faces.get(i).copied().flatten().or(placeholder) {
                Some(tex) => Draw::Tex {
                    x,
                    y,
                    w: draw_w,
                    h: draw_h,
                    tex,
                    alpha: alpha * dim,
                },
                // A cart whose face has not been uploaded still holds its place. A gap in
                // the row would read as a missing game.
                None => {
                    let c = label_colour(&label_text(cart));
                    Draw::Rect {
                        x,
                        y,
                        w: draw_w,
                        h: draw_h,
                        colour: [
                            c[0] as f32 / 255.0,
                            c[1] as f32 / 255.0,
                            c[2] as f32 / 255.0,
                            alpha * dim,
                        ],
                    }
                }
            });
            if self.favorites.contains(&cart.stem) {
                if let Some((tex, mw, mh)) = self.favorite_mark {
                    let (mw, mh) = (mw as f32 * scale, mh as f32 * scale);
                    let inset = 6.0 * scale;
                    let (mark_x, mark_y) = if complete_artwork {
                        // A complete cartridge image has no generated label panel to anchor
                        // this badge to; keep the favorite mark in its own top-right corner.
                        (x + w - mw - inset, y + inset)
                    } else {
                        let (label_x, label_y, label_w) = match cart.platform {
                            slot_store::Platform::Gba => (LABEL_X, LABEL_Y, LABEL_W),
                            slot_store::Platform::Gb | slot_store::Platform::Gbc => {
                                (GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W)
                            }
                        };
                        (
                            x + (label_x + label_w) as f32 * scale - mw - inset,
                            y + label_y as f32 * scale + inset,
                        )
                    };
                    out.push(Draw::Tex {
                        x: mark_x,
                        y: mark_y,
                        w: mw,
                        h: mh,
                        tex,
                        alpha: alpha * dim,
                    });
                }
            }
        }
    }
}

/// How solid the shadow under a dimmed cart is. It carries the whole of the cart's opacity
/// while the face is translucent over it, and leaves with the face as the row parts.
fn recede_alpha(face_alpha: f32) -> f32 {
    (face_alpha / SIDE_ALPHA).clamp(0.0, 1.0)
}
