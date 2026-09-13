use crate::plate::UndoFace;
use crate::text;

/// What a held POWER offers. Restart, because a device you develop on wants one that is not
/// "off, then find the button again" — and off, which is the only other thing this hardware
/// can honestly do.
///
/// There is no Standby in this menu. The lid's five-minute dark wait becomes H700 Super
/// Standby, and five more minutes of that with the lid still shut cuts the rails. Open the
/// lid or press POWER inside that window and you're back in the game. Holding POWER is
/// therefore off, not another path into the same sleep.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PowerChoice {
    Restart,
    PowerOff,
}

impl PowerChoice {
    /// Restart first: it is the one that costs nothing to pick by mistake.
    pub const ALL: [PowerChoice; 2] = [PowerChoice::Restart, PowerChoice::PowerOff];

    /// Position in `ALL`, which is the order the faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            PowerChoice::Restart => "Restart",
            PowerChoice::PowerOff => "Power Off",
        }
    }
}

/// The menu is read at arm's length on a 720x480 panel while the user is deciding something
/// they cannot undo, so it is set well above the key-caption type the rest of the chrome
/// uses. The shutdown line that follows a choice is rastered at the same size: the words
/// change but the voice should not.
const MENU_PX: f32 = 30.0;
const MENU_MIN_PX: f32 = 18.0;
const MENU_H: u32 = 40;
/// Breathing room either side of the ink, which is also what the highlight bar is padded by
/// so the bar hugs the words rather than the panel.
pub const MENU_PAD: u32 = 18;
const MENU_INK: [u8; 3] = [0xf6, 0xf4, 0xef];

/// Sized to its own text rather than to a fixed box, so a caller can put a bar behind it
/// that fits the words. A fixed width would make the bar the same size under "Restart" and
/// "Power Off", which is the thing that looks wrong when the selection moves.
pub fn menu_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let ink = text::line_width(font, label, MENU_PX, 0.0).ceil() as u32;
    let w = ink + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    let layout = text::fit(font, label, w as f32, 1, MENU_PX, MENU_MIN_PX);
    text::draw_centred(&mut rgba, w, MENU_H, &layout, MENU_INK);
    UndoFace { rgba, w, h: MENU_H }
}
