use std::ffi::{c_int, c_ulong, c_void};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{Battery, Charge, LedState, Platform};

/// The top step. Levels are 0 to 16 everywhere above the trait; what that spans in the
/// kernel's own units is whatever `max_brightness` says, which is 255 on some panels and
/// 100 on others.
const TOP_STEP: u32 = 16;

/// Perceptual brightness curve in H700's native eight-bit units. Human brightness perception
/// is not linear, so spend more of the nine visible levels at the dark end where equal raw
/// increments are useful and spread them increasingly far apart toward full brightness.
/// The extra values at 2 and 7 are the useful levels the old nine-step curve skipped.
const BACKLIGHT_CURVE: [u32; 17] = [
    0, 1, 2, 4, 7, 10, 15, 22, 31, 42, 57, 72, 91, 112, 140, 172, 255,
];

/// Turn the frontend's sixteen visible levels into the panel's native range. Scale the curve
/// against the range reported by other kernels, keeping every lit step non-zero even on a
/// particularly narrow backlight range.
fn backlight_value(step: u32, max: u32) -> u32 {
    let step = step.min(TOP_STEP) as usize;
    let value = u64::from(max) * u64::from(BACKLIGHT_CURVE[step]) / 255;
    if step == 0 || max == 0 {
        0
    } else {
        value.max(1) as u32
    }
}

/// Where the event nodes live. Only the motor is opened from here; the buttons are the
/// binary's own business.
const DEV_INPUT: &str = "/dev/input";

const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;
const I2C_SLAVE_FORCE: c_ulong = 0x0706;
const AXP_ADDR: c_int = 0x34;
/// `_IOW('E', 0x80, struct ff_effect)`. The struct is 48 bytes once the union's eight byte
/// alignment is counted, which is where the size in the middle of this comes from.
const EVIOCSFF: c_ulong = 0x4030_4580;

/// What `setbl` takes. The class publishes its own range in `max_brightness`; the display
/// driver's debugfs door has no such file and is eight bit.
const DISPDBG_MAX: u32 = 255;

/// How the panel is dimmed. The class is the standard and the H700 does not have one: its
/// display driver's only control surface is four files under debugfs, which is why the base
/// system mounts debugfs at all.
enum Backlight {
    Class(PathBuf),
    Dispdbg(PathBuf),
}

/// What was found under `/sys/class/leds`, and therefore how much of the policy can be
/// expressed. Nothing has ever been seen on real hardware here — BaseOS ships no LED
/// userland — so the shapes are the two the kernel's LED class defines, most capable first.
///
/// Unverified against real hardware; on arrival: `ls /sys/class/leds/` for whether a node
/// exists at all and what it is named, `cat .../max_brightness` for its range, and
/// `multi_index`/`color` (or a `:red`/`:green`/`:blue` sibling scheme, which neither shape
/// here discovers) to confirm a `multi_intensity` node's channel order before trusting the
/// R-G-B guess in `set_led`. None of that is the real test, though: plug in a charger and
/// confirm the LED visibly goes amber within a second, then unplug and confirm it returns to
/// green. If nothing changes either way, `cat /sys/class/power_supply/*/status` while
/// plugging and unplugging — a `status` attribute that stays blank (this PMIC's
/// `current_now` already reads empty, so it would not be the first) looks identical from here
/// to `probe` having picked the wrong node under `/sys/class/leds`, and only that command
/// tells the two apart.
enum Led {
    /// A single node taking "r g b" intensities.
    Multi(PathBuf),
    /// One brightness node, on or off.
    Mono { dir: PathBuf, max: u32 },
}

/// Everything the frontend touches on a Linux handheld, found by walking sysfs. Nothing here
/// is a hardcoded path: the board that ships with the next kernel names these differently
/// and an absent one has to read as a feature the device does not have, not as a boot
/// failure.
pub struct DevicePlatform {
    root: PathBuf,
    sysfs: PathBuf,
    backlight: Option<Backlight>,
    max_brightness: u32,
    battery: Option<PathBuf>,
    charger: Option<PathBuf>,
    /// Only ever reported. Whether the board keeps time with the power off decides what
    /// setting the clock can mean, and that is a bring-up question rather than a running one.
    rtc: Option<PathBuf>,
    motor: Option<Motor>,
    led: Option<Led>,
}

