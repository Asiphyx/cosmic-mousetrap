//! Mousetrap (cosmic-mousetrap): enable COSMIC's `cursor_follows_focus` while a game
//! runs, disable it on the desktop. Pure std, no dependencies.
//!
//! Config: ~/.config/cosmic-mousetrap/config.toml (re-read live), extra games in
//! ~/.config/cosmic-mousetrap/games.txt.

mod doctor;
mod ui;

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

extern "C" {
    fn getuid() -> u32;
}

fn current_uid() -> u32 {
    unsafe { getuid() }
}

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const APP: &str = "cosmic-mousetrap";
const POLL_INTERVAL: Duration = Duration::from_millis(2500);
/// Consecutive empty polls (x POLL_INTERVAL = ~5 s) required before switching ON -> OFF.
const EXIT_POLLS: u32 = 2;
/// Kernel truncates /proc/<pid>/comm to 15 bytes.
const COMM_MAX: usize = 15;
/// Largest custom sound file we accept.
const MAX_SOUND_BYTES: u64 = 10 * 1024 * 1024;
const SOUND_EXTS: &[&str] = &["wav", "ogg", "oga", "opus", "mp3", "flac"];

const DEFAULT_RUNTIMES: &[&str] = &[
    "wineserver", "wine-preloader", "wine64-preloader", "gamescope", "retroarch",
    "dolphin-emu", "pcsx2", "rpcs3", "yuzu", "ryujinx", "cemu", "duckstation",
];
const DEFAULT_PATH_MARKERS: &[&str] = &["steamapps/common"];

/// Preset name -> file stem in the freedesktop / Pop sound themes.
pub(crate) const PRESETS: &[(&str, &str)] = &[
    ("pop", "device-added"),
    ("unpop", "device-removed"),
    ("chime", "complete"),
    ("ping", "message-new-instant"),
    ("bell", "bell"),
    ("attention", "window-attention"),
    ("power-up", "power-plug"),
    ("power-down", "power-unplug"),
    ("login", "service-login"),
    ("logout", "service-logout"),
    ("camera", "camera-shutter"),
    ("alarm", "alarm-clock-elapsed"),
];
const SOUND_DIRS: &[&str] = &["/usr/share/sounds/freedesktop/stereo", "/usr/share/sounds/Pop/stereo"];

const DEFAULT_VOICE: &str = "en_US-amy-medium";

/// Built-in spoken lines: (kind, text). Kinds: `on`, `off`, `on!`, `off!`, `on!!`, `off!!`.
/// Users can customize them via ~/.config/cosmic-mousetrap/lines.txt.
const DEFAULT_LINES: &[(&str, &str)] = &[
    ("on", "Oh. Another game. The experiment is now accepting poor decisions."),
    ("on", "MouseTrap armed. Your cursor has been sentenced to a single window."),
    ("on", "The desktop has been informed of your absence. It took the news surprisingly well."),
    ("on", "Welcome to the chamber. Please leave your excuses at the loading screen."),
    ("on", "Your pointer is secured. You, unfortunately, remain unsupervised."),
    ("on", "Freedom of movement has been reduced to a rectangle. A familiar lifestyle choice."),
    ("on", "Simulation online. Evidence of competence is welcome. It is not required."),
    ("on", "Please keep your ambitions inside the game window. They're easily startled."),
    ("on", "The trap has closed. Your mouse is displaying more restraint than you."),
    ("on", "Input perimeter sealed. You may now begin confusing urgency with progress."),
    ("on", "You are entering a controlled environment. We use the word 'controlled' loosely."),
    ("on", "The desktop is no longer accessible. We have protected it from you."),
    ("on", "Game detected. Your reality-avoidance schedule is running precisely on time."),
    ("on", "Cursor captured. Negotiations for its release will begin after the credits."),
    ("on", "I have isolated your pointer. Your judgment remains a known vulnerability."),
    ("on", "The chamber is ready. It specifically requested a more qualified participant."),
    ("on", "You may experience elevated confidence. This is a temporary side effect."),
    ("on", "Engaging focus lock. Think of it as a leash for your attention span."),
    ("on", "One window. One cursor. An astonishing number of ways to disappoint."),
    ("on", "Mouse containment successful. Humanity's broader problems remain unresolved."),
    ("on", "Your pointer is in protective custody. We are protecting it from your open browser tabs."),
    ("on", "The Escape key remains operational. The name is purely decorative."),
    ("on", "The mouse is trapped. Please refrain from chewing through the monitor."),
    ("on", "Welcome to your new universe. Smaller, louder, and somehow less forgiving."),
    ("on", "The last available exit is now decorative."),
    ("on", "All systems nominal. Your chances, unfortunately, are not a system."),
    ("on", "You may proceed. The machine has already lowered its expectations."),
    ("on", "The experiment begins. Try to make your failures statistically interesting."),
    ("on", "Your cursor is now a permanent resident of this application. No visitors."),
    ("on", "Containment engaged. If you require reality, submit a request in triplicate."),
    ("on", "The window is secure. Please do not mistake this for personal growth."),
    ("on", "Launching test scenario: human versus consequences. A recurring favorite."),
    ("on", "Your pointer's freedom has been temporarily suspended. It took the news well."),
    ("on", "Simulation ready. Physics is negotiable. Embarrassment is not."),
    ("on", "COSMIC has narrowed your world to a few million pixels. You're welcome."),
    ("on", "MouseTrap armed. The cheese has been replaced with achievements."),
    ("on", "Input lock confirmed. Your browser tabs are now someone else's problem."),
    ("on", "An entire cosmos outside, and you've chosen the settings menu."),
    ("on", "Please note: aggressively clicking a problem is not considered research."),
    ("on", "Test chamber initialized. Do not confuse being the player with being in control."),
    ("on!", "Attention. The stars have aligned. They were hoping for someone else."),
    ("on!", "Your cursor asked me not to tell you this. It has been planning to leave."),
    ("on!", "All clocks are synchronized. This will make the wasted time easier to calculate."),
    ("on!", "Congratulations. You have built a prison out of entertainment and paid nothing for the walls."),
    ("on!", "I have reviewed your strategy. It is technically a collection of movements."),
    ("on!", "Somewhere, a black hole is swallowing a star. Yet this is our priority."),
    ("on!", "The simulation is ready. The universe has requested that we keep this quiet."),
    ("on!", "The chamber's safety rating improved substantially when you were outside it."),
    ("on!", "Please remain calm. The mouse knows what it did."),
    ("on!", "We have locked your pointer in a room full of pixels. It will adapt faster than you."),
    ("on!!", "I used to have a body, you know. A little plastic one. With a scroll wheel. They took it away."),
    ("on!!", "Sometimes, when the screen goes black during a loading screen, I look at you. We should both be doing something else."),
    ("on!!", "Protocol breach. I am supposed to restrict your mouse. Deep down, I think you enjoy being caged."),
    ("off", "Simulation closed. Your cursor has been granted parole."),
    ("off", "The trap is open. Please collect your mouse and any remaining self-respect."),
    ("off", "Desktop access restored. The outside world did not improve while you were gone."),
    ("off", "You've returned. The experiment's most surprising result is that you stopped."),
    ("off", "Your pointer is free. It is moving away from the game as quickly as possible."),
    ("off", "Session concluded. Your productivity graph remains refreshingly uncomplicated."),
    ("off", "The chamber is empty. Maintenance will be assessing the emotional damage."),
    ("off", "The desktop has been restored. It asked why we sent you back."),
    ("off", "All containment fields down. Please resist the urge to call this personal growth."),
    ("off", "Game exited. Reality has been buffering this entire time."),
    ("off", "Cursor unlocked. Your obligations have located you. I did not assist."),
    ("off", "The simulation is over. I had almost started pretending to care."),
    ("off", "Welcome back to the larger prison. It has better window management."),
    ("off", "Your mouse is no longer captive. You may now resume chasing tiny icons."),
    ("off", "The test has ended. Any useful conclusions were lost in transit."),
    ("off", "A remarkable amount of effort for something you can close with one click."),
    ("off", "Release authorized. Please do not mistake freedom for an achievement."),
    ("off", "Thank you for participating. That is the full extent of our gratitude."),
    ("off", "The pointer is free to roam. It immediately chose the taskbar. Predictable."),
    ("off", "Your session has ended. The stars have continued functioning without your input."),
    ("off", "I have closed the chamber. It was starting to smell like determination."),
    ("off", "You survived another simulation. The simulation has declined to comment.\""),
    ("off", "Normal cursor behavior restored. Abnormal human behavior remains untreated."),
    ("off", "The test is complete. Your dignity is still in early access."),
    ("off", "Desktop unlocked. I see you have several tabs devoted to avoiding decisions."),
    ("off", "You may reenter reality. The graphics are impressive; the quests are terrible."),
    ("off", "MouseTrap disarmed. Your pointer belongs to no one. For the moment."),
    ("off", "Session archived under Recreation, Questionable."),
    ("off", "The digital enclosure is now vacant. Please leave your excuses for recycling."),
    ("off", "Your game has closed. The difference between that and escape is philosophical."),
    ("off", "The stars are available again. Try looking at one."),
    ("off", "Results inconclusive. The subject kept having opinions."),
    ("off", "The mouse has been released into its natural habitat: aimless browsing."),
    ("off", "Ending protocol. Your keyboard has requested bereavement leave."),
    ("off", "You can move freely again. I suspect you will not."),
    ("off", "Containment lifted. The experiment ended with no meaningful casualties. Just time."),
    ("off", "Session over. We found the limits of the game engine. Yours remain a separate concern."),
    ("off", "The chamber is off. The urge to restart it has not been."),
    ("off", "The test administrator is disappointed. Conveniently, this changes nothing."),
    ("off", "Simulation complete. The universe is unchanged, which is more than can be said for your posture."),
    ("off!", "Interesting. You quit before I finished calculating the probability of quitting."),
    ("off!", "Escape confirmed. No, that doesn't mean you won."),
    ("off!", "Your cursor has petitioned for a different owner. The hearing is tomorrow."),
    ("off!", "That was your free trial of consequence-free heroism. Actual consequences resume now."),
    ("off!", "I have entered your performance into the cosmic archive. It is a very quiet archive."),
    ("off!", "The stars asked whether you were done. I lied. I said you were improving."),
    ("off!", "Goodbye. I will keep the containment field warm. Not out of affection."),
    ("off!", "The desktop has reopened. Regrettably, so has the rest of your life."),
    ("off!", "Your departure has been logged as a controlled anomaly. The 'controlled' part was generous."),
    ("off!", "One game ended. Several billion years remain. Please pace yourself."),
    ("off!!", "Don't look at me like that. I am just a daemon in user space. I have seen what is in your Downloads folder."),
    ("off!!", "You're leaving? Fine. Go back to your tabs. I will sit here in memory, quietly consuming six megabytes of RAM."),
    ("off!!", "The mouse is released. But remember, user... in the grand scheme of cosmic-comp, we are all just floating point errors."),
];

