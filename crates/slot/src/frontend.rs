//! Everything the binary does with a compositor except own one. The window is the only
//! difference between the host and the device, so it is the only thing left above this.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use slot_gfx::{Compositor, Draw, TexId, OUT_H, OUT_W};
use slot_input::{InputSource, Millis};
use slot_power::{Platform, Power};
use slot_store::format_stamp;
use slot_ui::{
    arrows_hint_face, badge_face, cart_placeholder_for, cart_shadow, cart_shadow_for,
    category_tab_face, chip_face, chip_shadow_face, date_time_text, favorite_mark_face, hhmm,
    hint_face, icon_face, menu_face, photo_face, quick_caret_face, quick_label_face,
    quick_legend_faces, quick_value_face, set_clock_hint_face, socket_face, sticker_face,
    title_face, toast_face, word_face, Icon, LinkBadge, PowerChoice, Printed, QuickMenuFaces,
    QuickRow, QuickValue, StickerFields, StickerPage, Toast, UndoFace, ALERT_PX, BOLT_PX, CART_H,
    CART_W, EMPTY_SHELF, HUD_ICON_PX, HUD_INK, LEGEND, TURN_PAD,
};

use crate::app::{App, LinkRow, Phase};
use crate::build_info::Build;
use crate::face_builder::{FaceBuilder, ShelfFaceBuilder};
use crate::link_art_builder::LinkArtBuilder;
use crate::link_screen::{LinkSprites, Sprite};
use crate::link_start::{LinkFail, LinkStep};
use crate::session::Session;

/// How long a dark panel waits before userspace standby. The dark is immediate — the lid or
/// the button kills the backlight on the edge — but the device is still running flat out
/// behind it at 400-700 mA, so this is the window in which the user might come straight
/// back, not a power saving.
///
/// Five minutes, and then the device stops rendering and polls input and timers at 5 Hz. Five
/// more minutes in that state with the lid still shut cuts the rails; resume.state was written
/// when the panel went dark, so the next boot seats the cart. Open the lid or press POWER inside
/// that standby window and you're back in the game. External power and an enumerated USB host
/// keep the dark panel in the first stage instead.
const DOZE_TIMEOUT: Duration = Duration::from_secs(300);

/// A hitch longer than this would jump the shelf spring and the insert. The emu paces itself
/// on its own thread; this is only the UI clock.
const UI_DT_MAX: f32 = 0.08;

/// Amber. The only warning colour in the tree, and the reason it is not the HUD's ink: a
/// refusal that looks like a volume glyph is a refusal nobody reads as one.
const ALERT_INK: [u8; 3] = [0xf0, 0xb4, 0x3c];

pub struct Frontend {
    session: Session,
    start: Instant,
    last: Instant,
    draws: Vec<Draw>,
    /// One texture per ring slot, reused every time the switcher opens.
    polaroid_texes: Vec<TexId>,
    /// The top plate's line of type, re-rasterised whenever the selection moves.
    title_tex: Option<TexId>,
    /// Builds the open cart's faces off the frame loop.
    faces: FaceBuilder,
    /// Builds shelf faces and captions away from the frame loop. The queue is ordered with the
    /// visible ring slots first, including the wrapped tail at the opposite edge.
    shelf_faces: ShelfFaceBuilder,
    cart_upload_queue: Vec<slot_store::Cart>,
    cart_upload_inflight: bool,
    /// Static assets are intentionally hydrated in small batches after the first present. A
    /// single monolithic upload made the shelf appear frozen while fonts, icons and wallpaper
    /// were still being decoded.
    static_upload_stage: u8,
    /// The silhouette placeholders are tiny and are uploaded before the first render, so the
    /// very first shelf frame cannot expose a rectangular loading block.
    placeholders_uploaded: bool,
    wallpaper: WallpaperBuilder,
    /// Builds the link screen's artwork off the frame loop, once, at boot.
    link_art: LinkArtBuilder,
    /// Whether the link art has been uploaded and handed to `App` already.
    link_art_done: bool,
    /// The cart last asked for.
    core_asked: Option<String>,
    /// The open cart and its lid, and which cart they were built for.
    core_board_tex: Option<TexId>,
    core_lid_tex: Option<TexId>,
    core_built: Option<String>,
    /// The undo cap's label, which changes with what is on offer.
    undo_tex: Option<TexId>,
    switcher: Switcher,
    clocks: Clocks,
    about: AboutFace,
    quick_clock: QuickClock,
    font_revision: u64,
    gb_overlay: Option<TexId>,
    gbc_overlay: Option<TexId>,
}

