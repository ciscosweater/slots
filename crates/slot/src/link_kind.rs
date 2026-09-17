//! Which link hardware a cart uses, so the link screen draws the thing the game expects: the
//! cable or the Wireless Adapter. Mirrors the rule gpSP's `gpsp_serial=auto` applies, because
//! that is the link the core actually runs.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Cable,
    Wireless,
}

/// gpSP's `FLAGS_RFU` entries, from `gba_over.h` in `vendor/gpsp-src.tar.gz`.
const WIRELESS: [&str; 43] = [
    "B2WE", "B3AE", "B4UE", "B4UP", "B85A", "B85P", "BDGE", "BDGP", "BG3E", "BKRJ", "BMGD", "BMGE",
    "BMGF", "BMGI", "BMGJ", "BMGP", "BMGS", "BMGU", "BPED", "BPEE", "BPEF", "BPEI", "BPEJ", "BPES",
    "BPGD", "BPGE", "BPGF", "BPGI", "BPGJ", "BPGS", "BPRD", "BPRE", "BPRF", "BPRI", "BPRJ", "BPRS",
    "BR5E", "BR6E", "BRBE", "BRKE", "BTME", "BTMJ", "BTMP",
];

/// `code` and `title` are the header's, as `Cart` has them; `clean` is
/// `slot_store::header_clean` for the same ROM.
pub fn link_kind(code: &str, title: &str, clean: bool) -> LinkKind {
    if pokemon(code, title) {
        // gpSP treats a Pokémon ROM as a hack, and links it by cable, unless its header is
        // standard, it is 16 MB or smaller, its code is one gpSP knows and its title is exactly
        // the retail one. Of the retail games only FireRed, LeafGreen and Emerald get the adapter.
        let retail = clean
            && WIRELESS.contains(&code)
            && ["POKEMON FIRE", "POKEMON LEAF", "POKEMON EMER"].contains(&title);
        return if retail {
            LinkKind::Wireless
        } else {
            LinkKind::Cable
        };
    }
    if WIRELESS.contains(&code) {
        LinkKind::Wireless
    } else {
        LinkKind::Cable
    }
}

/// Whether gpSP can actually carry this cart's link.
///
/// gpSP does not emulate the link cable. It speaks the Wireless Adapter and three named cable
/// protocols — Pokémon Gen3, Advance Wars 1 and Advance Wars 2 — and a cart it recognises none
/// of is left on `SERIAL_MODE_AUTO`, which `netpacket_receive` has no case for. The session still
/// comes up, which is what makes this worth asking before the radio does: two devices join, and
/// then every packet is dropped in silence.
///
/// True for the three sets gpSP has a protocol for: the adapter list, the Pokémon family, and
/// Advance Wars 1 and 2. `code` and `title` are the header's, as `Cart` has them.
pub fn link_carried(code: &str, title: &str) -> bool {
    WIRELESS.contains(&code)
        || pokemon(code, title)
        || code.starts_with("AWR")
        || code.starts_with("AW2")
}

/// The Pokémon family, by title or by any of its codes. gpSP's own test, and the one both its
/// automatic pick and its cable protocol hang off.
fn pokemon(code: &str, title: &str) -> bool {
    title.starts_with("POKEMON")
        || ["AXV", "AXP", "BPE", "BPR", "BPG"]
            .iter()
            .any(|p| code.starts_with(p))
}