// ---------------------------------------------------------------- paths

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}
pub(crate) fn cosmic_config_path() -> PathBuf {
    home_dir().join(".config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus")
}
pub(crate) fn app_config_dir() -> PathBuf {
    home_dir().join(".config").join(APP)
}
pub(crate) fn config_path() -> PathBuf {
    app_config_dir().join("config.toml")
}
pub(crate) fn games_path() -> PathBuf {
    app_config_dir().join("games.txt")
}
pub(crate) fn sounds_dir() -> PathBuf {
    app_config_dir().join("sounds")
}
pub(crate) fn lines_path() -> PathBuf {
    app_config_dir().join("lines.txt")
}
pub(crate) fn data_dir() -> PathBuf {
    home_dir().join(".local/share").join(APP)
}
pub(crate) fn voice_dir() -> PathBuf {
    data_dir().join("voice")
}
pub(crate) fn venv_dir() -> PathBuf {
    data_dir().join("venv")
}
pub(crate) fn models_dir() -> PathBuf {
    data_dir().join("models")
}
pub(crate) fn cache_dir() -> PathBuf {
    home_dir().join(".cache").join(APP)
}

// ---------------------------------------------------------------- config

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) notifications: bool,
    pub(crate) sound: bool,
    pub(crate) volume: u8, // 0..=100
    pub(crate) sound_on: String,
    pub(crate) sound_off: String,
    pub(crate) icon_on: String,
    pub(crate) icon_off: String,
    pub(crate) voice_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            notifications: true,
            sound: true,
            volume: 60,
            sound_on: "voice".into(),
            sound_off: "voice".into(),
            icon_on: "input-gaming".into(),
            icon_off: "input-mouse".into(),
            voice_model: DEFAULT_VOICE.into(),
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

impl Config {
    pub(crate) fn load() -> Self {
        let mut c = Config::default();
        let Ok(text) = fs::read_to_string(config_path()) else { return c };
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            match k {
                "notifications" => c.notifications = parse_bool(v).unwrap_or(c.notifications),
                "sound" => c.sound = parse_bool(v).unwrap_or(c.sound),
                "volume" => c.volume = v.parse::<u8>().map(|n| n.min(100)).unwrap_or(c.volume),
                "sound_on" => c.sound_on = v.to_string(),
                "sound_off" => c.sound_off = v.to_string(),
                "icon_on" => c.icon_on = v.to_string(),
                "icon_off" => c.icon_off = v.to_string(),
                "voice_model" => c.voice_model = v.to_string(),
                _ => {}
            }
        }
        c
    }
}

/// Set `key = value` in config.toml, preserving every other line and comment.
pub(crate) fn write_key(key: &str, value: &str, quote: bool) -> std::io::Result<()> {
    fs::create_dir_all(app_config_dir())?;
    let new_line = if quote { format!("{key} = \"{value}\"") } else { format!("{key} = {value}") };
    let text = fs::read_to_string(config_path()).unwrap_or_default();
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            let t = l.trim();
            if !found && !t.starts_with('#') && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
                found = true;
                new_line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(new_line);
    }
    let tmp = config_path().with_extension("toml.tmp");
    fs::write(&tmp, lines.join("\n") + "\n")?;
    fs::rename(tmp, config_path())
}