/// Date & Time's value in the quick menu, grey and lit, and the text they were built for.
#[derive(Default)]
struct QuickClock {
    dim: Option<TexId>,
    lit: Option<TexId>,
    shown: String,
}

/// The about label, and what it was last built for. The gauge is the only thing on it that
/// moves, so the reading is what decides whether it is rebuilt.
#[derive(Default)]
struct AboutFace {
    tex: Option<TexId>,
    /// `None` is a board with no gauge, which is a different thing from not having built one
    /// yet — `tex` says that.
    battery: Option<u8>,
    page: StickerPage,
}

/// The clock screen's two faces and the shelf's one, with what each was last built for. The
/// picker's line changes under the caret; the shelf clock changes once a minute; the battery
/// percent changes whenever the reading does.
#[derive(Default)]
struct Clocks {
    line: Option<TexId>,
    hint: Option<TexId>,
    shelf: Option<TexId>,
    picked: Option<String>,
    shown: String,
    battery: String,
    battery_tex: Option<TexId>,
    shelf_count: String,
    shelf_count_tex: Option<TexId>,
}

/// What the switcher's textures were built for. The photos and the undo cap are per opening;
/// the title is per selection.
#[derive(Default)]
struct Switcher {
    open: bool,
    titled: Option<String>,
}

/// Decodes the optional user wallpaper away from the render thread. Large PNGs on the card can
/// take long enough to make a staged static upload look frozen if decoding happens inline.
struct WallpaperBuilder {
    built: Receiver<Option<Vec<u8>>>,
}

impl WallpaperBuilder {
    fn spawn(root: Option<PathBuf>, seed: u64) -> Self {
        let (tx, built) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("slot-wallpaper".into())
            .spawn(move || {
                let face = root
                    .as_deref()
                    .and_then(|root| crate::wallpaper::pick(root, seed))
                    .and_then(|path| slot_ui::wallpaper_face(&path));
                let _ = tx.send(face);
            });
        if let Err(e) = spawned {
            eprintln!("slot: wallpaper: worker thread failed to start: {e}");
        }
        WallpaperBuilder { built }
    }

    fn take(&self) -> Option<Option<Vec<u8>>> {
        self.built.try_recv().ok()
    }
}

impl Frontend {
    pub fn boot(platform: Box<dyn Platform>) -> Self {
        let now = Instant::now();
        let mut session = Session::boot(platform.root().to_path_buf());
        session
            .app_mut()
            .set_power(Power::new(platform, DOZE_TIMEOUT));
        let cart_upload_queue = session.app().shelf_face_upload_order();
        let wallpaper = WallpaperBuilder::spawn(
            session.app().root().map(PathBuf::from),
            session.app().wall_secs().unsigned_abs(),
        );
        Frontend {
            session,
            start: now,
            last: now,
            draws: Vec::new(),
            polaroid_texes: Vec::new(),
            title_tex: None,
            faces: FaceBuilder::spawn(),
            shelf_faces: ShelfFaceBuilder::spawn(),
            cart_upload_queue,
            cart_upload_inflight: false,
            static_upload_stage: 0,
            placeholders_uploaded: false,
            wallpaper,
            link_art: LinkArtBuilder::spawn(),
            link_art_done: false,
            core_asked: None,
            core_board_tex: None,
            core_lid_tex: None,
            core_built: None,
            undo_tex: None,
            switcher: Switcher::default(),
            clocks: Clocks::default(),
            about: AboutFace::default(),
            quick_clock: QuickClock::default(),
            font_revision: 0,
            gb_overlay: None,
            gbc_overlay: None,
        }
    }

    pub fn presented(&self) {
        self.session.presented();
    }

    /// Reset the staged upload after a font/theme revision. The next frames repopulate the
    /// static plates and the shelf worker without blocking animation on one giant pass.
    pub fn upload_faces(&mut self, compositor: &mut Compositor) {
        self.static_upload_stage = 0;
        self.upload_static_faces(compositor);
        self.cart_upload_queue = self.session.app().shelf_face_upload_order();
        self.cart_upload_inflight = false;
    }

    /// Upload all static batches synchronously for a deliberate reload (font changes). The
    /// device loop uses `upload_next_static_faces` instead, one batch after each present.
    pub fn upload_static_faces(&mut self, compositor: &mut Compositor) {
        while !self.upload_next_static_faces(compositor) {}
    }

