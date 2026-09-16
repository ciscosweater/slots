# slots

`slots` is a community-maintained fork of [`slot`](https://github.com/BrandonKowalski/slot),
the original focused frontend for the Anbernic RG SP. The original project supports only Game
Boy Advance; this fork keeps that foundation and adds Game Boy and Game Boy Color support.

This is not an official upstream release. The original author and project are credited below,
and the original `slot` name is intentionally kept for the executable and device paths so an
existing installation can be updated without changing its launcher contract.

> [!IMPORTANT]
> This fork has only been tested on the Anbernic RG SP. I do not know how it behaves on other
> devices or on other operating-system versions. Please treat other hardware as untested.

## Features

- GBA, GB and GBC ROMs in one carousel.
- mGBA as the default GBA core, with optional per-game gpSP selection.
- Gambatte for GB and GBC, with a PixelShift BGB palette for GB and accurate GBC colour correction.
- Platform filters: `ALL`, `REC`, `GBA`, `GB` and `GBC`.
- Favorites, recent games, save-state rings, rewind, fast-forward and lid-aware standby.
- Quick menu entries for Fast Forward, Fast Forward Sound, Rumble, Date & Time and About.
- Optional custom cartridge artwork, labels, wallpapers, BIOS files and colour themes.
- GBA link support through gpSP between two RG SP devices.

## Download

Download the latest device bundle from this repository's [GitHub Releases](../../releases/latest).
The release contains the frontend, all three libretro cores, the BaseOS launcher and the
required third-party license/source material. It does **not** contain ROMs or BIOS files.

## Installing with BaseOS

The release tree currently targets **BaseOS** on the RG SP. You will need:

- an RG SP;
- one or two microSD cards and a card reader;
- the matching [BaseOS image](https://github.com/pvaibhav/BaseOS/releases/latest);
- the latest release ZIP from this repository; and
- ROMs and optional BIOS files that you legally obtained or dumped yourself.

Flashing an image normally erases the target card. Back up anything important before starting.

### One-card setup: BaseOS on TF1

1. Flash the BaseOS image to the card used in **TF1**.
2. Boot the RG SP once with that card so BaseOS can expand its data partition, then shut down.
3. Put the card in your computer and open the `BASEOS` data volume.
4. Extract the release ZIP and copy the **contents** of its release directory to the root of the `BASEOS` volume.
5. Copy your ROMs and optional content into the folders described below.
6. Put the card back in TF1 and boot with TF2 empty.

In this setup BaseOS exposes the TF1 data volume to the frontend as `/mnt/sdcard`.

### Two-card setup: BaseOS on TF1, frontend on TF2

1. Prepare and boot the BaseOS card in TF1 once, exactly as in the one-card setup, so its data partition is expanded.
2. Put the second card in your computer and copy the contents of the release directory to its root.
3. Add your ROMs, saves, labels, cartridge artwork, wallpapers and optional BIOS files to this second card.
4. Put the BaseOS card in TF1 and the content/frontend card in TF2, then boot.

When TF2 is present, BaseOS uses it as the frontend volume. The same release tree therefore works
on TF2 without changing the `System/slot` path.

### AGS-102 compatibility

The release tree also retains the upstream two-card AGS-102 layout: boot AGS-102 from TF1 and
put the release/content tree on TF2. AGS-102 launches `System/slot` directly. BaseOS users should
follow one of the two procedures above instead.

## SD-card layout

The frontend content root should look like this:

```
BIOS/         Optional gba_bios.bin, gb_bios.bin and gbc_bios.bin.
Games/        .gba, .gb and .gbc ROMs.
Cartridges/   Optional complete cartridge artwork: <ROM stem>.png.
Labels/       Optional label artwork: <ROM stem>.png.
Saves/        Battery saves, normally .sav and .srm files.
States/       Save-state rings under <core>/<ROM stem>/.
System/       slot, the three cores, configuration and third-party licenses.
Wallpapers/   Optional .png images, one selected randomly at boot.
```

### ROMs and filenames

Only `.gba`, `.gb` and `.gbc` files are scanned. Extensions are case-insensitive; unrelated files
are ignored. Artwork, saves and states are matched by the filename stem, not by a content hash:

```
Games/Pokemon Emerald.gba
Labels/Pokemon Emerald.png
Cartridges/Pokemon Emerald.png
```

Keep ROM stems unique across the library. Two different ROMs with the same stem can share or
overwrite their artwork, saves or states.

### Labels

Labels are PNG files in `Labels/`, without the cartridge shell around them. They are rendered at
these target sizes:

- **GBA:** `176×90` pixels, approximately the real label's `43:22` ratio.
- **GB and GBC:** `192×169` pixels, matching the real label's `42:37` ratio.

Images with another size are scaled to cover the target box and centre-cropped. A square or
portrait image can therefore lose its top and bottom; small images are enlarged. The included
Skyscraper definitions produce correctly sized artwork for both label formats:

- [`artwork-gba-labels.xml`](artwork-gba-labels.xml) outputs `176×90` GBA labels from a screenshot
  and wheel logo.
- [`artwork-gb-labels.xml`](artwork-gb-labels.xml) outputs `192×169` GB/GBC labels.

Point Skyscraper at the matching definition for the platform you are scraping.

If a label is missing or unreadable, the frontend generates a text label from the ROM filename.

### Complete cartridge artwork

Complete artwork goes in `Cartridges/` and takes priority over the generated shell and label. It
is resized to the cartridge's `240`-pixel display width while preserving its aspect ratio. Its
height follows the source image: no padding or letterboxing is added, and PNG transparency is
preserved. An invalid image falls back to the normal shell/label path.

### BIOS files

BIOS files are optional and are not included in this repository or its releases:

| System | Filename |
|---|---|
| GBA | `BIOS/gba_bios.bin` |
| GB | `BIOS/gb_bios.bin` |
| GBC | `BIOS/gbc_bios.bin` |

When present, the corresponding core can show the system's original boot logo. When absent, the
core's own high-level BIOS behaviour is used. Only use BIOS files you are permitted to use.

### Themes and core selection

`System/theme.txt` is optional. It controls the shell colours:

```
housing #24242a
recess  #1a1a1e
opening #050508
edge    #4d4d57
```

`System/selected_core.ini` is also optional. It selects the GBA core per ROM, one entry per line:

```
Pokemon Emerald = gpsp
```

GBA games default to **mGBA**. Use **gpSP** when you need its link/serial support; press `START`
on the carousel to choose a core interactively. Save states are kept separately under
`States/mgba/` and `States/gpsp/`, so states made by one core are not accidentally loaded by the
other. Battery saves are shared by the ROM.

GB and GBC games always use **Gambatte**. GB uses PixelShift Pack 1's `PixelShift 03 - BGB 0.3
Emulator` palette, while GBC uses Gambatte's GBC colour correction.

## Controls

`/` means either button. `+` means both buttons together.

### Anywhere

| Input | Action |
|---|---|
| `SELECT` + `Up` / `Down` | Adjust brightness |
| `SELECT` + `Left` / `Right` | Adjust blue light |
| `VOL+` / `VOL-` | Change the volume |
| `VOL+` + `VOL-` | Mute while remembering the previous level |
| Hold `POWER` | Save and power off |

### On the carousel

| Input | Action |
|---|---|
| `L` / `R` | Browse the carousel |
| `L1` / `R1` | Jump to the previous/next letter |
| `L2` / `R2` | Previous/next category: `ALL`, `REC`, `GBA`, `GB`, `GBC` |
| `SELECT` | Toggle Pixelify and the original label font |
| `Y` | Add or remove the game from favorites |
| `X` | Toggle the LCD effect |
| Tap `A` | Resume the last save state |
| Hold `A` | Start the game fresh |
| `MENU` | Open the quick menu |
| Quick menu | `Up` / `Down` select; `Left` / `Right` change values; `B` / `MENU` close; `A` opens `Date & Time` or `About` |
| `START` | Choose which GBA core runs the cart |

### In game

| Input | Action |
|---|---|
| Hold `MENU` | Save state, eject the cart and return to the carousel |
| Double-tap `MENU` | Open the save-state switcher; load, delete or undo a recent action |
| `SELECT` + `MENU` | Link with another RG SP; gpSP carts only |
| `SELECT` + `R1` | Save state |
| `SELECT` + `L1` | Load the most recent save state |
| Hold `L2` | Rewind |
| Hold `R2` | Fast-forward |
| Double-tap `R2` | Lock fast-forward; press again to unlock |
| `X` | Toggle the LCD effect |

Closing the lid writes a save state and turns off the display, audio and emulation. After five
minutes the device enters a lower-frequency standby that stops rendering but continues watching
`POWER` and the shutdown clock. Open the lid or press `POWER` during either five-minute stage to
resume. If the second stage expires, the device powers off and the next boot resumes from that
save state.

External power keeps a dark unit out of deep sleep, and an enumerated USB debugging session does
the same. Holding `POWER` still requests a real shutdown, including while charging.

## Updating an installation

1. Power off the RG SP.
2. Eject the card containing the active frontend volume: TF1 for one-card BaseOS, or TF2 for
   two-card BaseOS/AGS-102.
3. Back up `Games`, `Cartridges`, `Labels`, `Saves`, `States` and `Wallpapers`.
4. Replace the `System/` and hidden `.system/` directories with the ones from the new release.
5. Restore or keep your personal content directories and put the card back in the device.

Do not replace the BaseOS boot card with a new image merely to update the frontend unless the
release notes specifically require it.

## Building and checking

Development uses Rust `1.96` and [Task](https://taskfile.dev/). Host core builds may also need
the native desktop/audio libraries listed by the CI workflow. Device releases are aarch64 builds
for the H700 and use Docker or an equivalent arm64 build environment when necessary.

Run the same checks used by CI:

```sh
task check
```

Build, verify and package a device tree:

```sh
./scripts/build-release.sh --out dist-device
./scripts/verify-release.sh dist-device
./scripts/package-release.sh v1.0.0 --tree dist-device
```

Or build and package in one command:

```sh
./scripts/package-release.sh v1.0.0
```

The resulting ZIP and SHA-256 checksum are written to `dist/releases/`. The release scripts also
verify the aarch64 binaries, BaseOS launcher, license notices and corresponding source archives
before packaging.

## Known limitations and contributions

This fork has only been tested on the **Anbernic RG SP**. I have no reliable information about
how it runs on other handhelds, displays, SoCs or operating-system images. If you test it
elsewhere, please open an issue or send a PR with the exact device, OS version, release,
observed behaviour and, when possible, `System/slot.log`.

PRs are welcome. Useful contributions include compatibility testing, documentation fixes, artwork
support and bug fixes. Please do not commit commercial ROMs, BIOS files or other copyrighted
game data; attach logs and reproduction steps instead.

## Credits and licensing

### Upstream project

This repository is a fork of [`slot`](https://github.com/BrandonKowalski/slot) by **Brandon T.
Kowalski**. The original project, its GBA-focused frontend, design direction and upstream
implementation are the foundation of this fork. The original author must continue to receive
credit, and this fork is not affiliated with or endorsed by upstream.

The device environment [AGS-102](https://github.com/BrandonKowalski/AGS-102) is also by Brandon T.
Kowalski. [BaseOS](https://github.com/pvaibhav/BaseOS) is by @pvaibhav.

### Third-party software and assets

- [mGBA](https://mgba.io), by Jeffrey “endrift” Pfau, through [libretro](https://www.libretro.com).
- [gpSP](https://github.com/libretro/gpsp), originally by Gilead “Exophase” Kutnick, through
  libretro.
- [Gambatte](https://github.com/libretro/gambatte-libretro), through libretro.
- [Pixelify Sans](https://github.com/googlefonts/pixelify) and the original label font, under the
  SIL Open Font License.
- [Nerd Fonts](https://www.nerdfonts.com) symbols by Ryan L. McIntyre, under the MIT license.
- The GB and GBC screen overlays are from [Jeltr0n's Retro-Overlays](https://github.com/Jeltr0n/Retro-Overlays);
  the included files are `jeltron/GB_DMG.png` and `jeltron/GB_Color.png`.
- The panel mask is derived from LCD3x, a public-domain shader by Gigaherz in the libretro shader
  collection.
- The cartridge insertion/ejection sounds come from the upstream project.

The frontend is MIT-licensed under [`LICENSE`](LICENSE), including the original copyright notice.
The compiled mGBA, gpSP and Gambatte cores have their own MPL-2.0 or GPL-2.0 terms. Their license
texts and the source material required for the distributed GPL cores are kept in
[`licenses/`](licenses/) and copied into device releases.

### AI disclosure

The changes in this fork—including GB/GBC support, cartridge artwork handling, tests and this
documentation—were developed with assistance from AI tools. A human maintainer reviewed the
resulting changes. AI assistance does not change the authorship, license or attribution of the
original `slot` project or any third-party component.