impl DevicePlatform {
    pub fn new(root: PathBuf) -> Self {
        DevicePlatform::probe(Path::new("/sys"), root)
    }

    /// Against an arbitrary sysfs root, which is what makes the probing testable off device.
    pub fn probe(sysfs: &Path, root: PathBuf) -> Self {
        let class = first_dir(&sysfs.join("class/backlight"), |d| {
            d.join("brightness").is_file()
        });
        let max_brightness = class
            .as_ref()
            .and_then(|d| read_number(&d.join("max_brightness")))
            .unwrap_or(DISPDBG_MAX);
        let backlight = match class {
            Some(dir) => Some(Backlight::Class(dir)),
            // Only where there is no class to prefer. A board with both is driven through the
            // standard one, which does not depend on debugfs being mounted.
            None => {
                let dbg = sysfs.join("kernel/debug/dispdbg");
                dbg.join("param")
                    .is_file()
                    .then_some(Backlight::Dispdbg(dbg))
            }
        };
        let battery = first_dir(&sysfs.join("class/power_supply"), is_battery);
        let charger = first_dir(&sysfs.join("class/power_supply"), |d| {
            fs::read_to_string(d.join("type"))
                .is_ok_and(|t| matches!(t.trim(), "USB" | "USB_C" | "Mains"))
                && d.join("online").is_file()
        });
        let rtc = first_dir(&sysfs.join("class/rtc"), |_| true);
        let motor = Motor::open(sysfs);
        let led = first_dir(&sysfs.join("class/leds"), |d| {
            d.join("multi_intensity").is_file() || d.join("brightness").is_file()
        })
        .map(|dir| {
            if dir.join("multi_intensity").is_file() {
                Led::Multi(dir)
            } else {
                let max = read_number(&dir.join("max_brightness")).unwrap_or(255);
                Led::Mono { dir, max }
            }
        });
        DevicePlatform {
            root,
            sysfs: sysfs.to_path_buf(),
            backlight,
            max_brightness,
            battery,
            charger,
            rtc,
            motor,
            led,
        }
    }

    /// A line on the card, opened and closed per call, so a shutdown that hangs leaves a
    /// record of how far it got. Deliberately not buffered and not batched: the whole point
    /// is to survive a machine that stops responding a moment later, and the last thing
    /// written is the thing worth knowing.
    ///
    /// Appends rather than truncates, unlike slot.log, because the interesting case spans
    /// the boot that follows.
    fn breadcrumb(&self, line: &str) {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("ags-shutdown-trace.log"))
        {
            let _ = writeln!(f, "{} {line}", self.now());
            // The boot breadcrumb is observability only and sits on the first-frame path.
            // Shutdown breadcrumbs still force the card because they are crash evidence; the
            // normal boot marker can safely be flushed by the filesystem in the background.
            if !line.starts_with("boot:") {
                let _ = f.sync_all();
            }
        }
    }

    /// Proof the trace works at all. Without it, an empty trace after a hung shutdown is two
    /// different findings wearing the same face: the shutdown path never ran, or the writing
    /// never worked.
    pub fn trace_boot(&self) {
        self.breadcrumb("boot: platform up, trace working");
    }

    /// One line for the log. Everything below this reads as "the device does not have one"
    /// when a node is missing, which is right for running and useless for bring-up: a panel
    /// that never dims and a panel that was never found look identical from the outside.
    pub fn report(&self) -> String {
        let leaf = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        };
        let name = |p: &Option<PathBuf>, absent: &str| match p {
            Some(p) => leaf(p),
            None => absent.to_string(),
        };
        let backlight = match &self.backlight {
            Some(Backlight::Class(dir)) => leaf(dir),
            Some(Backlight::Dispdbg(_)) => "dispdbg".to_string(),
            None => "no backlight".to_string(),
        };
        let motor = match &self.motor {
            Some(m) => format!("motor {}", m.name),
            None => "no motor".to_string(),
        };
        format!(
            "backlight {} (0 to {}), battery {}, {}, {motor}",
            backlight,
            self.max_brightness,
            name(&self.battery, "no battery"),
            name(&self.rtc, "no rtc"),
        )
    }
}