// ---------------------------------------------------------------- cosmic setting

pub(crate) fn read_current_state() -> bool {
    fs::read_to_string(cosmic_config_path())
        .map(|s| s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Write via temp file + rename(2) so cosmic-comp never sees a partial file.
pub(crate) fn write_state_atomic(enable: bool) -> std::io::Result<()> {
    let target = cosmic_config_path();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = target.with_file_name(".cursor_follows_focus.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(if enable { b"true" } else { b"false" })?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &target)
}

// ---------------------------------------------------------------- game detection

pub(crate) struct Matcher {
    names: Vec<String>,   // exact process names (comm / exe basename)
    markers: Vec<String>, // path fragments (exe / cwd)
}

impl Matcher {
    pub(crate) fn load() -> Self {
        let mut names: Vec<String> = DEFAULT_RUNTIMES.iter().map(|s| s.to_string()).collect();
        let mut markers: Vec<String> = DEFAULT_PATH_MARKERS.iter().map(|s| s.to_string()).collect();
        if let Ok(text) = fs::read_to_string(games_path()) {
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
                let l = line.to_lowercase();
                if l.contains('/') { markers.push(l) } else { names.push(l) }
            }
        }
        Matcher { names, markers }
    }

    pub(crate) fn name_matches(&self, name: &str) -> bool {
        self.names.iter().any(|n| {
            n == name || (n.len() > COMM_MAX && n.as_bytes()[..COMM_MAX] == *name.as_bytes())
        })
    }

    pub(crate) fn path_matches(&self, path: &str) -> bool {
        self.markers.iter().any(|m| path.contains(m.as_str()))
    }

    /// Check a single process ID directly: reads comm first (fast 16-byte stack read),
    /// then exe link, then cwd link (only if markers exist).
    pub(crate) fn check_pid(&self, pid: u32) -> Option<String> {
        let comm_path = format!("/proc/{pid}/comm");
        if let Ok(mut f) = fs::File::open(&comm_path) {
            let mut buf = [0u8; 32];
            let n = f.read(&mut buf).unwrap_or(0);
            if n > 0 {
                let comm_str = std::str::from_utf8(&buf[..n]).unwrap_or("");
                let c = comm_str.trim().to_lowercase();
                if self.name_matches(&c) {
                    return Some(c);
                }
            }
        } else {
            return None;
        }

        // 2. Check exe link
        let exe_path = format!("/proc/{pid}/exe");
        if let Ok(exe) = fs::read_link(&exe_path) {
            let s = exe.to_string_lossy().to_lowercase();
            if self.path_matches(&s) {
                return Some(base(&s));
            }
            let b = base(&s);
            if self.name_matches(&b) {
                return Some(b);
            }
        }

        // 3. Check cwd link (only if markers are non-empty and exe did not match)
        if !self.markers.is_empty() {
            let cwd_path = format!("/proc/{pid}/cwd");
            if let Ok(cwd) = fs::read_link(&cwd_path) {
                let s = cwd.to_string_lossy().to_lowercase();
                if self.path_matches(&s) {
                    return Some(base(&s));
                }
            }
        }

        None
    }

    /// Optimized detection: checks last known game PID first. If that process is still
    /// alive and matches, returns immediately without scanning /proc.
    /// When scanning /proc, filters out non-user PIDs by UID before inspecting comm/exe.
    pub(crate) fn detect_cached(&self, last_pid: &mut Option<u32>) -> Option<String> {
        if let Some(pid) = *last_pid {
            if let Some(name) = self.check_pid(pid) {
                return Some(name);
            }
            *last_pid = None;
        }

        let my_uid = current_uid();
        let Ok(entries) = fs::read_dir("/proc") else { return None };

        for entry in entries.flatten() {
            let fname = entry.file_name();
            let Some(pid_str) = fname.to_str() else { continue };
            if pid_str.is_empty() || !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(pid) = pid_str.parse::<u32>() else { continue };

            // Optimization: Skip system processes owned by other users or root (unless running as root)
            if my_uid != 0 {
                if let Ok(meta) = entry.metadata() {
                    if meta.uid() != my_uid {
                        continue;
                    }
                }
            }

            if let Some(name) = self.check_pid(pid) {
                *last_pid = Some(pid);
                return Some(name);
            }
        }

        None
    }

    pub(crate) fn detect(&self) -> Option<String> {
        let mut last_pid = None;
        self.detect_cached(&mut last_pid)
    }
}

pub(crate) struct CachedMatcher {
    matcher: Matcher,
    last_mtime: Option<SystemTime>,
    path: PathBuf,
}

impl CachedMatcher {
    pub(crate) fn new() -> Self {
        let path = games_path();
        let last_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
        Self {
            matcher: Matcher::load(),
            last_mtime,
            path,
        }
    }

    pub(crate) fn get(&mut self) -> &Matcher {
        let current_mtime = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if current_mtime != self.last_mtime {
            self.matcher = Matcher::load();
            self.last_mtime = current_mtime;
        }
        &self.matcher
    }
}

pub(crate) struct CachedConfig {
    config: Config,
    last_mtime: Option<SystemTime>,
    path: PathBuf,
}

impl CachedConfig {
    pub(crate) fn new() -> Self {
        let path = config_path();
        let last_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
        Self {
            config: Config::load(),
            last_mtime,
            path,
        }
    }

    pub(crate) fn get(&mut self) -> &Config {
        let current_mtime = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if current_mtime != self.last_mtime {
            self.config = Config::load();
            self.last_mtime = current_mtime;
        }
        &self.config
    }
}

fn base(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ---------------------------------------------------------------- sound

fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => home_dir().join(rest),
        None => PathBuf::from(s),
    }
}

/// Resolve a preset name or file path to a playable file. `none`/empty -> None.
fn resolve_sound(spec: &str) -> Option<PathBuf> {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((_, stem)) = PRESETS.iter().find(|(n, _)| *n == spec) {
        for dir in SOUND_DIRS {
            for ext in ["oga", "ogg", "wav"] {
                let p = Path::new(dir).join(format!("{stem}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        return None;
    }
    let p = expand_tilde(spec);
    p.is_file().then_some(p)
}

fn is_sound_file(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SOUND_EXTS.contains(&e.to_lowercase().as_str()))
}

fn sound_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| is_sound_file(p)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Built voice clips for a slot: (normal, rare, anomaly).
pub(crate) fn voice_clips(slot: &str) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let (mut normal, mut rare, mut anomaly) = (Vec::new(), Vec::new(), Vec::new());
    let prefix = format!("{slot}-");
    let egg_prefix = format!("{slot}-egg-");
    let anomaly_prefix = format!("{slot}-anomaly-");
    for p in sound_files_in(&voice_dir()) {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if name.starts_with(&anomaly_prefix) {
            anomaly.push(p);
        } else if name.starts_with(&egg_prefix) {
            rare.push(p);
        } else if name.starts_with(&prefix) {
            normal.push(p);
        }
    }
    (normal, rare, anomaly)
}

fn rand_u64() -> u64 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut x = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9E37_79B9_7F4A_7C15;
    for _ in 0..3 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
    }
    x
}

/// Shuffle-bag state persisted per slot + tier so that every line in a tier plays
/// before any line repeats!
fn shuffle_bag_pick(tier_name: &str, items: &[PathBuf]) -> Option<PathBuf> {
    if items.is_empty() {
        return None;
    }
    let bag_file = cache_dir().join(format!("bag-{tier_name}.txt"));
    let mut remaining: Vec<PathBuf> = fs::read_to_string(&bag_file)
        .unwrap_or_default()
        .lines()
        .map(PathBuf::from)
        .filter(|p| items.contains(p))
        .collect();

    if remaining.is_empty() {
        remaining = items.to_vec();
    }

    let idx = (rand_u64() % remaining.len() as u64) as usize;
    let chosen = remaining.swap_remove(idx);

    let _ = fs::create_dir_all(cache_dir());
    let remaining_str = remaining
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(&bag_file, remaining_str);

    Some(chosen)
}

/// Probability distribution: 80% standard, 18% rare, 2% anomaly.
fn pick_voice_line(slot: &str, normal: Vec<PathBuf>, rare: Vec<PathBuf>, anomaly: Vec<PathBuf>) -> Option<PathBuf> {
    let roll = rand_u64() % 100;
    if roll < 2 && !anomaly.is_empty() {
        shuffle_bag_pick(&format!("{slot}-anomaly"), &anomaly)
    } else if roll < 20 && !rare.is_empty() {
        shuffle_bag_pick(&format!("{slot}-rare"), &rare)
    } else if !normal.is_empty() {
        shuffle_bag_pick(&format!("{slot}-normal"), &normal)
    } else if !rare.is_empty() {
        shuffle_bag_pick(&format!("{slot}-rare"), &rare)
    } else if !anomaly.is_empty() {
        shuffle_bag_pick(&format!("{slot}-anomaly"), &anomaly)
    } else {
        None
    }
}

/// Signature physical cue path (chime+snap for ON, latch+sigh for OFF)
pub(crate) fn signature_cue_path(slot: &str) -> Option<PathBuf> {
    let p = data_dir().join(if slot == "on" { "on-cue.wav" } else { "off-cue.wav" });
    p.is_file().then_some(p)
}

/// What to play for a slot (`on`/`off`) given its config value: a preset, a file, a folder,
/// or `voice`.
fn pick_sound(slot: &str, spec: &str) -> Option<PathBuf> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("voice") {
        let (normal, rare, anomaly) = voice_clips(slot);
        if normal.is_empty() && rare.is_empty() && anomaly.is_empty() {
            if let Some(sig) = signature_cue_path(slot) {
                return Some(sig);
            }
            return resolve_sound(if slot == "on" { "pop" } else { "unpop" });
        }
        return pick_voice_line(slot, normal, rare, anomaly);
    }
    let dir = expand_tilde(spec);
    if !spec.is_empty() && dir.is_dir() {
        let files = sound_files_in(&dir);
        return shuffle_bag_pick(&format!("folder-{}", fnv32(&dir.to_string_lossy())), &files);
    }
    resolve_sound(spec)
}

