# Changelog

## Unreleased — `slots` fork

This fork starts from [`slot`](https://github.com/BrandonKowalski/slot) by Brandon T. Kowalski.
The original GBA-focused frontend remains the foundation; this fork adds:

- Game Boy and Game Boy Color support through Gambatte.
- Platform filters for `GBA`, `GB` and `GBC`, with platform-specific cartridge shells.
- Optional complete cartridge artwork in `Cartridges/` and labels for all three platforms.
- BaseOS v1.1.0 installation support for both one-card and two-card setups.
- A quick menu for fast-forward speed and sound, rumble, Date & Time and About.

### Display

- Platform defaults and per-game LCD, colour correction, and overlay prefs, with reset for the
  current scope.
- In-game display quick menu over a translucent game scrim; shelf LCD row and clamp on category
  tabs.
- Wallpaper draws at full strength (no dark scrim).

### Shelf

- Pixel console tab icons, display toasts, and X/Y face-button mapping.
- GBA migrate repair when porting prefs across layouts.

### Device / build

- H700 toolchain helper for cross builds (`scripts/with-h700-toolchain.sh`).

### Stability

- Missing-core insert refuses once (no per-frame dylib reopen).
- Eject flush times out if the emulator worker stalls, so the cart can still leave the slot.
- One-shot toast when the audio device fails to open.
- MENU double-tap still opens the state switcher after the first tap opens play settings.

### Device smoke checklist

- Lid close / standby / resume and power-off resume.
- Display prefs: platform defaults, per-game override, reset current scope.
- In-game display menu and scrim; shelf tabs ALL / REC / GBA / GB / GBC; wallpaper.
- Eject and save trust; link + doze if you use link.

## 1.0.0

A focused GBA, Game Boy and Game Boy Color frontend for the Anbernic RG SP.

### Play

- Game Boy and Game Boy Color carts, through Gambatte. GB uses PixelShift Pack 1's
  BGB 0.3 palette; GBC uses Gambatte's accurate colour correction. Optional
  `gb_bios.bin` / `gbc_bios.bin` play the Nintendo logo the same way `gba_bios.bin`
  already does for mGBA.
- Shelf categories `ALL`, `REC`, `GBA`, `GB`, `GBC` on L2 / R2, letter jump on L1 / R1,
  favorites on Y, recently played.
- GBA cores still pickable with START (mGBA or gpSP). GB and GBC always use Gambatte.
- Cart silhouettes match the real plastic: GBA landscape, GB with the lock notch, GBC
  without it. Labels render at 176x90 on GBA and 192x169 (42:37) on GB/GBC.
- SELECT toggles Pixelify against the original label font.
- The compositor's GBA LCD mask stays off while a GB or GBC cart is seated; Gambatte
  already supplies that image.

### Device

- Lid close writes a save state and darkens the panel. Open within five minutes and
  the game is still there. After that the H700 enters a lower-frequency userspace
  standby. Open the lid or press POWER within five more minutes and you're back in
  the game; after that, slot powers off and the next boot resumes from the same save
  state. External power and an enumerated USB debugging session keep the device out
  of standby.
- Power-off hands shutdown to BaseOS/BusyBox after syncing, leaving service teardown,
  filesystem unmounts and the final PMIC cut to the operating system.
- Installs on AGS-102 (two-card) and BaseOS v1.1.0 (one-card or two-card).

### Release

- The zip ships three cores: mGBA (MPL-2.0, patched), gpSP and Gambatte (GPL-2.0, with
  corresponding source). Crate and about-sticker version is 1.0.0.