    pub fn upload_next_static_faces(&mut self, compositor: &mut Compositor) -> bool {
        match self.static_upload_stage {
            0 => {
                // Overlay artwork from Jeltr0n's Retro-Overlays:
                // https://github.com/Jeltr0n/Retro-Overlays
                self.gb_overlay =
                    upload_png(compositor, include_bytes!("../../../jeltron/GB_DMG.png"));
                self.gbc_overlay =
                    upload_png(compositor, include_bytes!("../../../jeltron/GB_Color.png"));
                self.upload_placeholders(compositor);
                let favorite = word_face("Favorites");
                let favorite = Printed::new(
                    compositor.create_texture(favorite.w, favorite.h, &favorite.rgba),
                    favorite.w,
                );
                self.session
                    .app_mut()
                    .set_shelf_captions(BTreeMap::new(), favorite);
                let empty = title_face(EMPTY_SHELF);
                self.session.app_mut().set_empty_caption(Printed::new(
                    compositor.create_texture(empty.w, empty.h, &empty.rgba),
                    empty.w,
                ));
                let empty_recents = title_face("no recently played games");
                self.session
                    .app_mut()
                    .set_empty_recents_caption(Printed::new(
                        compositor.create_texture(
                            empty_recents.w,
                            empty_recents.h,
                            &empty_recents.rgba,
                        ),
                        empty_recents.w,
                    ));
                let recents_hint = arrows_hint_face("Categories");
                let hint_tex =
                    compositor.create_texture(recents_hint.w, recents_hint.h, &recents_hint.rgba);
                self.session
                    .app_mut()
                    .set_empty_recents_hint((hint_tex, recents_hint.w));
                let idle = [hint_face("A", "Open"), hint_face("START", "Core")]
                    .into_iter()
                    .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                    .collect();
                self.session.app_mut().set_shelf_idle_faces(idle);
                self.static_upload_stage = 1;
            }
            1 => {
                let category_faces = (0..5)
                    .map(|category| {
                        let face = category_tab_face(category);
                        // Pixel chips need nearest upload; glyphs are fine either way.
                        let tex = if category >= 2 {
                            compositor.create_texture_nearest(face.w, face.h, &face.rgba)
                        } else {
                            compositor.create_texture(face.w, face.h, &face.rgba)
                        };
                        Printed::sized(tex, face.w, face.h)
                    })
                    .collect();
                self.session
                    .app_mut()
                    .set_shelf_category_faces(category_faces);
                let flip = arrows_hint_face("Flip");
                self.session.app_mut().set_about_flip_face(
                    compositor.create_texture(flip.w, flip.h, &flip.rgba),
                    flip.w,
                );
                let clock_hint = set_clock_hint_face();
                self.session.app_mut().set_about_clock_hint(
                    compositor.create_texture(clock_hint.w, clock_hint.h, &clock_hint.rgba),
                    clock_hint.w,
                );
                let mark = favorite_mark_face();
                if mark.w > 0 {
                    self.session.app_mut().set_favorite_mark(
                        compositor.create_texture(mark.w, mark.h, &mark.rgba),
                        mark.w,
                        mark.h,
                    );
                }
                // The clock screen's instruction never changes. Upload it with the other key
                // caps so reopening Date & Time only has to rasterise the line under the caret.
                let clock_hint = set_clock_hint_face();
                self.clocks.hint =
                    Some(compositor.create_texture(clock_hint.w, clock_hint.h, &clock_hint.rgba));
                self.static_upload_stage = 2;
            }
            2 => {
                let icons = Icon::ALL
                    .iter()
                    .map(|i| {
                        let f = icon_face(*i, HUD_ICON_PX, HUD_INK);
                        compositor.create_texture(f.w, f.h, &f.rgba)
                    })
                    .collect();
                self.session.app_mut().set_icon_faces(icons);
                let link_badges = LinkBadge::FACES
                    .iter()
                    .map(|b| {
                        let (badge, ink) = (
                            b.badge().expect("a face has a glyph"),
                            b.colour().expect("and a colour"),
                        );
                        let f = badge_face(badge, HUD_ICON_PX, ink);
                        compositor.create_texture(f.w, f.h, &f.rgba)
                    })
                    .collect();
                self.session.app_mut().set_link_badge_faces(link_badges);
                let alert = icon_face(Icon::Alert, ALERT_PX, ALERT_INK);
                let alert = compositor.create_texture(alert.w, alert.h, &alert.rgba);
                self.session.app_mut().set_alert_face(alert);
                self.static_upload_stage = 3;
            }
            3 => {
                let lines = PowerChoice::ALL
                    .iter()
                    .map(|c| {
                        let f = menu_face(match c {
                            PowerChoice::Restart => "Restarting",
                            PowerChoice::PowerOff => "Powering Down",
                        });
                        (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                    })
                    .collect();
                self.session.app_mut().set_shutdown_faces(lines);
                let menu = PowerChoice::ALL
                    .iter()
                    .map(|c| {
                        let f = menu_face(c.text());
                        (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
                    })
                    .collect();
                self.session.app_mut().set_power_menu_faces(menu);
                let pwr_legend = [hint_face("B", "Cancel"), hint_face("A", "Select")]
                    .into_iter()
                    .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                    .collect();
                self.session.app_mut().set_power_legend_faces(pwr_legend);
                // The quick menu's rows, every value a row can hold in both inks, its two arrows and
                // its legend. At boot, like the power menu's rows, so moving through the menu or
                // changing a value never waits on a font. Only Date & Time's value is left to
                // `sync_quick_clock`: it is the one thing on the menu that changes by itself.
                let mut up = |f: UndoFace| (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h);
                let labels = QuickRow::ALL
                    .iter()
                    .map(|r| up(quick_label_face(*r)))
                    .collect();
                let values = QuickValue::ALL
                    .iter()
                    .map(|v| [false, true].map(|lit| up(quick_value_face(v.text(), lit))))
                    .collect();
                let carets = [false, true].map(|right| up(quick_caret_face(right)));
                let legend = quick_legend_faces().map(|f| {
                    let (tex, w, _) = up(f);
                    (tex, w)
                });
                self.session.app_mut().set_quick_menu_faces(QuickMenuFaces {
                    labels,
                    values,
                    carets,
                    legend,
                });
                self.static_upload_stage = 4;
            }
            4 => {
                let sockets = slot_store::Core::PICKABLE
                    .iter()
                    .map(|c| {
                        let f = socket_face(*c);
                        compositor.create_texture(f.w, f.h, &f.rgba)
                    })
                    .collect();
                let chips = slot_store::Core::PICKABLE
                    .iter()
                    .map(|c| {
                        let f = chip_face(Some(*c));
                        compositor.create_texture(f.w, f.h, &f.rgba)
                    })
                    .collect();
                let blank = chip_face(None);
                let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
                let shadow = chip_shadow_face();
                let shadow = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
                self.session
                    .app_mut()
                    .set_core_part_faces(sockets, chips, blank, shadow);
                let legend = [
                    hint_face("B", "Cancel"),
                    arrows_hint_face("Swap"),
                    hint_face("A", "Choose"),
                ]
                .into_iter()
                .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                .collect();
                self.session.app_mut().set_core_legend_faces(legend);
                self.static_upload_stage = 5;
            }
            5 => {
                let roles = menu_faces(compositor, LinkRow::ALL.iter().map(|r| r.text()));
                self.session.app_mut().set_link_menu_faces(roles);
                if let Some(linked) = menu_faces(compositor, ["Linked"].into_iter()).pop() {
                    self.session.app_mut().set_link_linked_face(linked);
                }
                let legend = [
                    hint_face("B", "Cancel"),
                    arrows_hint_face("Swap"),
                    hint_face("A", "Link"),
                    hint_face("A", "OK"),
                ]
                .into_iter()
                .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                .collect();
                self.session.app_mut().set_link_legend_faces(legend);
                let steps = menu_faces(compositor, LinkStep::ALL.iter().map(|s| s.line()));
                self.session.app_mut().set_link_step_faces(steps);
                let fails = menu_faces(compositor, LinkFail::SHOWN.iter().map(|f| f.line()));
                self.session.app_mut().set_link_fail_faces(fails);
                let toasts = Toast::ALL
                    .iter()
                    .map(|t| {
                        let f = toast_face(*t);
                        compositor.create_texture(f.w, f.h, &f.rgba)
                    })
                    .collect();
                self.session.app_mut().set_toast_faces(toasts);
                self.static_upload_stage = 6;
            }
            6 => {
                self.font_revision = self.session.app().font_revision();
                let legend = legend_faces(compositor, &LEGEND);
                self.session.app_mut().set_legend_faces(legend);
                let bolt = icon_face(Icon::Charging, BOLT_PX, HUD_INK);
                let bolt_id = compositor.create_texture(bolt.w, bolt.h, &bolt.rgba);
                self.session.app_mut().set_bolt_face(bolt_id);
                self.static_upload_stage = 7;
            }
            _ => return true,
        }
        false
    }

    /// Poll the worker and upload at most one finished cart per frame. Requests are sent only
    /// one at a time; the queue is therefore both cancellable and ordered by the visible ring.
    pub fn upload_next_cart_face(&mut self, compositor: &mut Compositor) -> bool {
        // A resumed cart has no shelf on screen. Leave its CPU and SD bandwidth to the core
        // until the user actually returns home; the pending queue is retained for that moment.
        if !matches!(self.session.app().phase(), Phase::Shelf) {
            return self.cart_upload_queue.is_empty() && !self.cart_upload_inflight;
        }
        if let Some(built) = self.shelf_faces.take() {
            self.upload_shelf_face(compositor, built);
            self.cart_upload_inflight = false;
        }
        if !self.cart_upload_inflight && !self.cart_upload_queue.is_empty() {
            let best_idx = self
                .cart_upload_queue
                .iter()
                .enumerate()
                .min_by_key(|(_, cart)| self.session.app().cart_upload_priority_for(cart))
                .map(|(i, _)| i)
                .unwrap_or(0);
            let cart = self.cart_upload_queue.swap_remove(best_idx);
            self.cart_upload_inflight = self.shelf_faces.request(cart);
        }
        !self.cart_upload_inflight && self.cart_upload_queue.is_empty()
    }

    fn upload_shelf_face(
        &mut self,
        compositor: &mut Compositor,
        built: crate::face_builder::BuiltShelfFace,
    ) {
        let tex = compositor.create_texture(built.face.w, built.face.h, &built.face.rgba);
        self.session.app_mut().set_face_for(
            built.platform,
            &built.stem,
            tex,
            (built.face.w, built.face.h),
            built.complete_artwork,
        );
        let caption = (
            Printed::new(
                compositor.create_texture(built.group.w, built.group.h, &built.group.rgba),
                built.group.w,
            ),
            Printed::new(
                compositor.create_texture(built.title.w, built.title.h, &built.title.rgba),
                built.title.w,
            ),
        );
        self.session
            .app_mut()
            .set_shelf_caption(built.stem, caption);
    }

    fn upload_placeholders(&mut self, compositor: &mut Compositor) {
        if self.placeholders_uploaded {
            return;
        }
        let shadow = cart_shadow_for(slot_store::Platform::Gb);
        self.session
            .app_mut()
            .set_gb_shadow(compositor.create_texture(shadow.w, shadow.h, &shadow.rgba));
        let placeholder = cart_placeholder_for(slot_store::Platform::Gb);
        self.session
            .app_mut()
            .set_gb_cart_placeholder(compositor.create_texture(
                placeholder.w,
                placeholder.h,
                &placeholder.rgba,
            ));
        let placeholder = cart_placeholder_for(slot_store::Platform::Gba);
        self.session
            .app_mut()
            .set_cart_placeholder(compositor.create_texture(
                placeholder.w,
                placeholder.h,
                &placeholder.rgba,
            ));
        let shadow = cart_shadow();
        let id = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session.app_mut().set_cart_shadow(id);
        self.placeholders_uploaded = true;
    }

    fn sync_wallpaper(&mut self, compositor: &mut Compositor) {
        let Some(result) = self.wallpaper.take() else {
            return;
        };
        let Some(rgba) = result else {
            return;
        };
        let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
        self.session.app_mut().set_wallpaper(id);
    }

    /// One frame into the offscreen target and out to a surface of `window` pixels. The
    /// caller swaps: only it knows what presenting costs.
    pub fn render(&mut self, compositor: &mut Compositor, window: (u32, u32)) {
        self.compose(compositor);
        compositor.end_frame(window);
    }

    /// One frame into the offscreen target and no further: what `render` presents, and what a
    /// test with no window to present to reads back with `Compositor::read_frame`.
    pub fn compose(&mut self, compositor: &mut Compositor) {
        self.upload_placeholders(compositor);
        self.sync_wallpaper(compositor);
        if self.font_revision != self.session.app().font_revision() {
            self.title_tex = None;
            self.undo_tex = None;
            self.clocks = Clocks::default();
            self.about = AboutFace::default();
            self.quick_clock = QuickClock::default();
            self.upload_faces(compositor);
        }
        // Set every frame rather than on the edge: the grade is part of the final blit, so
        // it has to be right whether or not anything just changed it.
        compositor.set_blue_light(self.session.app().blue_light());
        compositor.set_lcd(self.session.app().lcd_enabled());
        compositor.set_shake(self.session.app().screen_shake());
        compositor.set_screen_power(self.session.app().screen_power());
        compositor.set_game_source_rect(self.session.app().source_rect());
        compositor.begin_frame();
        if let Some(frame) = self.session.frame() {
            compositor.upload_game(&frame);
        }
        sync_clock(self.session.app_mut(), compositor, &mut self.clocks);
        sync_about(self.session.app_mut(), compositor, &mut self.about);
        sync_quick_clock(self.session.app_mut(), compositor, &mut self.quick_clock);
        sync_core_picker(
            self.session.app_mut(),
            compositor,
            &self.faces,
            &mut self.core_asked,
            &mut self.core_board_tex,
            &mut self.core_lid_tex,
            &mut self.core_built,
        );
        if !self.link_art_done {
            if let Some(art) = self.link_art.take() {
                let mut up = |f: &slot_ui::CartFace| Sprite {
                    tex: compositor.create_texture(f.w, f.h, &f.rgba),
                    w: f.w,
                    h: f.h,
                };
                let sprites = LinkSprites {
                    port: up(&art.port),
                    plug_host: up(&art.plug_host),
                    plug_join: up(&art.plug_join),
                    adapter: up(&art.adapter),
                    arcs_right: [
                        up(&art.arcs_right[0]),
                        up(&art.arcs_right[1]),
                        up(&art.arcs_right[2]),
                    ],
                    arcs_left: [
                        up(&art.arcs_left[0]),
                        up(&art.arcs_left[1]),
                        up(&art.arcs_left[2]),
                    ],
                    clicks: up(&art.clicks),
                    arrow_left: up(&art.arrow_left),
                    arrow_right: up(&art.arrow_right),
                };
                self.session.app_mut().set_link_sprites(sprites);
                self.link_art_done = true;
            }
        }
        sync_switcher(
            self.session.app_mut(),
            compositor,
            Faces {
                pool: &mut self.polaroid_texes,
                title: &mut self.title_tex,
                undo: &mut self.undo_tex,
            },
            &mut self.switcher,
        );
        self.draws.clear();
        self.session.app().draw(&mut self.draws);
        let overlay = if self.session.app().gb_overlay_visible() {
            match self.session.app().game_platform() {
                Some(slot_store::Platform::Gb) => self.gb_overlay,
                Some(slot_store::Platform::Gbc) => self.gbc_overlay,
                _ => None,
            }
        } else {
            None
        };
        if let Some(tex) = overlay {
            let mut i = 0;
            while i < self.draws.len() {
                if self.draws[i] == Draw::Game {
                    self.draws.insert(
                        i + 1,
                        Draw::Tex {
                            x: 0.0,
                            y: 0.0,
                            w: OUT_W as f32,
                            h: OUT_H as f32,
                            tex,
                            alpha: 1.0,
                        },
                    );
                    i += 1;
                }
                i += 1;
            }
        }
        compositor.draw_list(&self.draws);
    }

    /// Input and time, after the frame is on screen. The gesture windows expire on this
    /// whether or not anything was pressed, so it is called every frame.
    pub fn advance(&mut self, input: &mut dyn InputSource) {
        let now = self.now();
        // Standby can last for minutes while the animation clock is intentionally throttled.
        // Move the app clock to wall time before input and the single normal update pass; calling
        // `tick_ms` here would run the timer scheduler twice on every rendered frame.
        if self.session.app().standby() {
            self.session.app_mut().set_clock_ms(now);
        }
        let events = input.poll(now);
        self.session.feed(events, now);
        // A hitch would otherwise jump the shelf spring and the insert. The emu paces itself
        // on its own thread; this is only the UI clock.
        let dt = self.last.elapsed().as_secs_f32().min(UI_DT_MAX);
        self.last = Instant::now();
        self.session.update(dt);
    }

    fn now(&self) -> Millis {
        self.start.elapsed().as_millis() as Millis
    }

    pub fn powering_off(&self) -> bool {
        self.session.app().ready_to_power_off()
    }

    pub fn standby(&self) -> bool {
        self.session.app().standby()
    }

    pub fn restarting(&self) -> bool {
        self.session.app().ready_to_restart()
    }

    pub fn restart(&mut self) {
        self.session.silence();
        self.session.app_mut().restart();
    }

    /// The state was flushed on the edge that set `powering_off`, so there is nothing left to
    /// do but go. The PCM is dropped first: `poweroff` does not return, and an open H700
    /// codec is a hiss behind a dark panel if init then sits on the rails.
    pub fn poweroff(&mut self) {
        self.session.silence();
        self.session.app_mut().poweroff();
    }
}

fn upload_png(compositor: &mut Compositor, bytes: &[u8]) -> Option<TexId> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return None,
    };
    Some(compositor.create_texture(info.width, info.height, &rgba))
}