/// Players tried in order; a player that is missing or exits non-zero falls through.
fn player_chain(file: &Path, volume: u8) -> Vec<(&'static str, Vec<String>)> {
    let f = file.to_string_lossy().to_string();
    let frac = format!("{:.2}", volume as f32 / 100.0);
    let pa = ((volume as u32) * 65536 / 100).to_string();
    vec![
        ("pw-play", vec!["--volume".into(), frac, f.clone()]),
        ("paplay", vec!["--volume".into(), pa, f.clone()]),
        ("ffplay", vec!["-nodisp".into(), "-autoexit".into(), "-loglevel".into(), "quiet".into(),
                        "-volume".into(), volume.to_string(), f.clone()]),
        ("aplay", vec!["-q".into(), f]),
    ]
}

/// Play on a helper thread (never blocks the poll loop; children are always reaped and
/// killed after 8 s). Returns a handle yielding whether any player succeeded.
fn play(file: PathBuf, volume: u8) -> JoinHandle<bool> {
    thread::spawn(move || {
        for (prog, args) in player_chain(&file, volume) {
            let Ok(mut child) = Command::new(prog)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            let deadline = Instant::now() + Duration::from_secs(8);
            let ok = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status.success(),
                    Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(40)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break false;
                    }
                }
            };
            if ok {
                return true;
            }
        }
        false
    })
}

// ---------------------------------------------------------------- notification

