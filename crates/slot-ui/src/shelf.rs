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
/// Carts stand on the row rather than float: the foot stays put as a cart shrinks away.
pub(crate) const FOOT_Y: f32 = (OUT_H + CART_H) as f32 / 2.0;
/// Critically damped, so a flick lands on a cart instead of bouncing past and returning.
const OMEGA: f32 = 16.0;
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

/// Printed on the case when `Games/` is empty. Same type as a cart title, not a dialog.
pub const EMPTY_SHELF: &str = "no carts in Games/";

pub struct Shelf {
    pub carts: Vec<Cart>,
    all_carts: Vec<Cart>,
    pub index: usize,
    pub scroll: f32,
    faces: Vec<Option<TexId>>,
    all_faces: Vec<Option<TexId>>,
    category: usize,
    /// The cart silhouette in black, drawn under a dimmed cart. One texture for the whole
    /// row: every cart is the same shape.
    shadow: Option<TexId>,
    gb_shadow: Option<TexId>,
    /// Platform-shaped placeholders used while the real face is being built off-thread. They
    /// are silhouettes rather than rectangles, so the ring remains legible while it hydrates.
    placeholder: Option<TexId>,
    gb_placeholder: Option<TexId>,
    vel: f32,
    /// The direction being held and when it next repeats. Repeat lives here rather than in
    /// the gesture layer so nothing in game starts auto firing.
    held: Option<(i32, Millis)>,
    favorites: BTreeSet<String>,
    recents: Vec<String>,
    favorite_mark: Option<(TexId, u32, u32)>,
}

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        Shelf {
            all_carts: carts.clone(),
            carts,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            all_faces: Vec::new(),
            category: 0,
            shadow: None,
            gb_shadow: None,
            placeholder: None,
            gb_placeholder: None,
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
        self.faces = faces;
        self.set_category(self.category);
    }

    /// Publish one library face while the rest of the shelf still uses cheap colour
    /// placeholders. Resolve by stem so a favorite/category reorder during hydration cannot
    /// pair a texture with the wrong cart.
    pub fn set_face(&mut self, stem: &str, face: TexId) {
        let Some(all_index) = self.all_carts.iter().position(|cart| cart.stem == stem) else {
            return;
        };
        if self.all_faces.len() < self.all_carts.len() {
            self.all_faces.resize(self.all_carts.len(), None);
        }
        self.all_faces[all_index] = Some(face);

        // Face building happens asynchronously while the user can already browse. Do not
        // rebuild the category here: set_category also snaps the spring, clears its velocity,
        // and cancels button-repeat. A late texture upload must only replace the face at the
        // matching slot, never restart the motion currently visible on screen.
        if self.faces.len() < self.carts.len() {
            self.faces.resize(self.carts.len(), None);
        }
        if let Some(current_index) = self.carts.iter().position(|cart| cart.stem == stem) {
            self.faces[current_index] = Some(face);
        }
    }

    pub fn all_carts(&self) -> &[Cart] {
        &self.all_carts
    }

    pub fn category(&self) -> usize {
        self.category
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
        for distance in 1..CATEGORY_COUNT {
            let category = (self.category + CATEGORY_COUNT - distance) % CATEGORY_COUNT;
            if self.category_available(category) {
                self.set_category(category);
                break;
            }
        }
    }

    pub fn next_category(&mut self) {
        for distance in 1..CATEGORY_COUNT {
            let category = (self.category + distance) % CATEGORY_COUNT;
            if self.category_available(category) {
                self.set_category(category);
                break;
            }
        }
    }

    /// Filename stems in most-recent-first order. Rebuild the current category because a
    /// game can become recent while its cart is still away from the shelf.
    pub fn set_recents(&mut self, recents: Vec<String>) {
        self.recents = recents;
        self.set_category(self.category);
    }

    fn set_category(&mut self, category: usize) {
        let selected = self.carts.get(self.index).map(|c| c.stem.clone());
        self.category = category;
        let wanted = |cart: &Cart| {
            category == 0
                || match category {
                    1 => self.recents.contains(&cart.stem),
                    2 => cart.platform == slot_store::Platform::Gba,
                    3 => cart.platform == slot_store::Platform::Gb,
                    4 => cart.platform == slot_store::Platform::Gbc,
                    _ => false,
                }
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
            .zip(self.all_faces.iter().copied())
            .filter(|(cart, _)| wanted(cart))
            .map(|(_, face)| face)
            .collect();
        self.sort_by_favorites(&self.favorites.clone());
        self.index = selected
            .and_then(|stem| self.carts.iter().position(|cart| cart.stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
        self.vel = 0.0;
        self.held = None;
    }

    /// In `hints` order.
    pub fn find(&self, stem: &str) -> Option<(&Cart, Option<TexId>)> {
        let i = self.carts.iter().position(|c| c.stem == stem)?;
        let cart = &self.carts[i];
        let face = self.faces.get(i).copied().flatten().or_else(|| {
            Some(match cart.platform {
                slot_store::Platform::Gba => self.placeholder?,
                slot_store::Platform::Gb | slot_store::Platform::Gbc => self.gb_placeholder?,
            })
        });
        Some((cart, face))
    }

    /// Carts nearest the current selection first, then the rest of the library. The shelf is a
    /// ring: the visible tail when index zero is selected is the last cart, so a plain FIFO
    /// leaves exactly the end of the row as rectangles during boot.
    pub fn face_upload_order(&self) -> Vec<Cart> {
        let mut stems = Vec::with_capacity(self.all_carts.len());
        if !self.carts.is_empty() {
            let n = self.carts.len() as i32;
            let mut offsets = Vec::with_capacity(self.carts.len());
            offsets.push(0);
            for distance in 1..=n / 2 {
                offsets.push(-distance);
                offsets.push(distance);
            }
            for offset in offsets {
                let Some(index) = self.cart_at_offset(offset) else {
                    continue;
                };
                let stem = &self.carts[index].stem;
                if !stems.iter().any(|seen| seen == stem) {
                    stems.push(stem.clone());
                }
            }
        }
        for cart in &self.all_carts {
            if !stems.iter().any(|seen| seen == &cart.stem) {
                stems.push(cart.stem.clone());
            }
        }
        stems
            .into_iter()
            .filter_map(|stem| {
                self.all_carts
                    .iter()
                    .find(|cart| cart.stem == stem)
                    .cloned()
            })
            .collect()
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
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
        let mut paired: Vec<_> = std::mem::take(&mut self.carts)
            .into_iter()
            .enumerate()
            .map(|(i, cart)| {
                (
                    cart,
                    faces
                        .as_ref()
                        .and_then(|faces| faces.get(i).copied())
                        .flatten(),
                )
            })
            .collect();
        paired.sort_by(|(a, _), (b, _)| {
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
        self.carts = paired.iter().map(|(cart, _)| cart.clone()).collect();
        if have_faces {
            self.faces = paired.iter().map(|(_, face)| *face).collect();
        }
        self.index = selected
            .and_then(|stem| self.carts.iter().position(|cart| cart.stem == stem))
            .unwrap_or(0);
        self.scroll = self.index as f32;
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
        self.held = Some((by, now + REPEAT_DELAY_MS));
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
        if matches!(self.held, Some((held, _)) if held == by) {
            self.held = None;
        }
    }

    /// Whatever is held, let go of. Nothing on screen is holding it.
    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Fires the repeat. Due from `now` rather than from the deadline it passed, so a frame
    /// the app was late for costs one cart instead of a burst of catching up.
    pub fn tick(&mut self, now: Millis) {
        let Some((by, due)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        self.held = Some((by, now + REPEAT_MS));
    }

    fn step(&mut self, by: i32) {
        let n = self.carts.len();
        if n == 0 {
            return;
        }
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
    }

    fn jump_letter(&mut self, by: i32) {
        let n = self.carts.len();
        if n < 2 {
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
                return;
            }
        }
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row
    /// is a ring, so the selected cart has an image every `n` slots; this is the one nearest
    /// where the row already is, which is what stops a wrap unwinding the whole row.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        let n = n as f32;
        self.scroll + (self.index as f32 - self.scroll + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection. `None` when the row is empty, or when
    /// this slot would repeat a cart another slot is already showing: with two carts the
    /// left and right neighbours are the same one, and a row holding it twice reads as a
    /// bug. The row is left with a gap instead.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        let r = off.rem_euclid(n);
        let nearest = if r * 2 > n { r - n } else { r };
        (nearest == off).then(|| (self.index as i32 + off).rem_euclid(n) as usize)
    }

    pub fn update(&mut self, dt: f32) {
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
    }

    /// The shelf screen: the row of carts and the slot under it. What is printed on the case
    /// is drawn after this, by whoever holds the type.
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_row(None, shake, 0.0, 1.0, out);
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
            let (base_w, base_h) = match cart.platform {
                slot_store::Platform::Gba => (CART_W, CART_H),
                slot_store::Platform::Gb | slot_store::Platform::Gbc => (GB_CART_W, GB_CART_H),
            };
            let (w, h) = (base_w as f32 * scale, base_h as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * PITCH - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            let x = x + shake;
            let foot_y = (OUT_H + base_h) as f32 / 2.0;
            let y = foot_y - h;
            // Black in the cart's own shape, under the dimmed face. Without it the dimming is
            // transparency, and over a wallpaper the row reads as ghosts of carts.
            if alpha < 1.0 {
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
            out.push(match self.faces.get(i).copied().flatten().or(placeholder) {
                Some(tex) => Draw::Tex {
                    x,
                    y,
                    w,
                    h,
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
                        w,
                        h,
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
                    let (label_x, label_y, label_w) = match cart.platform {
                        slot_store::Platform::Gba => (LABEL_X, LABEL_Y, LABEL_W),
                        slot_store::Platform::Gb | slot_store::Platform::Gbc => {
                            (GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W)
                        }
                    };
                    out.push(Draw::Tex {
                        x: x + (label_x + label_w) as f32 * scale - mw - inset,
                        y: y + label_y as f32 * scale + inset,
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
