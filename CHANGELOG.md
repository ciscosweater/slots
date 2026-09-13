# Changelog

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
  without it. Labels cover 196x86 on GBA and 168x148 (42:37) on GB/GBC.
- SELECT toggles Pixelify against the original label font.
- The compositor's GBA LCD mask stays off while a GB or GBC cart is seated; Gambatte
  already supplies that image.

### Device

- Lid close writes a save state and darkens the panel. Open within five minutes and
  the game is still there. After that the H700 enters Super Standby. Open the lid or
  press POWER within five more minutes and you're back in the game; after that, slot
  powers off and the next boot resumes from the same save state. If suspend is missing
  or fails, slot powers off when the dark-panel grace ends.
- Power-off talks to the AXP2202 with `I2C_SLAVE_FORCE` and a two-second timeout, so a
  hung I2C write cannot pin the shutdown screen until a ten-second hardware hold.
- Installs on AGS-102 (two-card) and BaseOS v1.1.0 (one-card or two-card).

### Release

- The zip ships three cores: mGBA (MPL-2.0, patched), gpSP and Gambatte (GPL-2.0, with
  corresponding source). Crate and about-sticker version is 1.0.0.