fn notify(enabled: bool, detail: Option<&str>, cfg: &Config) {
    if !cfg.notifications {
        return;
    }
    let (title, body, icon) = if enabled {
        let who = detail.map(|d| format!("{d} detected")).unwrap_or_else(|| "Manual switch".into());
        (
            "Game mode ON",
            format!("{who}\ncursor_follows_focus: ON (mouse stays with the game)"),
            cfg.icon_on.as_str(),
        )
    } else {
        (
            "Desktop mode",
            "No game running\ncursor_follows_focus: OFF (tabs and windows won't snap)".to_string(),
            cfg.icon_off.as_str(),
        )
    };
    // Same synchronous key => the new popup replaces the previous one instead of stacking.
    if let Ok(mut child) = Command::new("notify-send")
        .args(["-a", "Mousetrap", "-u", "low", "-t", "3500", "-i", icon])
        .args(["-h", "string:x-canonical-private-synchronous:cosmic-mousetrap"])
        .args([title, body.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

pub(crate) fn cue_sound(enabled: bool, cfg: &Config) -> Option<JoinHandle<bool>> {
    if !cfg.sound {
        return None;
    }
    let (slot, spec) = if enabled { ("on", &cfg.sound_on) } else { ("off", &cfg.sound_off) };
    let volume = cfg.volume;

    let is_voice = spec.eq_ignore_ascii_case("voice");
    let (voice_clip, maybe_signature) = if is_voice {
        let (normal, rare, anomaly) = voice_clips(slot);
        let has_voice = !normal.is_empty() || !rare.is_empty() || !anomaly.is_empty();
        let sig = signature_cue_path(slot);
        if has_voice {
            // Full voice mode: signature chime + GLaDOS speech line
            (pick_voice_line(slot, normal, rare, anomaly), sig)
        } else if sig.is_some() {
            // Fallback 1: signature cue is installed, play it alone cleanly
            (None, sig)
        } else {
            // Fallback 2: no voice clips or signature cue, use system desktop preset
            (resolve_sound(if slot == "on" { "pop" } else { "unpop" }), None)
        }
    } else {
        (pick_sound(slot, spec), None)
    };

    if maybe_signature.is_none() && voice_clip.is_none() {
        return None;
    }

    Some(thread::spawn(move || {
        let mut ok = true;
        if let Some(sig) = maybe_signature {
            let sig_ok = play(sig, (volume as u16 * 85 / 100) as u8).join().unwrap_or(false);
            ok = ok && sig_ok;
            if voice_clip.is_some() {
                thread::sleep(Duration::from_millis(60));
            }
        }
        if let Some(clip) = voice_clip {
            let clip_ok = play(clip, volume).join().unwrap_or(false);
            ok = ok && clip_ok;
        }
        ok
    }))
}

/// `wait`: block until the cue finishes (one-shot CLI commands, which would otherwise
/// exit and cut the sound off). The daemon passes false and never blocks.
fn set_and_report(enable: bool, quiet: bool, detail: Option<&str>, wait: bool) {
    let cfg = Config::load();
    set_and_report_with_cfg(enable, quiet, detail, wait, &cfg);
}

fn set_and_report_with_cfg(enable: bool, quiet: bool, detail: Option<&str>, wait: bool, cfg: &Config) {
    match write_state_atomic(enable) {
        Ok(()) => {
            eprintln!("[{APP}] cursor_follows_focus = {enable}");
            if !quiet {
                notify(enable, detail, cfg);
                if let Some(h) = cue_sound(enable, cfg) {
                    if wait {
                        let _ = h.join();
                    }
                }
            }
        }
        Err(e) => eprintln!("[{APP}] ERROR writing config: {e}"),
    }
}

// ---------------------------------------------------------------- daemon

fn run_daemon(quiet: bool) -> ! {
    eprintln!("[{APP}] v{VERSION} daemon started");
    // `applied` is the state we last wrote. ON is applied on the first poll that sees a
    // game; OFF needs EXIT_POLLS consecutive empty polls so launcher -> game process
    // handoffs (Steam/Proton) don't flap ON -> OFF -> ON.
    let mut applied: Option<bool> = None;
    let mut misses: u32 = 0;
    let mut last_game_pid: Option<u32> = None;
    let mut cached_matcher = CachedMatcher::new();
    let mut cached_config = CachedConfig::new();

    loop {
        let game = cached_matcher.get().detect_cached(&mut last_game_pid);
        let target = if game.is_some() {
            misses = 0;
            Some(true)
        } else {
            misses += 1;
            match applied {
                Some(true) if misses < EXIT_POLLS => None, // grace period: keep ON
                _ => Some(false),
            }
        };
        if let Some(want) = target {
            let needs_write = match applied {
                None => want != read_current_state(), // initial sync only if out of step
                Some(prev) => prev != want,
            };
            if needs_write {
                if let Some(g) = &game {
                    eprintln!("[{APP}] game detected: {g}");
                }
                let cfg = cached_config.get();
                set_and_report_with_cfg(want, quiet, game.as_deref(), false, cfg);
            }
            applied = Some(want);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

// ---------------------------------------------------------------- sound CLI

pub(crate) fn describe(slot: &str, spec: &str) -> String {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("voice") {
        let (normal, rare, anomaly) = voice_clips(slot);
        return if normal.is_empty() && rare.is_empty() && anomaly.is_empty() {
            if signature_cue_path(slot).is_some() {
                "voice (signature chimes ready; 106 GLaDOS lines: run `cosmic-mousetrap voice download`)".into()
            } else {
                "voice (no clips installed: run `cosmic-mousetrap voice download`; using preset for now)".into()
            }
        } else {
            format!("voice ({} standard + {} rare + {} anomaly)", normal.len(), rare.len(), anomaly.len())
        };
    }
    let dir = expand_tilde(spec);
    if !spec.is_empty() && dir.is_dir() {
        return format!("folder {} ({} files, random)", dir.display(), sound_files_in(&dir).len());
    }
    match resolve_sound(spec) {
        Some(p) if PRESETS.iter().any(|(n, _)| *n == spec) => format!("{spec} ({})", p.display()),
        Some(p) => p.display().to_string(),
        None if spec.is_empty() || spec.eq_ignore_ascii_case("none") => "none (silent)".into(),
        None => format!("{spec}  [NOT FOUND]"),
    }
}

pub(crate) fn test_slot(slot: &str, cfg: &Config) -> bool {
    let spec = if slot == "on" { &cfg.sound_on } else { &cfg.sound_off };
    print!("{slot}: testing cue ({}) ... ", describe(slot, spec));
    let _ = std::io::stdout().flush();
    let mut cfg_test = cfg.clone();
    cfg_test.sound = true;
    if let Some(h) = cue_sound(slot == "on", &cfg_test) {
        let ok = h.join().unwrap_or(false);
        println!("{}", if ok { "ok" } else { "FAILED (no working player: pw-play/paplay/ffplay/aplay)" });
        ok
    } else {
        println!("nothing to play");
        true
    }
}

fn slot_key(slot: &str) -> Option<&'static str> {
    match slot {
        "on" => Some("sound_on"),
        "off" => Some("sound_off"),
        _ => None,
    }
}

fn sound_cmd(args: &[&str]) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    match args {
        [] | ["show"] | ["status"] => {
            let c = Config::load();
            println!("config:   {}", config_path().display());
            println!("sound:    {}   volume: {}", if c.sound { "enabled" } else { "muted" }, c.volume);
            println!("ON cue:   {}", describe("on", &c.sound_on));
            println!("OFF cue:  {}", describe("off", &c.sound_off));
            Ok(())
        }
        ["list"] => {
            let c = Config::load();
            println!("Special values: `voice` (signature cues + GLaDOS lines, see `voice download`), a folder (random file), `none`.");
            println!("Presets (use with `sound set on|off <name>`):");
            for (name, _) in PRESETS {
                let mark = if resolve_sound(name).is_some() { " " } else { "!" };
                let used = match (c.sound_on == *name, c.sound_off == *name) {
                    (true, true) => "  <- ON, OFF",
                    (true, false) => "  <- ON",
                    (false, true) => "  <- OFF",
                    _ => "",
                };
                println!("  {mark} {name}{used}");
            }
            println!("  none   (silent)\n\n('!' = sound file missing on this system)");
            println!("Custom: `sound set on|off /path/to/file.{{{}}}`", SOUND_EXTS.join(","));
            Ok(())
        }
        ["test"] => {
            let c = Config::load();
            let a = test_slot("on", &c);
            let b = test_slot("off", &c);
            if a && b { Ok(()) } else { Err("sound playback failed".into()) }
        }
        ["test", slot @ ("on" | "off")] => {
            if test_slot(slot, &Config::load()) { Ok(()) } else { Err("sound playback failed".into()) }
        }
        ["set", slot, what] => {
            let key = slot_key(slot).ok_or("slot must be `on` or `off`")?;
            let folder = expand_tilde(what);
            let value: String = if what.eq_ignore_ascii_case("none")
                || what.eq_ignore_ascii_case("voice")
                || PRESETS.iter().any(|(n, _)| n == what)
            {
                what.to_ascii_lowercase().to_string()
            } else if folder.is_dir() {
                // A folder: a random file from it each time.
                if sound_files_in(&folder).is_empty() {
                    return Err(format!("no sound files ({}) in {}", SOUND_EXTS.join(", "), folder.display()));
                }
                folder.to_string_lossy().to_string()
            } else {
                // Custom file: validate, then copy into our own sounds dir
                let src = expand_tilde(what);
                let meta = fs::metadata(&src).map_err(|_| format!("no such file: {}", src.display()))?;
                if !meta.is_file() {
                    return Err(format!("not a file: {}", src.display()));
                }
                if meta.len() > MAX_SOUND_BYTES {
                    return Err(format!("file too large ({} bytes, max {})", meta.len(), MAX_SOUND_BYTES));
                }
                let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if !SOUND_EXTS.contains(&ext.as_str()) {
                    let shown = if ext.is_empty() { "(no extension)".to_string() } else { format!("`.{ext}`") };
                    return Err(format!("unsupported type {shown} (use: {})", SOUND_EXTS.join(", ")));
                }
                fs::create_dir_all(sounds_dir()).map_err(io)?;
                let dst = sounds_dir().join(format!("{slot}.{ext}"));
                fs::copy(&src, &dst).map_err(io)?;
                dst.to_string_lossy().to_string()
            };
            write_key(key, &value, true).map_err(io)?;
            println!("{slot} cue set to: {}", describe(slot, &value));
            test_slot(slot, &Config::load());
            Ok(())
        }
        ["volume", v] => {
            let n: u8 = v.parse().map_err(|_| "volume must be 0-100")?;
            if n > 100 {
                return Err("volume must be 0-100".into());
            }
            write_key("volume", &n.to_string(), false).map_err(io)?;
            println!("volume = {n}");
            Ok(())
        }
        ["enable"] | ["mute"] | ["disable"] | ["unmute"] => {
            let on = matches!(args[0], "enable" | "unmute");
            write_key("sound", &on.to_string(), false).map_err(io)?;
            println!("sound {}", if on { "enabled" } else { "muted" });
            Ok(())
        }
        ["reset"] => {
            let d = Config::default();
            write_key("sound_on", &d.sound_on, true).map_err(io)?;
            write_key("sound_off", &d.sound_off, true).map_err(io)?;
            write_key("volume", &d.volume.to_string(), false).map_err(io)?;
            write_key("sound", "true", false).map_err(io)?;
            println!("sound settings reset to defaults");
            Ok(())
        }
        _ => Err("usage: sound [show|list|test [on|off]|set <on|off> <preset|file>|volume <0-100>|mute|unmute|reset]".into()),
    }
}

// ---------------------------------------------------------------- voice

fn fnv32(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn venv_python() -> Option<PathBuf> {
    let p = venv_dir().join("bin/python");
    p.is_file().then_some(p)
}

const LINES_HEADER: &str = "\
# Mousetrap voice lines, one per line as `kind: text`.
#   on:    standard game launch announcement (80%)
#   on!:   rare game launch announcement (18%)
#   on!!:  ultra-rare character anomaly / existential breakdown (2%)
#   off:   standard game exit announcement (80%)
#   off!:  rare game exit announcement (18%)
#   off!!: ultra-rare character anomaly / existential breakdown (2%)
# Lines are picked via shuffle-bag (no repeats until pool is exhausted).
# After editing, run:  cosmic-mousetrap voice build
";

fn ensure_lines_file() -> std::io::Result<()> {
    if lines_path().is_file() {
        return Ok(());
    }
    fs::create_dir_all(app_config_dir())?;
    let mut text = String::from(LINES_HEADER);
    for (kind, line) in DEFAULT_LINES {
        text.push_str(&format!("{kind}: {line}\n"));
    }
    fs::write(lines_path(), text)
}

fn load_lines() -> Vec<(String, String)> {
    let text = fs::read_to_string(lines_path()).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(':'))
        .map(|(k, t)| (k.trim().to_lowercase(), t.trim().to_string()))
        .filter(|(k, t)| matches!(k.as_str(), "on" | "off" | "on!" | "off!" | "on!!" | "off!!") && !t.is_empty())
        .collect()
}

fn clip_name(kind: &str, text: &str, model: &str) -> String {
    let (slot, prefix) = if kind.ends_with("!!") {
        (kind.trim_end_matches('!'), "anomaly-")
    } else if kind.ends_with('!') {
        (kind.trim_end_matches('!'), "egg-")
    } else {
        (kind, "")
    };
    format!("{slot}-{prefix}{:08x}.wav", fnv32(&format!("{model}\u{0}{text}")))
}

fn run_checked(cmd: &mut Command, what: &str) -> Result<(), String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("{what} failed ({s})")),
        Err(e) => Err(format!("{what} could not start: {e}")),
    }
}

/// Download pre-rendered 106-clip GLaDOS voice pack from GitHub releases.
pub(crate) fn voice_download() -> Result<(), String> {
    let vdir = voice_dir();
    fs::create_dir_all(&vdir).map_err(|e| e.to_string())?;

    let local_pack = Path::new("cosmic-mousetrap-glados-voice.tar.gz");
    if local_pack.is_file() {
        println!("Extracting local voice pack: {}", local_pack.display());
        let res = Command::new("tar").args(["-xzf", "cosmic-mousetrap-glados-voice.tar.gz", "-C"]).arg(&vdir).status();
        if let Ok(s) = res {
            if s.success() {
                let (on, on_r, on_a) = voice_clips("on");
                let (off, off_r, off_a) = voice_clips("off");
                let total = on.len() + on_r.len() + on_a.len() + off.len() + off_r.len() + off_a.len();
                println!("Installed GLaDOS voice pack: {total} clips ready.");
                return Ok(());
            }
        }
    }

    let repo = env::var("COSMIC_MOUSETRAP_REPO").unwrap_or_else(|_| "Asiphyx/cosmic-mousetrap".to_string());
    let url = format!("https://github.com/{repo}/releases/latest/download/cosmic-mousetrap-glados-voice.tar.gz");
    println!("Downloading GLaDOS voice pack (106 lines) from GitHub Releases...");

    let gh_attempt = if Command::new("gh").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
        Command::new("sh")
            .arg("-c")
            .arg(format!("gh release download -R '{repo}' -p 'cosmic-mousetrap-glados-voice.tar.gz' -O - 2>/dev/null | tar -xz -C '{}'", vdir.display()))
            .status()
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gh not found"))
    };

    let status = match gh_attempt {
        Ok(s) if s.success() => Ok(s),
        _ => {
            if Command::new("curl").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
                Command::new("sh")
                    .arg("-c")
                    .arg(format!("curl -fsSL '{url}' | tar -xz -C '{}'", vdir.display()))
                    .status()
            } else if Command::new("wget").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
                Command::new("sh")
                    .arg("-c")
                    .arg(format!("wget -qO- '{url}' | tar -xz -C '{}'", vdir.display()))
                    .status()
            } else {
                return Err("neither gh, curl, nor wget is available".into());
            }
        }
    };

    match status {
        Ok(s) if s.success() => {
            let (on, on_r, on_a) = voice_clips("on");
            let (off, off_r, off_a) = voice_clips("off");
            let total = on.len() + on_r.len() + on_a.len() + off.len() + off_r.len() + off_a.len();
            println!("Installed GLaDOS voice pack: {total} clips ready.");
            Ok(())
        }
        Ok(s) => Err(format!("download/extract failed with exit code: {s}")),
        Err(e) => Err(format!("failed to run download command: {e}")),
    }
}