/// Best effort, but not silently so. `date` here is whatever busybox provides and it need not
/// take the same arguments as the one this was written against; a clock that never moved and a
/// clock that moved and was not saved are the same wrong time on the shelf.
fn ran(what: &str, result: std::io::Result<std::process::ExitStatus>) {
    match result {
        Ok(status) if status.success() => eprintln!("slot: {what} ok"),
        Ok(status) => eprintln!("slot: {what} {status}"),
        Err(e) => eprintln!("slot: {what}: {e}"),
    }
}

/// sysfs prints a capability bitmask as 64 bit words in hex, most significant first, so the
/// last word holds bits 0 to 63 and an offset only comes out right counted from the end.
pub fn has_bit(mask: &str, bit: u16) -> bool {
    let words: Vec<&str> = mask.split_whitespace().collect();
    let from_end = usize::from(bit) / 64;
    let Some(word) = words.len().checked_sub(from_end + 1).map(|i| words[i]) else {
        return false;
    };
    u64::from_str_radix(word, 16).is_ok_and(|w| w >> (bit % 64) & 1 == 1)
}

/// The event node whose driver says it can rumble, by name. Found the way the buttons are,
/// because `event1` is the pad on one boot and something else on the next.
pub fn rumble_node(sysfs: &Path) -> Option<String> {
    let dir = first_dir(&sysfs.join("class/input"), |node| {
        fs::read_to_string(node.join("device/capabilities/ff"))
            .is_ok_and(|ff| has_bit(&ff, FF_RUMBLE))
    })?;
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

/// What the motor should be told, given a new strength and what it is doing now. `None` is
/// the common case twice over: the core asks for the same thing most frames, and this driver
/// ignores magnitude entirely, so anything above zero is the same buzz.
pub fn motor_change(strength: u16, running: bool) -> Option<bool> {
    match (strength > 0, running) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// One effect, uploaded once and held. The driver latches magnitude when the effect is built
/// rather than when it is played, so there is nothing to be gained by rebuilding it for a new
/// strength — and doing so puts a blip through the motor on every change.
struct Motor {
    node: fs::File,
    /// Only ever reported. A board with no motor and a motor that refused to take an effect
    /// are the same still cart from outside.
    name: String,
    id: i16,
    running: bool,
}

#[repr(C)]
#[derive(Default)]
struct FfTrigger {
    button: u16,
    interval: u16,
}

#[repr(C)]
#[derive(Default)]
struct FfReplay {
    length: u16,
    delay: u16,
}

#[repr(C)]
#[derive(Default)]
struct FfRumble {
    strong: u16,
    weak: u16,
}

/// `struct ff_effect`. The union after `replay` is eight byte aligned, so it starts at 16 and
/// the whole thing is 48; `_align` and `_tail` are that padding written down.
#[repr(C)]
struct FfEffect {
    kind: u16,
    id: i16,
    direction: u16,
    trigger: FfTrigger,
    replay: FfReplay,
    _align: u16,
    rumble: FfRumble,
    _tail: [u8; 28],
}

/// `struct input_event`, the same 24 bytes the buttons arrive in.
#[repr(C)]
struct FfEvent {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

impl Motor {
    /// `None` where there is no motor, which is a device that does not buzz rather than a
    /// boot failure.
    fn open(sysfs: &Path) -> Option<Motor> {
        let name = rumble_node(sysfs)?;
        let node = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(DEV_INPUT).join(&name))
            .map_err(|e| eprintln!("slot: rumble {name}: {e}"))
            .ok()?;
        // Length zero is held until it is stopped, which is what a strength that persists
        // across frames needs. Full magnitude because the driver has only the one.
        let mut effect = FfEffect {
            kind: FF_RUMBLE,
            id: -1,
            direction: 0,
            trigger: FfTrigger::default(),
            replay: FfReplay::default(),
            _align: 0,
            rumble: FfRumble {
                strong: u16::MAX,
                weak: u16::MAX,
            },
            _tail: [0; 28],
        };
        let rc = unsafe {
            ioctl(
                node.as_raw_fd(),
                EVIOCSFF,
                &mut effect as *mut FfEffect as *mut c_void,
            )
        };
        if rc < 0 {
            eprintln!("slot: rumble {name}: {}", std::io::Error::last_os_error());
            return None;
        }
        Some(Motor {
            node,
            name,
            id: effect.id,
            running: false,
        })
    }

    fn play(&mut self, on: bool) {
        let ev = FfEvent {
            sec: 0,
            usec: 0,
            kind: EV_FF,
            code: self.id as u16,
            value: i32::from(on),
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(&ev as *const FfEvent as *const u8, size_of::<FfEvent>())
        };
        match (&self.node).write_all(bytes) {
            Ok(()) => self.running = on,
            Err(e) => eprintln!("slot: rumble: {e}"),
        }
    }
}

/// A held effect outlives the process that started it, so a frontend that stops without
/// putting the motor down leaves the device buzzing in someone's hand.
impl Drop for Motor {
    fn drop(&mut self) {
        if self.running {
            self.play(false);
        }
    }
}

/// Entries in name order, so a tree with two panels picks the same one on every boot.
fn first_dir(parent: &Path, keep: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut names: Vec<PathBuf> = fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    names.sort();
    names.into_iter().find(|p| keep(p))
}

/// A charger is a power supply too, and it has no charge of its own to report.
fn is_battery(dir: &Path) -> bool {
    fs::read_to_string(dir.join("type")).is_ok_and(|t| t.trim() == "Battery")
        && dir.join("capacity").is_file()
}

fn read_number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The kernel names five states and this cares about three. Anything else — "Not charging"
/// on a charger that is not filling, an empty file, a node that is not there — is the
/// reading that leaves every policy where it was.
fn charge_at(dir: &Path) -> Charge {
    let Ok(text) = fs::read_to_string(dir.join("status")) else {
        return Charge::Unknown;
    };
    match text.trim() {
        "Charging" => Charge::Charging,
        "Discharging" => Charge::Discharging,
        "Full" => Charge::Full,
        _ => Charge::Unknown,
    }
}

fn active_charge(battery: &Path, charger: Option<&Path>) -> Charge {
    let Some(charger) = charger else {
        return charge_at(battery);
    };
    let online = read_number(&charger.join("online")).is_some_and(|v| v == 1);
    let time_to_full = read_number(&battery.join("time_to_full_now")).unwrap_or(0);
    if online && time_to_full > 0 {
        return Charge::Charging;
    }
    match charge_at(battery) {
        Charge::Full => Charge::Full,
        Charge::Discharging if !online => Charge::Discharging,
        _ => Charge::Unknown,
    }
}

/// Sysfs node for the PMIC, e.g. `/sys/bus/i2c/devices/5-0034`.
fn axp_i2c_sysfs() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/bus/i2c/devices").ok()?;
    for entry in entries.flatten() {
        let leaf = entry.file_name().to_string_lossy().into_owned();
        let Some((_, address)) = leaf.split_once('-') else {
            continue;
        };
        if address != "0034" && address != "34" {
            continue;
        }
        if fs::read_to_string(entry.path().join("name"))
            .is_ok_and(|n| n.trim_start().starts_with("axp"))
        {
            return Some(entry.path());
        }
    }
    None
}

fn axp_i2c_devnode(sysfs: Option<&Path>) -> PathBuf {
    sysfs
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .and_then(|leaf| {
            leaf.split_once('-')
                .map(|(bus, _)| format!("/dev/i2c-{bus}"))
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev/i2c-5"))
}

/// `I2C_SLAVE` is EBUSY here: the kernel axp2202 driver already owns 0x34. Unbinding that
/// driver to free the address hung the process in D-state (the shutdown trace stopped at
/// "trying AXP2202 rail cut" and the panel sat on Powering Down until a 10 s hardware hold).
/// `I2C_SLAVE_FORCE` is what `i2cset -f` uses — talk to the chip without ripping the MFD out.
fn claim_axp_i2c() -> std::io::Result<std::fs::File> {
    let path = axp_i2c_devnode(axp_i2c_sysfs().as_deref());
    let bus = OpenOptions::new().read(true).write(true).open(&path)?;
    if unsafe { ioctl(bus.as_raw_fd(), I2C_SLAVE_FORCE, AXP_ADDR) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(bus)
}

/// The stock H700 kernel's generic AXP power-off register is wrong for the AXP2202. Quiesce
/// pending wake IRQs before asking its real soft-power register to cut the rails; otherwise
/// a held button or attached charger can turn a black-screen shutdown straight back on.
fn axp2202_poweroff() -> std::io::Result<()> {
    let mut bus = claim_axp_i2c()?;
    let mut register = |reg: u8, value: u8| bus.write_all(&[reg, value]);
    for reg in 0x40..=0x44 {
        register(reg, 0x00)?;
    }
    for reg in 0x48..=0x4c {
        register(reg, 0xff)?;
    }
    register(0x22, 0x0a)?;
    std::thread::sleep(Duration::from_millis(50));
    register(0x27, 0x01)
}

/// I2C can block uninterruptibly. A hung write must not pin the shutdown screen forever;
/// init still has a chance if this gives up.
fn axp2202_poweroff_or_timeout() -> std::io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("axp-poweroff".into())
        .spawn(move || {
            let _ = tx.send(axp2202_poweroff());
        })
        .map_err(std::io::Error::other)?;
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "AXP I2C did not return"))?
}

/// How long Super Standby may last before the rails are cut. The dark-panel grace is
/// `Power`'s own timeout (five minutes of 400-700 mA); this is the second stage, spent
/// inside `echo mem`.
const SUPER_STANDBY: Duration = Duration::from_secs(300);

/// The stock H700 userspace writes 16 here. This is an enable switch, not a duration: the
/// vendor driver treats every non-zero value alike. A timed shutdown therefore needs an RTC
/// alarm to bring userspace back from suspend; writing `5` here does not mean five minutes.
const OS_SLEEP_ENABLED: &str = "16";

fn arm_wakealarm(rtc: Option<&Path>, after: Duration) -> std::io::Result<()> {
    let rtc = rtc.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no RTC available for timed wake",
        )
    })?;
    let alarm = rtc.join("wakealarm");
    fs::write(&alarm, "0")?;
    // Linux's RTC sysfs accepts a relative alarm. This does not depend on the wall clock
    // having been set yet, which matters on a freshly flashed unit without network time.
    fs::write(alarm, format!("+{}", after.as_secs().max(1)))
}