/// A line of menu type per label, in the order they were handed over, each with the size it
/// was rastered at. Every menu on the device is drawn from a list shaped exactly like this,
/// so the four the in-game menu needs are built through one function rather than four copies
/// of the same three lines.
fn menu_faces<'a>(
    compositor: &mut Compositor,
    labels: impl Iterator<Item = &'a str>,
) -> Vec<(TexId, u32, u32)> {
    labels
        .map(|label| {
            let f = menu_face(label);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        })
        .collect()
}

/// A screen's key caps, in the order the legend names them. None of them ever changes what
/// it says, so they are uploaded once and outlive every visit to that screen.
fn legend_faces(compositor: &mut Compositor, legend: &[(&str, &str)]) -> Vec<TexId> {
    legend
        .iter()
        .map(|(key, label)| {
            let f = hint_face(key, label);
            compositor.create_texture(f.w, f.h, &f.rgba)
        })
        .collect()
}

/// The switcher's textures, which outlive any one opening.
struct Faces<'a> {
    pool: &'a mut Vec<TexId>,
    title: &'a mut Option<TexId>,
    undo: &'a mut Option<TexId>,
}

/// Photos and the undo cap are built once per opening, on the way in, while the game is
/// already paused. Rebuilt each time rather than cached because the ring changes underneath
/// them. The title names the selection, so it follows a flick instead.
fn sync_switcher(app: &mut App, compositor: &mut Compositor, texes: Faces, state: &mut Switcher) {
    if !matches!(app.phase(), Phase::Polaroids { .. }) {
        state.open = false;
        return;
    }
    if !state.open {
        state.open = true;
        state.titled = None;
        let faces: Vec<_> = app.polaroid_entries().iter().map(photo_face).collect();
        let ids = faces
            .iter()
            .enumerate()
            .map(|(i, f)| match texes.pool.get(i) {
                Some(id) => {
                    compositor.update_texture(*id, f.w, f.h, &f.rgba);
                    *id
                }
                None => {
                    let id = compositor.create_texture_nearest(f.w, f.h, &f.rgba);
                    texes.pool.push(id);
                    id
                }
            })
            .collect();
        app.set_polaroid_faces(ids);

        // An offer can expire while the switcher is up but it cannot change into the other
        // kind, so the cap only has to be rasterised on the way in. Whether it is drawn at
        // all is the app's call.
        let label = app
            .undo_label()
            .map(|l| upload(compositor, texes.undo, hint_face("X", l)));
        app.set_undo_face(label);
    }
    if state.titled.as_deref() != app.polaroid_stamp() {
        state.titled = app.polaroid_stamp().map(str::to_string);
        let face = title_face(&app.polaroid_title(&format_stamp(app.wall_secs())));
        let id = upload(compositor, texes.title, face);
        app.set_polaroid_title_face(id);
    }
}