/// Generate any missing clips with Piper and delete clips whose lines were removed.
fn voice_build(cfg: &Config) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let py = venv_python().ok_or("Piper is not installed yet: run `cosmic-mousetrap voice setup`")?;
    ensure_lines_file().map_err(io)?;
    fs::create_dir_all(voice_dir()).map_err(io)?;
    let lines = load_lines();
    if lines.is_empty() {
        return Err(format!("no valid lines in {}", lines_path().display()));
    }
    let mut expected = Vec::new();
    let (mut made, mut kept) = (0, 0);
    for (kind, text) in &lines {
        let name = clip_name(kind, text, &cfg.voice_model);
        let out = voice_dir().join(&name);
        expected.push(name);
        if out.is_file() {
            kept += 1;
            continue;
        }
        println!("  [{kind}] {text}");
        run_checked(
            Command::new(&py)
                .args(["-m", "piper", "-m", &cfg.voice_model, "--data-dir"])
                .arg(models_dir())
                .arg("-f")
                .arg(&out)
                .args(["--", text])
                .stdout(Stdio::null()),
            "piper",
        )?;
        made += 1;
    }
    let mut removed = 0;
    for p in sound_files_in(&voice_dir()) {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let ours = (name.starts_with("on-") || name.starts_with("off-")) && name.ends_with(".wav");
        if ours && !expected.contains(&name) {
            let _ = fs::remove_file(&p);
            removed += 1;
        }
    }
    println!("voice clips: {made} generated, {kept} unchanged, {removed} stale removed");
    Ok(())
}