impl Platform for DevicePlatform {
    /// Step 0 is the panel off rather than the panel dim: it is what the lid closes with, and
    /// any floor under it lights a shut clamshell.
    fn set_backlight(&mut self, step: u8) {
        let Some(backlight) = &self.backlight else {
            return;
        };
        let value = backlight_value(u32::from(step), self.max_brightness);
        // A node that was found and will not take a write is a different fault from one that
        // was never there, and from outside they are the same dark panel.
        let write = |file: PathBuf, body: String| {
            if let Err(e) = fs::write(&file, body) {
                eprintln!("slot: {}: {e}", file.display());
            }
        };
        match backlight {
            Backlight::Class(dir) => write(dir.join("brightness"), value.to_string()),
            // In this order, and `start` last: the first three are arguments and the fourth
            // is what runs the command with them.
            Backlight::Dispdbg(dir) => {
                write(dir.join("name"), "lcd0".to_string());
                write(dir.join("command"), "setbl".to_string());
                write(dir.join("param"), value.to_string());
                write(dir.join("start"), "1".to_string());
            }
        }
    }

    /// None where there is no gauge, and none where it will not parse. Zero would be a
    /// battery critical, which flushes and powers the device off mid game.
    fn battery(&self) -> Option<Battery> {
        let dir = self.battery.as_ref()?;
        let percent = read_number(&dir.join("capacity"))?;
        Some(Battery {
            percent: percent.min(100) as u8,
            charge: active_charge(dir, self.charger.as_deref()),
        })
    }