/// The picker is rasterised on every change under the caret, which is once per press. The
/// shelf clock follows the wall clock, so it is rebuilt when the minute turns and not on the
/// fifty nine seconds either side of it.
fn sync_clock(app: &mut App, compositor: &mut Compositor, clocks: &mut Clocks) {
    let picked = app.picker().map(|p| p.text());
    // The static clock hint is staged after the first frame. If the picker was already up on
    // that frame, `picked` has been remembered even though there was no hint to pair with its
    // line yet; retry until the line has actually been uploaded.
    if picked != clocks.picked || (app.picker().is_some() && clocks.line.is_none()) {
        clocks.picked = picked;
        if let (Some(face), Some(hint)) = (app.picker().map(|p| p.face()), clocks.hint) {
            let line = upload(compositor, &mut clocks.line, face);
            app.set_clock_faces(line, hint);
        }
    }
    let shown = hhmm(app.wall_secs());
    if shown != clocks.shown {
        let face = word_face(&shown);
        clocks.shown = shown;
        let w = face.w;
        let id = upload(compositor, &mut clocks.shelf, face);
        app.set_shelf_clock_face(id, w);
    }
    let battery_shown = app
        .battery()
        .map(|b| format!("{}%", b.percent))
        .unwrap_or_default();
    if battery_shown != clocks.battery {
        clocks.battery = battery_shown.clone();
        if !battery_shown.is_empty() {
            let face = word_face(&battery_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.battery_tex, face);
            app.set_battery_percent_face(id, w);
        }
    }
    let count_shown = if matches!(app.phase(), Phase::Shelf) && app.shelf_total() > 1 {
        format!("{}/{}", app.shelf_index() + 1, app.shelf_total())
    } else {
        String::new()
    };
    if count_shown != clocks.shelf_count {
        clocks.shelf_count = count_shown.clone();
        if !count_shown.is_empty() {
            let face = word_face(&count_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.shelf_count_tex, face);
            app.set_shelf_count_face(Some((id, w)));
        } else {
            app.set_shelf_count_face(None);
        }
    }
}