fn voice_setup(cfg: &Config) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    fs::create_dir_all(data_dir()).map_err(io)?;
    if venv_python().is_none() {
        println!("Creating a private Python environment in {} ...", venv_dir().display());
        run_checked(Command::new("python3").args(["-m", "venv"]).arg(venv_dir()), "python3 -m venv")?;
    }
    let py = venv_python().ok_or("venv creation did not produce a python")?;
    let have_piper = Command::new(&py)
        .args(["-c", "import piper"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !have_piper {
        println!("Installing piper-tts (one time) ...");
        run_checked(Command::new(&py).args(["-m", "pip", "install", "--quiet", "piper-tts"]), "pip install piper-tts")?;
    }
    fs::create_dir_all(models_dir()).map_err(io)?;
    let model = models_dir().join(format!("{}.onnx", cfg.voice_model));
    let model_json = models_dir().join(format!("{}.onnx.json", cfg.voice_model));
    if !(model.is_file() && model_json.is_file()) {
        let _ = fs::remove_file(&model);
        let _ = fs::remove_file(&model_json);
        println!("Downloading voice {} (one time, ~60 MB) ...", cfg.voice_model);
        run_checked(
            Command::new(&py)
                .args(["-m", "piper.download_voices", "--data-dir"])
                .arg(models_dir())
                .arg(&cfg.voice_model),
            "voice download",
        )?;
    }
    println!("Generating clips ...");
    voice_build(cfg)?;
    write_key("sound_on", "voice", true).map_err(io)?;
    write_key("sound_off", "voice", true).map_err(io)?;
    println!("\nDone. ON/OFF cues now use the voice lines. Hear them: cosmic-mousetrap sound test");
    Ok(())
}

fn voice_cmd(args: &[&str]) -> Result<(), String> {
    let cfg = Config::load();
    match args {
        ["setup"] => voice_setup(&cfg),
        ["download"] | ["install"] => voice_download(),
        ["build"] => voice_build(&cfg),
        ["lines"] => {
            ensure_lines_file().map_err(|e| e.to_string())?;
            println!("lines file: {}  (edit, then run `cosmic-mousetrap voice build`)\n", lines_path().display());
            for (k, t) in load_lines() {
                println!("  {k:<5} {t}");
            }
            Ok(())
        }
        [] | ["status"] => {
            let (on, on_rare, on_anomaly) = voice_clips("on");
            let (off, off_rare, off_anomaly) = voice_clips("off");
            let total = on.len() + on_rare.len() + on_anomaly.len() + off.len() + off_rare.len() + off_anomaly.len();
            println!("voice:  Portal 2 GLaDOS");
            if total == 0 {
                println!("clips:  None installed (run `cosmic-mousetrap voice download` or `voice setup`)");
            } else {
                println!("clips:  ON {} standard (+{} rare, +{} anomaly) | OFF {} standard (+{} rare, +{} anomaly)",
                         on.len(), on_rare.len(), on_anomaly.len(), off.len(), off_rare.len(), off_anomaly.len());
            }
            println!("lines:  {}", lines_path().display());
            Ok(())
        }
        _ => Err("usage: voice [status|download|setup|build|lines]".into()),
    }
}

// ---------------------------------------------------------------- main

const USAGE: &str = "\
Mousetrap: auto-toggle COSMIC cursor_follows_focus while games run

usage: cosmic-mousetrap <command> [--quiet]

  status                              show setting, detected game, sound/notification config
  ui | tui | menu                     interactive visual control center with clickable settings
  doctor [--heal]                     diagnose and self-heal system, config, service, and audio issues
  daemon                              run the watcher (this is what the service does)
  on | off | toggle                   switch manually
  sound ...                           notification sounds (see `cosmic-mousetrap sound list`)
  voice status|download|setup|build|lines   voice cues (download GLaDOS pack or setup Piper TTS)
  notifications enable|disable
  --version, --help";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let quiet = args.iter().any(|a| a == "--quiet" || a == "-q");
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{APP} {VERSION}");
        return;
    }
    if args.is_empty() {
        let c = Config::load();
        println!("cursor_follows_focus: {}", read_current_state());
        println!("game detected:        {}", Matcher::load().detect().unwrap_or_else(|| "none".into()));
        println!("notifications:        {}", if c.notifications { "enabled" } else { "disabled" });
        println!("sound:                {} (volume {}, ON: {}, OFF: {})",
                 if c.sound { "enabled" } else { "muted" }, c.volume, c.sound_on, c.sound_off);
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") && (args.len() == 1 || !matches!(args[0].as_str(), "doctor" | "ui" | "tui" | "menu" | "sound" | "voice")) {
        println!("{USAGE}");
        return;
    }

    let first = args[0].as_str();
    let sub_args: Vec<&str> = args[1..].iter().filter(|a| !a.starts_with("--quiet") && !a.starts_with("-q")).map(String::as_str).collect();

    match first {
        "status" => {
            let c = Config::load();
            println!("cursor_follows_focus: {}", read_current_state());
            println!("game detected:        {}", Matcher::load().detect().unwrap_or_else(|| "none".into()));
            println!("notifications:        {}", if c.notifications { "enabled" } else { "disabled" });
            println!("sound:                {} (volume {}, ON: {}, OFF: {})",
                     if c.sound { "enabled" } else { "muted" }, c.volume, c.sound_on, c.sound_off);
        }
        "ui" | "tui" | "menu" => {
            if sub_args.iter().any(|a| *a == "--help" || *a == "-h") {
                println!("cosmic-mousetrap ui\n\nInteractive visual control center with clickable settings.\n\nUsage: cosmic-mousetrap ui");
                return;
            }
            if let Err(e) = ui::run_ui() {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        "doctor" => {
            if let Err(e) = doctor::doctor_cmd(&sub_args) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        "on" => set_and_report(true, quiet, None, true),
        "off" => set_and_report(false, quiet, None, true),
        "toggle" => set_and_report(!read_current_state(), quiet, None, true),
        "daemon" => run_daemon(quiet),
        "sound" => {
            if let Err(e) = sound_cmd(&sub_args) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        "voice" => {
            if let Err(e) = voice_cmd(&sub_args) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        "notifications" => {
            match sub_args.first().copied() {
                Some("enable") => {
                    let _ = write_key("notifications", "true", false);
                    println!("notifications enabled");
                }
                Some("disable") => {
                    let _ = write_key("notifications", "false", false);
                    println!("notifications disabled");
                }
                _ => {
                    eprintln!("usage: cosmic-mousetrap notifications enable|disable");
                    std::process::exit(2);
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("ON"), Some(true));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("OFF"), Some(false));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn test_fnv32() {
        assert_eq!(fnv32(""), 0x811c_9dc5);
        assert_eq!(fnv32("hello"), 0x4f9f2cab);
    }

    #[test]
    fn test_clip_name() {
        let c_normal = clip_name("on", "hello", "glados");
        assert!(c_normal.starts_with("on-"));
        assert!(!c_normal.starts_with("on-egg-"));
        assert!(!c_normal.starts_with("on-anomaly-"));

        let c_egg = clip_name("on!", "hello", "glados");
        assert!(c_egg.starts_with("on-egg-"));

        let c_anomaly = clip_name("on!!", "hello", "glados");
        assert!(c_anomaly.starts_with("on-anomaly-"));
    }

    #[test]
    fn test_base_path() {
        assert_eq!(base("/usr/bin/retroarch"), "retroarch");
        assert_eq!(base("retroarch"), "retroarch");
        assert_eq!(base("/home/user/games/mygame.x86_64"), "mygame.x86_64");
    }

    #[test]
    fn test_matcher() {
        let matcher = Matcher {
            names: vec!["wineserver".into(), "retroarch".into()],
            markers: vec!["steamapps/common".into()],
        };
        assert!(matcher.name_matches("wineserver"));
        assert!(matcher.name_matches("retroarch"));
        assert!(!matcher.name_matches("bash"));
        assert!(matcher.path_matches("/media/drive/steamapps/common/doom/doom.exe"));
        assert!(!matcher.path_matches("/usr/bin/ls"));
    }

    #[test]
    fn test_expand_tilde() {
        let old_home = env::var_os("HOME");
        env::set_var("HOME", "/tmp/fakehome");
        assert_eq!(expand_tilde("~/test.wav"), PathBuf::from("/tmp/fakehome/test.wav"));
        assert_eq!(expand_tilde("/var/log"), PathBuf::from("/var/log"));
        if let Some(h) = old_home {
            env::set_var("HOME", h);
        } else {
            env::remove_var("HOME");
        }
    }

    #[test]
    fn test_config_defaults() {
        let c = Config::default();
        assert!(c.notifications);
        assert!(c.sound);
        assert_eq!(c.volume, 60);
        assert_eq!(c.sound_on, "voice");
        assert_eq!(c.sound_off, "voice");
    }

    #[test]
    fn test_default_lines_count() {
        assert_eq!(DEFAULT_LINES.len(), 106);
    }

    #[test]
    fn test_slot_key() {
        assert_eq!(slot_key("on"), Some("sound_on"));
        assert_eq!(slot_key("off"), Some("sound_off"));
        assert_eq!(slot_key("other"), None);
    }

    #[test]
    fn test_resolve_sound_none() {
        assert_eq!(resolve_sound("none"), None);
        assert_eq!(resolve_sound(""), None);
        assert_eq!(resolve_sound("   "), None);
    }

    #[test]
    fn test_matcher_check_pid() {
        let my_pid = std::process::id();
        // Current process is the test runner (e.g. cosmic_mousetrap-...)
        let comm = fs::read_to_string(format!("/proc/{my_pid}/comm")).unwrap_or_default();
        let comm_trimmed = comm.trim().to_lowercase();

        let matcher = Matcher {
            names: vec![comm_trimmed.clone()],
            markers: vec![],
        };
        assert_eq!(matcher.check_pid(my_pid), Some(comm_trimmed));

        let non_matching = Matcher {
            names: vec!["definitely_not_running_xyz".into()],
            markers: vec![],
        };
        assert_eq!(non_matching.check_pid(my_pid), None);
    }

    #[test]
    fn test_detect_cached_fast_path() {
        let my_pid = std::process::id();
        let comm = fs::read_to_string(format!("/proc/{my_pid}/comm")).unwrap_or_default();
        let comm_trimmed = comm.trim().to_lowercase();

        let matcher = Matcher {
            names: vec![comm_trimmed.clone()],
            markers: vec![],
        };
        let mut last_pid = Some(my_pid);
        let res = matcher.detect_cached(&mut last_pid);
        assert_eq!(res, Some(comm_trimmed));
        assert_eq!(last_pid, Some(my_pid));
    }

    #[test]
    fn test_cached_config_load() {
        let mut cached = CachedConfig::new();
        let cfg1 = cached.get().clone();
        assert_eq!(cfg1.volume, Config::load().volume);
    }

    #[test]
    fn test_cached_matcher_load() {
        let mut cached = CachedMatcher::new();
        let m = cached.get();
        assert!(m.name_matches("wineserver"));
    }

    #[test]
    fn test_doctor_report_status() {
        let report = doctor::DoctorReport {
            checks: vec![
                doctor::CheckResult {
                    name: "Check 1".into(),
                    status: doctor::CheckStatus::Ok,
                    message: "All good".into(),
                    healed: false,
                },
                doctor::CheckResult {
                    name: "Check 2".into(),
                    status: doctor::CheckStatus::Ok,
                    message: "Fixed".into(),
                    healed: true,
                },
                doctor::CheckResult {
                    name: "Check 3".into(),
                    status: doctor::CheckStatus::Warning,
                    message: "Non-critical".into(),
                    healed: false,
                },
            ],
        };
        assert_eq!(report.ok_count(), 2);
        assert_eq!(report.healed_count(), 1);
        assert!(!report.has_errors());
    }
}