    fn charge(&self) -> Charge {
        self.battery.as_ref().map_or(Charge::Unknown, |dir| {
            active_charge(dir, self.charger.as_deref())
        })
    }

    fn charger_present(&self) -> bool {
        self.charger
            .as_ref()
            .and_then(|d| read_number(&d.join("online")))
            .is_some_and(|v| v == 1)
    }

    fn usb_host(&self) -> bool {
        first_dir(&self.sysfs.join("class/udc"), |d| {
            fs::read_to_string(d.join("state")).is_ok_and(|s| s.trim() == "configured")
        })
        .is_some()
    }

    fn suspend(&mut self) -> bool {
        let battery = self.sysfs.join("class/power_supply/axp2202-battery");
        let hall = battery.join("hallkey");
        let os_sleep = battery.join("os_sleep");
        let work_led = battery.join("work_led");
        let _ = fs::write(&work_led, "0");
        let _ = Command::new("sync").status();

        let deadline = SystemTime::now() + SUPER_STANDBY;
        loop {
            // `Instant` is CLOCK_MONOTONIC on Linux and stops while the machine is suspended.
            // Wall time advances across suspend, so the RTC wake can actually exhaust this.
            let remaining = deadline
                .duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO);
            if remaining.is_zero() {
                // Five minutes in Super Standby with the lid still shut: cut the rails.
                // resume.state was written before this loop, so the next boot seats the cart.
                let _ = fs::write(&work_led, "1");
                return false;
            }
            if os_sleep.is_file() {
                // Match the vendor userspace and NextUI's H700 port: non-zero enables Super
                // Standby. The RTC alarm below, not this value, supplies the five-minute wake.
                let _ = fs::write(&os_sleep, OS_SLEEP_ENABLED);
            }
            if let Err(e) = arm_wakealarm(self.rtc.as_deref(), remaining) {
                eprintln!("slot: suspend: could not arm timed wake: {e}");
                let _ = fs::write(&work_led, "1");
                return false;
            }
            let status = Command::new("sh")
                .args(["-c", "echo mem > /sys/power/state"])
                .status();
            if !status.is_ok_and(|s| s.success()) {
                let _ = fs::write(&work_led, "1");
                return false;
            }
            // A pocketed power press can wake the kernel. Do not light a closed clamshell:
            // go back to sleep for whatever of the five minutes is left.
            if fs::read_to_string(&hall).map_or(true, |v| v.trim() != "0") {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        if let Some(rtc) = self.rtc.as_deref() {
            let _ = fs::write(rtc.join("wakealarm"), "0");
        }
        let _ = fs::write(&work_led, "1");
        true
    }

    fn set_led(&mut self, state: LedState) {
        let Some(led) = &self.led else {
            return;
        };
        match led {
            Led::Multi(dir) => {
                let (r, g, b) = match state {
                    LedState::Off => (0, 0, 0),
                    LedState::Running | LedState::Charged => (0, 255, 0),
                    LedState::Low => (255, 0, 0),
                    LedState::Charging => (255, 140, 0),
                };
                let _ = fs::write(dir.join("multi_intensity"), format!("{r} {g} {b}\n"));
                let _ = fs::write(dir.join("brightness"), "255\n");
            }
            // One distinction is all a single brightness carries without a blink timer, and
            // the fastest tick here is a second. Low is what it is spent on.
            Led::Mono { dir, max } => {
                let on = !matches!(state, LedState::Off | LedState::Low);
                let value = if on { *max } else { 0 };
                let _ = fs::write(dir.join("brightness"), format!("{value}\n"));
            }
        }
    }

    fn restart(&mut self) -> ! {
        self.breadcrumb("restart: reached slot, about to sync");
        let _ = Command::new("sync").status();
        self.breadcrumb("restart: sync returned, about to signal init");
        let _ = Command::new("reboot").status();
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(0)
    }

    fn poweroff(&mut self) -> ! {
        self.breadcrumb("poweroff: reached slot, about to sync");
        let _ = Command::new("sync").status();
        self.breadcrumb("poweroff: sync returned, trying AXP2202 rail cut");
        match axp2202_poweroff_or_timeout() {
            Ok(()) => {
                // A successful PMIC write normally never reaches the end of this sleep.
                std::thread::sleep(Duration::from_secs(1));
                self.breadcrumb("poweroff: AXP2202 write returned but rails stayed up");
            }
            Err(e) => self.breadcrumb(&format!("poweroff: AXP2202 unavailable: {e}")),
        }
        self.breadcrumb("poweroff: about to signal init fallback");
        let _ = Command::new("poweroff").status();
        self.breadcrumb("poweroff: signalled init, waiting for it to take the machine down");
        // The card is already flushed, so the worst case is a frontend BaseOS respawns
        // rather than a device that hangs on a button that did nothing.
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(0)
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Both halves matter: the first moves the running clock, the second is what survives the
    /// battery coming out. Best effort, since a device with no RTC still has a session's
    /// worth of correct time after the first.
    fn set_clock(&mut self, secs: i64) {
        ran(
            "date",
            Command::new("date").arg(format!("-s@{secs}")).status(),
        );
        ran(
            "hwclock",
            Command::new("hwclock").args(["-w", "-u"]).status(),
        );
    }

    /// The base system's role manager clears the gadget's UDC binding when the cable goes,
    /// and it runs no reconnect watcher on purpose: the documented recovery is to reboot with
    /// the cable in. Rewriting `g1/UDC` is the one mechanism it calls safe and proven, so
    /// that is the whole of this.
    ///
    /// Nothing here goes near the role manager. Writing `usbc0/otg_role` wedges the writer in
    /// an uninterruptible state forever, and its `usb_device`, `usb_host` and `usb_null`
    /// siblings are read triggers that can switch the port merely by being looked at. This
    /// cannot help when a later attach put the controller in host role; then it is still a
    /// reboot, which is why it reports whether it did anything at all.
    fn relink_adb(&mut self) -> bool {
        let gadget = self.sysfs.join("kernel/config/usb_gadget/g1/UDC");
        if !gadget.is_file() {
            return false;
        }
        let Some(udc) = first_dir(&self.sysfs.join("class/udc"), |_| true) else {
            return false;
        };
        let Some(name) = udc.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            return false;
        };
        // Unbound first: rebinding the name it already holds is not a re-enumeration.
        let _ = fs::write(&gadget, "\n");
        match fs::write(&gadget, &name) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("slot: adb {}: {e}", gadget.display());
                false
            }
        }
    }

    /// On or off. Measured on the device: 16383 and 65535 are the same buzz, and neither an
    /// in place update nor a re-trigger moves it, so the core's 0 to 65535 is a switch here
    /// however much it looks like a level.
    fn set_rumble(&mut self, strength: u16) {
        let Some(motor) = &mut self.motor else {
            return;
        };
        if let Some(on) = motor_change(strength, motor.running) {
            motor.play(on);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axp_i2c_devnode_reads_the_bus_out_of_the_sysfs_leaf() {
        assert_eq!(
            axp_i2c_devnode(Some(Path::new("/sys/bus/i2c/devices/5-0034"))),
            PathBuf::from("/dev/i2c-5")
        );
        assert_eq!(
            axp_i2c_devnode(None),
            PathBuf::from("/dev/i2c-5"),
            "the H700 fallback is bus 5"
        );
    }

    #[test]
    fn a_relative_rtc_alarm_is_armed_without_needing_a_valid_wall_clock() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("wakealarm"), "").unwrap();
        arm_wakealarm(Some(d.path()), Duration::from_secs(300)).unwrap();
        assert_eq!(
            fs::read_to_string(d.path().join("wakealarm")).unwrap(),
            "+300"
        );
    }
}