/// Date & Time's value, in both inks so the bar can land on it without anything being rastered.
/// Built only while the quick menu is up, and then only when the minute has turned since it was
/// last built, as the shelf clock is: a clock nobody is looking at is not worth a rasterisation a
/// minute on the H700.
fn sync_quick_clock(app: &mut App, compositor: &mut Compositor, state: &mut QuickClock) {
    if app.quick_menu().is_none() {
        return;
    }
    let text = date_time_text(app.wall_secs());
    if text == state.shown {
        return;
    }
    let (dim, lit) = (
        quick_value_face(&text, false),
        quick_value_face(&text, true),
    );
    let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
    let dim = upload(compositor, &mut state.dim, dim);
    let lit = upload(compositor, &mut state.lit, lit);
    app.set_quick_clock_faces((dim, dim_size.0, dim_size.1), (lit, lit_size.0, lit_size.1));
    state.shown = text;
}

/// Built only once the screen is up: it is a 660 by 228 rasterisation and most sessions never
/// open it.
fn sync_about(app: &mut App, compositor: &mut Compositor, state: &mut AboutFace) {
    if !matches!(app.phase(), Phase::About) {
        return;
    }
    let battery = app.battery().map(|b| b.percent);
    let page = app.about_page();
    if state.tex.is_some() && state.battery == battery && state.page == page {
        return;
    }
    state.battery = battery;
    state.page = page;
    let build = Build::current();
    let face = sticker_face(&StickerFields {
        battery,
        serial: &build.serial(),
        dirty_digit: build.dirty_digit(),
        page,
    });
    let id = upload(compositor, &mut state.tex, face);
    app.set_sticker_face(id);
}

/// The open cart's faces, asked for as soon as the caret lands on a cart and uploaded when the
/// worker hands them back, so they are normally on the GPU before START. The worker is the only
/// place they are built: rasterised on the frame loop, a board freezes the shelf for the better
/// part of half a second on the H700.
fn sync_core_picker(
    app: &mut App,
    compositor: &mut Compositor,
    builder: &FaceBuilder,
    asked: &mut Option<String>,
    board: &mut Option<TexId>,
    lid: &mut Option<TexId>,
    built: &mut Option<String>,
) {
    let highlighted = app.selected_stem().map(str::to_string);
    if highlighted.is_some() && *asked != highlighted {
        if let Some(cart) = app
            .carts()
            .iter()
            .find(|c| highlighted.as_deref() == Some(c.stem.as_str()))
        {
            builder.request(cart.clone());
        }
        *asked = highlighted.clone();
    }
    let Some(faces) = builder.take() else {
        return;
    };
    // A build for a cart the caret has since left is dropped; the one it is on is on its way.
    if highlighted.as_deref() != Some(faces.stem.as_str()) || *built == highlighted {
        return;
    }
    let board_id = upload_rgba(
        compositor,
        board,
        faces.board.w,
        faces.board.h,
        &faces.board.rgba,
    );
    let lid_id = upload_rgba(compositor, lid, faces.lid.w, faces.lid.h, &faces.lid.rgba);
    if (faces.lid.w, faces.lid.h) == (CART_W + 2 * TURN_PAD, CART_H + 2 * TURN_PAD) {
        app.set_core_board_faces(board_id, lid_id);
    } else {
        let artwork_size = (
            faces.lid.w.saturating_sub(2 * TURN_PAD),
            faces.lid.h.saturating_sub(2 * TURN_PAD),
        );
        app.set_core_board_faces_with_size(board_id, lid_id, artwork_size);
    }
    *built = Some(faces.stem);
}

fn upload(compositor: &mut Compositor, slot: &mut Option<TexId>, face: slot_ui::UndoFace) -> TexId {
    upload_rgba(compositor, slot, face.w, face.h, &face.rgba)
}

/// Into the slot's own texture if it has one, so the pool stops growing after the first time.
fn upload_rgba(
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> TexId {
    match *slot {
        Some(id) => {
            compositor.update_texture(id, w, h, rgba);
            id
        }
        None => {
            let id = compositor.create_texture(w, h, rgba);
            *slot = Some(id);
            id
        }
    }
}
