//! Visual TUI for COSMIC Mousetrap: interactive control center with clickable settings
//! and live system status. Pure Rust standard library, zero external crate dependencies.

use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::*;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
struct WinSize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
}

static TERMINAL_RESTORED: AtomicBool = AtomicBool::new(false);
static SAVED_STTY: Mutex<Option<String>> = Mutex::new(None);

extern "C" fn sig_cleanup(_: i32) {
    restore_terminal();
    std::process::exit(0);
}

fn restore_terminal() {
    if !TERMINAL_RESTORED.swap(true, Ordering::SeqCst) {
        // Disable mouse tracking (SGR 1006 & normal 1000), show cursor, exit alternate screen
        let _ = io::stdout().write_all(b"\x1b[?1000l\x1b[?1006l\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
        if let Ok(guard) = SAVED_STTY.lock() {
            if let Some(ref s) = *guard {
                let _ = Command::new("stty").arg(s).status();
                return;
            }
        }
        let _ = Command::new("stty").args(["-raw", "echo"]).status();
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        let is_tty = io::stdin().is_terminal();
        if !is_tty {
            return Err("interactive UI requires a terminal (tty)".into());
        }

        let saved = Command::new("stty")
            .arg("-g")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        if let Ok(mut guard) = SAVED_STTY.lock() {
            *guard = saved;
        }

        // Put terminal in raw mode: disable canonical input & echo
        let _ = Command::new("stty").args(["raw", "-echo"]).status();

        TERMINAL_RESTORED.store(false, Ordering::SeqCst);

        // Register signal handlers for clean restoration
        unsafe {
            signal(2, sig_cleanup);  // SIGINT
            signal(15, sig_cleanup); // SIGTERM
            signal(1, sig_cleanup);  // SIGHUP
        }

        // Set panic hook to restore cooked mode before panic prints
        let old_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            old_hook(info);
        }));

        // Alternate screen, hide cursor, enable mouse tracking + SGR 1006 extended mode
        print!("\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[2J\x1b[H");
        let _ = io::stdout().flush();

        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    ToggleState,
    ServiceStart,
    ServiceStop,
    ServiceRestart,
    ToggleSound,
    ToggleNotify,
    VolDown,
    VolUp,
    OnCuePrev,
    OnCueNext,
    TestOn,
    OffCuePrev,
    OffCueNext,
    TestOff,
    VoiceDownload,
    RunDoctor,
    Quit,
}

pub const BUTTON_ORDER: &[ButtonId] = &[
    ButtonId::ToggleState,
    ButtonId::ServiceStart,
    ButtonId::ServiceStop,
    ButtonId::ServiceRestart,
    ButtonId::ToggleSound,
    ButtonId::ToggleNotify,
    ButtonId::VolDown,
    ButtonId::VolUp,
    ButtonId::OnCuePrev,
    ButtonId::OnCueNext,
    ButtonId::TestOn,
    ButtonId::OffCuePrev,
    ButtonId::OffCueNext,
    ButtonId::TestOff,
    ButtonId::VoiceDownload,
    ButtonId::RunDoctor,
    ButtonId::Quit,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonRect {
    pub id: ButtonId,
    pub row: u16,
    pub col_start: u16,
    pub col_end: u16,
}

const PRESET_CYCLE: &[&str] = &[
    "voice", "pop", "unpop", "chime", "ping", "bell", "attention",
    "power-up", "power-down", "login", "logout", "camera", "alarm", "none",
];

fn cycle_cue(current: &str, forward: bool) -> String {
    let curr = current.trim().to_lowercase();
    let idx = PRESET_CYCLE.iter().position(|&p| p == curr);
    match idx {
        Some(i) => {
            if forward {
                PRESET_CYCLE[(i + 1) % PRESET_CYCLE.len()].to_string()
            } else {
                PRESET_CYCLE[if i == 0 { PRESET_CYCLE.len() - 1 } else { i - 1 }].to_string()
            }
        }
        None => "voice".to_string(),
    }
}

/// Instantaneous terminal size via single ioctl (zero subprocess forks).
fn get_terminal_size() -> (u16, u16) {
    let mut ws = WinSize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let ret = unsafe { ioctl(1, 0x5413, &mut ws) };
    if ret == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
        return (ws.ws_row, ws.ws_col);
    }
    let rows = env::var("LINES").ok().and_then(|s| s.parse().ok()).unwrap_or(24);
    let cols = env::var("COLUMNS").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    (rows, cols)
}

fn get_service_status() -> String {
    let out = Command::new("systemctl")
        .args(["--user", "is-active", "cosmic-mousetrap"])
        .output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "inactive".into()
            } else {
                s
            }
        }
        Err(_) => "not available".into(),
    }
}

enum InputEvent {
    Key(KeyEvent),
    MouseClick { col: u16, row: u16 },
    MouseScrollUp { row: u16 },
    MouseScrollDown { row: u16 },
}

enum KeyEvent {
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
    Enter,
    Space,
    Esc,
    Quit,
    Char(char),
}

fn read_input(timeout_ms: i32) -> Option<InputEvent> {
    let mut pfd = PollFd { fd: 0, events: 1, revents: 0 };
    let ret = unsafe { poll(&mut pfd, 1, timeout_ms) };
    if ret <= 0 {
        return None;
    }

    let mut buf = [0u8; 128];
    let n = io::stdin().read(&mut buf).ok()?;
    if n == 0 {
        // EOF encountered on stdin: terminate UI to avoid 100% CPU infinite loop
        return Some(InputEvent::Key(KeyEvent::Quit));
    }

    let slice = &buf[..n];

    // Single byte checks
    if n == 1 {
        match slice[0] {
            b'q' | b'Q' | 3 => return Some(InputEvent::Key(KeyEvent::Quit)), // 3 = Ctrl+C
            b'\r' | b'\n' => return Some(InputEvent::Key(KeyEvent::Enter)),
            b' ' => return Some(InputEvent::Key(KeyEvent::Space)),
            b'\t' => return Some(InputEvent::Key(KeyEvent::Tab)),
            27 => return Some(InputEvent::Key(KeyEvent::Esc)),
            b's' | b'S' => return Some(InputEvent::Key(KeyEvent::Char('s'))),
            b'm' | b'M' => return Some(InputEvent::Key(KeyEvent::Char('m'))),
            b'+' | b'=' => return Some(InputEvent::Key(KeyEvent::Char('+'))),
            b'-' | b'_' => return Some(InputEvent::Key(KeyEvent::Char('-'))),
            b't' | b'T' => return Some(InputEvent::Key(KeyEvent::Char('t'))),
            b'y' | b'Y' => return Some(InputEvent::Key(KeyEvent::Char('y'))),
            b'n' | b'N' => return Some(InputEvent::Key(KeyEvent::Char('n'))),
            b'v' | b'V' => return Some(InputEvent::Key(KeyEvent::Char('v'))),
            b'd' | b'D' => return Some(InputEvent::Key(KeyEvent::Char('d'))),
            b'r' | b'R' => return Some(InputEvent::Key(KeyEvent::Char('r'))),
            _ => return None,
        }
    }

    // Escape sequences
    if slice.starts_with(b"\x1b[<") {
        // SGR mouse sequence: \x1b[<btn;col;row(M|m)
        let end_idx = slice.iter().position(|&b| b == b'M' || b == b'm')?;
        let is_press = slice[end_idx] == b'M';
        let body = std::str::from_utf8(&slice[3..end_idx]).ok()?;
        let mut parts = body.split(';');
        let btn: u32 = parts.next()?.parse().ok()?;
        let col: u16 = parts.next()?.parse().ok()?;
        let row: u16 = parts.next()?.parse().ok()?;

        if btn == 64 {
            return Some(InputEvent::MouseScrollUp { row });
        } else if btn == 65 {
            return Some(InputEvent::MouseScrollDown { row });
        } else if (btn & 0b100011) == 0 && is_press {
            return Some(InputEvent::MouseClick { col, row });
        }
        return None;
    }

    // Standard arrow keys and application cursor mode
    if slice == b"\x1b[A" || slice == b"\x1bOA" {
        return Some(InputEvent::Key(KeyEvent::Up));
    } else if slice == b"\x1b[B" || slice == b"\x1bOB" {
        return Some(InputEvent::Key(KeyEvent::Down));
    } else if slice == b"\x1b[C" || slice == b"\x1bOC" {
        return Some(InputEvent::Key(KeyEvent::Right));
    } else if slice == b"\x1b[D" || slice == b"\x1bOD" {
        return Some(InputEvent::Key(KeyEvent::Left));
    } else if slice == b"\x1b[Z" {
        return Some(InputEvent::Key(KeyEvent::BackTab));
    }

    None
}

pub struct UiLayout {
    pub buttons: Vec<ButtonRect>,
    pub vol_row: u16,
    pub on_cue_row: u16,
    pub off_cue_row: u16,
    pub vol_bar_start: u16,
    pub vol_bar_end: u16,
}

pub fn run_ui() -> Result<(), String> {
    let _guard = TerminalGuard::enter()?;

    let mut focused_btn = ButtonId::ToggleState;
    let mut status_msg = String::from("Ready. Click any button or use Tab/Enter/Hotkeys.");
    let mut doctor_summary = String::from("Doctor ready. Click 'Heal Doctor' to check.");
    let mut last_game_poll = Instant::now() - Duration::from_secs(10);
    let mut last_svc_poll = Instant::now() - Duration::from_secs(10);
    let mut game_detected = String::from("none");
    let mut svc_status = String::from("inactive");
    let mut last_pid = None;
    let mut cached_matcher = CachedMatcher::new();
    let mut cached_config = CachedConfig::new();

    let mut prev_render_key: Option<String> = None;

    loop {
        let now = Instant::now();

        // 1. Throttled game scan (every 1s) with last_game_pid fast path
        if now.duration_since(last_game_poll) >= Duration::from_millis(1000) {
            last_game_poll = now;
            let found = cached_matcher.get().detect_cached(&mut last_pid);
            game_detected = found.unwrap_or_else(|| "none".into());
        }

        // 2. Throttled service state poll (every 2.5s) to avoid subprocess churn
        if now.duration_since(last_svc_poll) >= Duration::from_millis(2500) {
            last_svc_poll = now;
            svc_status = get_service_status();
        }

        let cfg = cached_config.get().clone();
        let curr_state = read_current_state();
        let (_term_rows, term_cols) = get_terminal_size();

        // 3. Render frame with exact layout
        let state = UiState {
            term_cols,
            curr_state,
            game: &game_detected,
            svc_status: &svc_status,
            cfg: &cfg,
            doctor_summary: &doctor_summary,
            status_msg: &status_msg,
            focused: focused_btn,
        };

        let mut layout = UiLayout {
            buttons: Vec::new(),
            vol_row: 0,
            on_cue_row: 0,
            off_cue_row: 0,
            vol_bar_start: 0,
            vol_bar_end: 0,
        };

        let frame = render_frame(&state, &mut layout);

        // Optimization: Only write to terminal if rendered frame or status changed
        let render_key = format!("{}:{}:{}:{}:{}:{}", curr_state, game_detected, svc_status, cfg.volume, focused_btn as u8, status_msg);
        if prev_render_key.as_deref() != Some(&render_key) {
            print!("\x1b[H{frame}");
            let _ = io::stdout().flush();
            prev_render_key = Some(render_key);
        }

        // Wait for input with 200ms timeout
        let event = read_input(200);
        let Some(ev) = event else { continue };

        let mut trigger_action: Option<ButtonId> = None;

        match ev {
            InputEvent::Key(KeyEvent::Quit) | InputEvent::Key(KeyEvent::Esc) => break,
            InputEvent::Key(KeyEvent::Tab) | InputEvent::Key(KeyEvent::Right) | InputEvent::Key(KeyEvent::Down) => {
                let curr_idx = BUTTON_ORDER.iter().position(|&b| b == focused_btn).unwrap_or(0);
                focused_btn = BUTTON_ORDER[(curr_idx + 1) % BUTTON_ORDER.len()];
                prev_render_key = None;
            }
            InputEvent::Key(KeyEvent::BackTab) | InputEvent::Key(KeyEvent::Left) | InputEvent::Key(KeyEvent::Up) => {
                let curr_idx = BUTTON_ORDER.iter().position(|&b| b == focused_btn).unwrap_or(0);
                focused_btn = BUTTON_ORDER[if curr_idx == 0 { BUTTON_ORDER.len() - 1 } else { curr_idx - 1 }];
                prev_render_key = None;
            }
            InputEvent::Key(KeyEvent::Enter) | InputEvent::Key(KeyEvent::Space) => {
                trigger_action = Some(focused_btn);
            }
            InputEvent::Key(KeyEvent::Char(c)) => match c {
                's' => trigger_action = Some(ButtonId::ToggleState),
                'm' => trigger_action = Some(ButtonId::ToggleSound),
                '+' => trigger_action = Some(ButtonId::VolUp),
                '-' => trigger_action = Some(ButtonId::VolDown),
                't' => trigger_action = Some(ButtonId::TestOn),
                'y' => trigger_action = Some(ButtonId::TestOff),
                'n' => trigger_action = Some(ButtonId::ToggleNotify),
                'v' => trigger_action = Some(ButtonId::VoiceDownload),
                'd' => trigger_action = Some(ButtonId::RunDoctor),
                'r' => trigger_action = Some(ButtonId::ServiceRestart),
                _ => {}
            },
            InputEvent::MouseClick { col, row } => {
                // Check if volume bar was clicked directly
                if row == layout.vol_row && col >= layout.vol_bar_start && col <= layout.vol_bar_end {
                    let bar_w = layout.vol_bar_end.saturating_sub(layout.vol_bar_start).max(1);
                    let offset = col.saturating_sub(layout.vol_bar_start);
                    let new_vol = ((offset as u32 * 100) / bar_w as u32).min(100) as u8;
                    let _ = write_key("volume", &new_vol.to_string(), false);
                    status_msg = format!("Volume set to {new_vol}%");
                    prev_render_key = None;
                } else if let Some(id) = find_button_at(col, row, &layout.buttons) {
                    focused_btn = id;
                    trigger_action = Some(id);
                }
            }
            InputEvent::MouseScrollUp { row } => {
                if row == layout.vol_row {
                    trigger_action = Some(ButtonId::VolUp);
                } else if row == layout.on_cue_row {
                    trigger_action = Some(ButtonId::OnCueNext);
                } else if row == layout.off_cue_row {
                    trigger_action = Some(ButtonId::OffCueNext);
                }
            }
            InputEvent::MouseScrollDown { row } => {
                if row == layout.vol_row {
                    trigger_action = Some(ButtonId::VolDown);
                } else if row == layout.on_cue_row {
                    trigger_action = Some(ButtonId::OnCuePrev);
                } else if row == layout.off_cue_row {
                    trigger_action = Some(ButtonId::OffCuePrev);
                }
            }
        }

        // Execute action if triggered
        if let Some(act) = trigger_action {
            prev_render_key = None;
            match act {
                ButtonId::ToggleState => {
                    let next = !curr_state;
                    match write_state_atomic(next) {
                        Ok(()) => status_msg = format!("Toggled: cursor_follows_focus = {next}"),
                        Err(e) => status_msg = format!("Error toggling state: {e}"),
                    }
                }
                ButtonId::ServiceStart => {
                    let ok = Command::new("systemctl").args(["--user", "start", "cosmic-mousetrap"]).status().map(|s| s.success()).unwrap_or(false);
                    svc_status = get_service_status();
                    last_svc_poll = Instant::now();
                    status_msg = if ok { "Systemd service started.".into() } else { "Failed to start service.".into() };
                }
                ButtonId::ServiceStop => {
                    let ok = Command::new("systemctl").args(["--user", "stop", "cosmic-mousetrap"]).status().map(|s| s.success()).unwrap_or(false);
                    svc_status = get_service_status();
                    last_svc_poll = Instant::now();
                    status_msg = if ok { "Systemd service stopped.".into() } else { "Failed to stop service.".into() };
                }
                ButtonId::ServiceRestart => {
                    let ok = Command::new("systemctl").args(["--user", "restart", "cosmic-mousetrap"]).status().map(|s| s.success()).unwrap_or(false);
                    svc_status = get_service_status();
                    last_svc_poll = Instant::now();
                    status_msg = if ok { "Systemd service restarted.".into() } else { "Failed to restart service.".into() };
                }
                ButtonId::ToggleSound => {
                    let next = !cfg.sound;
                    let _ = write_key("sound", &next.to_string(), false);
                    status_msg = format!("Sound cues {}", if next { "enabled" } else { "muted" });
                }
                ButtonId::VolDown => {
                    let next = cfg.volume.saturating_sub(5);
                    let _ = write_key("volume", &next.to_string(), false);
                    status_msg = format!("Volume set to {next}%");
                }
                ButtonId::VolUp => {
                    let next = (cfg.volume + 5).min(100);
                    let _ = write_key("volume", &next.to_string(), false);
                    status_msg = format!("Volume set to {next}%");
                }
                ButtonId::OnCuePrev => {
                    let next = cycle_cue(&cfg.sound_on, false);
                    let _ = write_key("sound_on", &next, true);
                    status_msg = format!("ON cue set to: {next}");
                }
                ButtonId::OnCueNext => {
                    let next = cycle_cue(&cfg.sound_on, true);
                    let _ = write_key("sound_on", &next, true);
                    status_msg = format!("ON cue set to: {next}");
                }
                ButtonId::TestOn => {
                    status_msg = "Playing ON cue...".into();
                    let mut test_cfg = cfg.clone();
                    test_cfg.sound = true;
                    // cue_sound spawns a background thread; drop the handle to detach cleanly
                    let _ = cue_sound(true, &test_cfg);
                }
                ButtonId::OffCuePrev => {
                    let next = cycle_cue(&cfg.sound_off, false);
                    let _ = write_key("sound_off", &next, true);
                    status_msg = format!("OFF cue set to: {next}");
                }
                ButtonId::OffCueNext => {
                    let next = cycle_cue(&cfg.sound_off, true);
                    let _ = write_key("sound_off", &next, true);
                    status_msg = format!("OFF cue set to: {next}");
                }
                ButtonId::TestOff => {
                    status_msg = "Playing OFF cue...".into();
                    let mut test_cfg = cfg.clone();
                    test_cfg.sound = true;
                    let _ = cue_sound(false, &test_cfg);
                }
                ButtonId::ToggleNotify => {
                    let next = !cfg.notifications;
                    let _ = write_key("notifications", &next.to_string(), false);
                    status_msg = format!("Notifications {}", if next { "enabled" } else { "disabled" });
                }
                ButtonId::VoiceDownload => {
                    status_msg = "Downloading GLaDOS voice pack... (please wait)".into();
                    let state = UiState {
                        term_cols,
                        curr_state,
                        game: &game_detected,
                        svc_status: &svc_status,
                        cfg: &cfg,
                        doctor_summary: &doctor_summary,
                        status_msg: &status_msg,
                        focused: focused_btn,
                    };
                    let mut dummy_layout = UiLayout {
                        buttons: Vec::new(),
                        vol_row: 0,
                        on_cue_row: 0,
                        off_cue_row: 0,
                        vol_bar_start: 0,
                        vol_bar_end: 0,
                    };
                    print!("\x1b[2J\x1b[H{}", render_frame(&state, &mut dummy_layout));
                    let _ = io::stdout().flush();
                    match voice_download() {
                        Ok(()) => status_msg = "Voice pack installed! 106 clips ready.".into(),
                        Err(e) => status_msg = format!("Voice download failed: {e}"),
                    }
                    print!("\x1b[2J"); // Clear after potential subshell output
                }
                ButtonId::RunDoctor => {
                    status_msg = "Running self-healing doctor...".into();
                    let state = UiState {
                        term_cols,
                        curr_state,
                        game: &game_detected,
                        svc_status: &svc_status,
                        cfg: &cfg,
                        doctor_summary: &doctor_summary,
                        status_msg: &status_msg,
                        focused: focused_btn,
                    };
                    let mut dummy_layout = UiLayout {
                        buttons: Vec::new(),
                        vol_row: 0,
                        on_cue_row: 0,
                        off_cue_row: 0,
                        vol_bar_start: 0,
                        vol_bar_end: 0,
                    };
                    print!("\x1b[2J\x1b[H{}", render_frame(&state, &mut dummy_layout));
                    let _ = io::stdout().flush();
                    let report = doctor::run_doctor(true);
                    let ok = report.ok_count();
                    let total = report.checks.len();
                    let healed = report.healed_count();
                    doctor_summary = format!("{ok}/{total} OK ({healed} healed)");
                    status_msg = if report.has_errors() {
                        format!("Doctor: {ok}/{total} checks passed. Some issues remain.")
                    } else if healed > 0 {
                        format!("Doctor: {ok}/{total} checks passed. Auto-healed {healed} issue(s)!")
                    } else {
                        format!("Doctor: {ok}/{total} checks passed. All systems healthy!")
                    };
                    print!("\x1b[2J"); // Clear after doctor run
                }
                ButtonId::Quit => break,
            }
        }
    }

    Ok(())
}

pub fn find_button_at(col: u16, row: u16, buttons: &[ButtonRect]) -> Option<ButtonId> {
    buttons
        .iter()
        .find(|b| b.row == row && col >= b.col_start && col <= b.col_end)
        .map(|b| b.id)
}

pub struct UiState<'a> {
    pub term_cols: u16,
    pub curr_state: bool,
    pub game: &'a str,
    pub svc_status: &'a str,
    pub cfg: &'a Config,
    pub doctor_summary: &'a str,
    pub status_msg: &'a str,
    pub focused: ButtonId,
}

/// A line builder that tracks visible terminal character width separately from ANSI escape sequences,
/// guaranteeing that right borders align vertically and button click coordinates match 100%.
struct LineBuilder<'a> {
    row: u16,
    pad_left_len: u16,
    inner_width: usize,
    visible_len: usize,
    output: String,
    layout: &'a mut UiLayout,
    focused: ButtonId,
}

impl<'a> LineBuilder<'a> {
    fn new(row: u16, pad_left_len: u16, inner_width: usize, layout: &'a mut UiLayout, focused: ButtonId) -> Self {
        Self {
            row,
            pad_left_len,
            inner_width,
            visible_len: 0,
            output: String::with_capacity(256),
            layout,
            focused,
        }
    }

    fn text(&mut self, s: &str) {
        self.output.push_str(s);
        self.visible_len += s.chars().count();
    }

    fn styled(&mut self, s: &str, ansi_code: &str) {
        self.output.push_str(ansi_code);
        self.output.push_str(s);
        self.output.push_str("\x1b[0m");
        self.visible_len += s.chars().count();
    }

    fn spaces(&mut self, n: usize) {
        for _ in 0..n {
            self.output.push(' ');
        }
        self.visible_len += n;
    }

    fn pad_to(&mut self, target_visible: usize) {
        if target_visible > self.visible_len {
            self.spaces(target_visible - self.visible_len);
        }
    }

    fn button(&mut self, id: ButtonId, label: &str) {
        // Col is 1-indexed. Border character is at pad_left_len + 1. Space after border is pad_left_len + 2.
        // First content character is at pad_left_len + 3.
        let col_start = self.pad_left_len + 3 + (self.visible_len as u16);
        let btn_len = label.chars().count() as u16;
        let col_end = col_start + btn_len.saturating_sub(1);

        self.layout.buttons.push(ButtonRect {
            id,
            row: self.row,
            col_start,
            col_end,
        });

        if id == self.focused {
            self.output.push_str("\x1b[7m\x1b[1;33m");
            self.output.push_str(label);
            self.output.push_str("\x1b[0m");
        } else {
            self.output.push_str("\x1b[1;36m");
            self.output.push_str(label);
            self.output.push_str("\x1b[0m");
        }
        self.visible_len += label.chars().count();
    }

    fn set_vol_bar(&mut self, start: u16, end: u16) {
        self.layout.vol_bar_start = start;
        self.layout.vol_bar_end = end;
    }

    fn finish(self, pad_left: &str) -> String {
        let rem = self.inner_width.saturating_sub(self.visible_len);
        let mut res = String::with_capacity(self.output.len() + rem + 32);
        res.push_str(pad_left);
        res.push_str("\x1b[1;34m│\x1b[0m ");
        res.push_str(&self.output);
        for _ in 0..rem {
            res.push(' ');
        }
        res.push_str(" \x1b[1;34m│\x1b[0m\n");
        res
    }
}

pub fn render_frame(
    state: &UiState,
    layout: &mut UiLayout,
) -> String {
    let width = 76usize;
    let pad_len = if state.term_cols > width as u16 {
        (state.term_cols - width as u16) / 2
    } else {
        0
    };
    let pad_left = " ".repeat(pad_len as usize);
    let inner_width = width - 4; // 72 columns of interior content

    let (on_v, on_vr, on_va) = voice_clips("on");
    let (off_v, off_vr, off_va) = voice_clips("off");
    let voice_count = on_v.len() + on_vr.len() + on_va.len() + off_v.len() + off_vr.len() + off_va.len();

    let mut out = String::with_capacity(4096);

    // Row 1: Top border
    out.push_str(&format!("{pad_left}\x1b[1;34m┌{}┐\x1b[0m\n", "─".repeat(width - 2)));

    // Row 2: Title
    let title_text = format!("🐭  COSMIC MOUSETRAP CONTROL CENTER  v{VERSION}");
    let t_len = title_text.chars().count();
    let t_pad = inner_width.saturating_sub(t_len) / 2;
    let mut r2 = LineBuilder::new(2, pad_len, inner_width, layout, state.focused);
    r2.spaces(t_pad);
    r2.styled(&title_text, "\x1b[1;37m");
    out.push_str(&r2.finish(&pad_left));

    // Row 3: Subtitle
    let sub_text = "Dynamic cursor_follows_focus for Pop!_OS COSMIC Desktop";
    let s_len = sub_text.chars().count();
    let s_pad = inner_width.saturating_sub(s_len) / 2;
    let mut r3 = LineBuilder::new(3, pad_len, inner_width, layout, state.focused);
    r3.spaces(s_pad);
    r3.styled(sub_text, "\x1b[90m");
    out.push_str(&r3.finish(&pad_left));

    // Row 4: Section 1 Divider
    out.push_str(&format!("{pad_left}\x1b[1;34m├{}┤\x1b[0m\n", "─".repeat(width - 2)));

    // Row 5: Section 1 Header
    let mut r5 = LineBuilder::new(5, pad_len, inner_width, layout, state.focused);
    r5.styled("[1] COSMIC SETTING & SYSTEMD SERVICE", "\x1b[1;33m");
    out.push_str(&r5.finish(&pad_left));

    // Row 6: State & Game line
    let mut r6 = LineBuilder::new(6, pad_len, inner_width, layout, state.focused);
    r6.text("cursor_follows_focus: ");
    if state.curr_state {
        r6.styled("[ ON (GAME) ]", "\x1b[1;42;37m");
    } else {
        r6.styled("[ OFF (DESK) ]", "\x1b[1;44;37m");
    }
    r6.spaces(2);
    r6.button(ButtonId::ToggleState, "[ Toggle (s) ]");
    r6.spaces(3);
    r6.text("Game: ");
    if state.game == "none" {
        r6.styled("none", "\x1b[90m");
    } else {
        r6.styled(state.game, "\x1b[1;32m");
    }
    out.push_str(&r6.finish(&pad_left));

    // Row 7: Service line
    let mut r7 = LineBuilder::new(7, pad_len, inner_width, layout, state.focused);
    r7.text("Service: ");
    if state.svc_status == "active" {
        r7.styled("active (running)", "\x1b[1;32m");
    } else if state.svc_status == "failed" {
        r7.styled("failed", "\x1b[1;31m");
    } else {
        r7.styled("inactive", "\x1b[1;33m");
    }
    r7.pad_to(28);
    r7.button(ButtonId::ServiceStart, "[ Start ]");
    r7.spaces(1);
    r7.button(ButtonId::ServiceStop, "[ Stop ]");
    r7.spaces(1);
    r7.button(ButtonId::ServiceRestart, "[ Restart (r) ]");
    out.push_str(&r7.finish(&pad_left));

    // Row 8: Section 2 Divider
    out.push_str(&format!("{pad_left}\x1b[1;34m├{}┤\x1b[0m\n", "─".repeat(width - 2)));

    // Row 9: Section 2 Header
    let mut r9 = LineBuilder::new(9, pad_len, inner_width, layout, state.focused);
    r9.styled("[2] AUDIO FEEDBACK & NOTIFICATIONS", "\x1b[1;33m");
    out.push_str(&r9.finish(&pad_left));

    // Row 10: Sound & Notify line
    let mut r10 = LineBuilder::new(10, pad_len, inner_width, layout, state.focused);
    r10.text("Sound: ");
    if state.cfg.sound {
        r10.styled("Enabled", "\x1b[32m");
        r10.spaces(1);
        r10.button(ButtonId::ToggleSound, "[ Mute (m) ]");
    } else {
        r10.styled("Muted  ", "\x1b[31m");
        r10.spaces(1);
        r10.button(ButtonId::ToggleSound, "[ Unmute (m) ]");
    }
    r10.pad_to(32);
    r10.text("Notify: ");
    if state.cfg.notifications {
        r10.styled("Enabled ", "\x1b[32m");
    } else {
        r10.styled("Disabled", "\x1b[31m");
    }
    r10.spaces(1);
    r10.button(ButtonId::ToggleNotify, "[ Toggle (n) ]");
    out.push_str(&r10.finish(&pad_left));

    // Row 11: Volume line
    layout.vol_row = 11;
    let mut r11 = LineBuilder::new(11, pad_len, inner_width, layout, state.focused);
    r11.text("Master Volume: ");
    r11.button(ButtonId::VolDown, "[ -5 (-) ]");
    r11.spaces(2);

    let vol_pct = state.cfg.volume.min(100);
    r11.styled(&format!("{vol_pct:>3}%"), "\x1b[1m");
    r11.spaces(1);

    // Compute volume bar position for mouse clicking
    let bar_start = pad_len + 3 + (r11.visible_len as u16);
    let vol_bar_filled = (vol_pct as usize * 16) / 100;
    let vol_bar_empty = 16 - vol_bar_filled;
    let bar_label = format!("[{}{}]", "█".repeat(vol_bar_filled), "░".repeat(vol_bar_empty));
    let bar_end = bar_start + bar_label.chars().count() as u16 - 1;
    r11.set_vol_bar(bar_start, bar_end);

    r11.styled(&bar_label, "\x1b[36m");
    r11.spaces(2);
    r11.button(ButtonId::VolUp, "[ +5 (+) ]");
    out.push_str(&r11.finish(&pad_left));

    // Row 12: ON Cue line
    layout.on_cue_row = 12;
    let mut r12 = LineBuilder::new(12, pad_len, inner_width, layout, state.focused);
    r12.text("ON Cue:  ");
    let on_cue_trunc = if state.cfg.sound_on.len() > 11 {
        format!("{:.8}...", state.cfg.sound_on)
    } else {
        format!("{:<11}", state.cfg.sound_on)
    };
    r12.styled(&on_cue_trunc, "\x1b[1m");
    r12.spaces(1);
    r12.button(ButtonId::OnCuePrev, "[ < Prev ]");
    r12.spaces(1);
    r12.button(ButtonId::OnCueNext, "[ Next > ]");
    r12.spaces(2);
    r12.button(ButtonId::TestOn, "[ ▶ Test ON (t) ]");
    out.push_str(&r12.finish(&pad_left));

    // Row 13: OFF Cue line
    layout.off_cue_row = 13;
    let mut r13 = LineBuilder::new(13, pad_len, inner_width, layout, state.focused);
    r13.text("OFF Cue: ");
    let off_cue_trunc = if state.cfg.sound_off.len() > 11 {
        format!("{:.8}...", state.cfg.sound_off)
    } else {
        format!("{:<11}", state.cfg.sound_off)
    };
    r13.styled(&off_cue_trunc, "\x1b[1m");
    r13.spaces(1);
    r13.button(ButtonId::OffCuePrev, "[ < Prev ]");
    r13.spaces(1);
    r13.button(ButtonId::OffCueNext, "[ Next > ]");
    r13.spaces(2);
    r13.button(ButtonId::TestOff, "[ ▶ Test OFF (y) ]");
    out.push_str(&r13.finish(&pad_left));

    // Row 14: Section 3 Divider
    out.push_str(&format!("{pad_left}\x1b[1;34m├{}┤\x1b[0m\n", "─".repeat(width - 2)));

    // Row 15: Section 3 Header
    let mut r15 = LineBuilder::new(15, pad_len, inner_width, layout, state.focused);
    r15.styled("[3] VOICE PACK & SELF-HEALING DOCTOR", "\x1b[1;33m");
    out.push_str(&r15.finish(&pad_left));

    // Row 16: Voice Pack line
    let mut r16 = LineBuilder::new(16, pad_len, inner_width, layout, state.focused);
    r16.text("GLaDOS Voice: ");
    if voice_count >= 106 {
        r16.styled(&format!("{voice_count} clips installed"), "\x1b[32m");
    } else if voice_count > 0 {
        r16.styled(&format!("{voice_count} clips installed"), "\x1b[33m");
    } else {
        r16.styled("Not installed (using chimes)", "\x1b[90m");
    }
    r16.pad_to(50);
    r16.button(ButtonId::VoiceDownload, "[ ⬇ Download (v) ]");
    out.push_str(&r16.finish(&pad_left));

    // Row 17: Doctor line
    let mut r17 = LineBuilder::new(17, pad_len, inner_width, layout, state.focused);
    r17.text("Doctor Status: ");
    r17.styled(state.doctor_summary, "\x1b[1m");
    r17.pad_to(47);
    r17.button(ButtonId::RunDoctor, "[ 🩺 Heal Doctor (d) ]");
    out.push_str(&r17.finish(&pad_left));

    // Row 18: Divider
    out.push_str(&format!("{pad_left}\x1b[1;34m├{}┤\x1b[0m\n", "─".repeat(width - 2)));

    // Row 19: Status Banner
    let mut r19 = LineBuilder::new(19, pad_len, inner_width, layout, state.focused);
    r19.styled("Status: ", "\x1b[1;33m");
    let max_stat = inner_width.saturating_sub(10);
    let stat_str = if state.status_msg.chars().count() > max_stat {
        state.status_msg.chars().take(max_stat.saturating_sub(3)).collect::<String>() + "..."
    } else {
        state.status_msg.to_string()
    };
    r19.text(&stat_str);
    out.push_str(&r19.finish(&pad_left));

    // Row 20: Divider
    out.push_str(&format!("{pad_left}\x1b[1;34m├{}┤\x1b[0m\n", "─".repeat(width - 2)));

    // Row 21: Footer & Quit button
    let mut r21 = LineBuilder::new(21, pad_len, inner_width, layout, state.focused);
    r21.styled("Mouse Click | Tab/Arrows: Move | Enter: Select", "\x1b[90m");
    r21.pad_to(54);
    r21.button(ButtonId::Quit, "[ ✕ Quit (q) ]");
    out.push_str(&r21.finish(&pad_left));

    // Row 22: Bottom border
    out.push_str(&format!("{pad_left}\x1b[1;34m└{}┘\x1b[0m\n", "─".repeat(width - 2)));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for next_c in chars.by_ref() {
                        if next_c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn test_ui_frame_dimensions_and_alignment() {
        let cfg = Config::default();
        let state = UiState {
            term_cols: 80,
            curr_state: false,
            game: "none",
            svc_status: "active",
            cfg: &cfg,
            doctor_summary: "8/8 OK",
            status_msg: "Ready.",
            focused: ButtonId::ToggleState,
        };
        let mut layout = UiLayout {
            buttons: Vec::new(),
            vol_row: 0,
            on_cue_row: 0,
            off_cue_row: 0,
            vol_bar_start: 0,
            vol_bar_end: 0,
        };

        let frame = render_frame(&state, &mut layout);
        let lines: Vec<&str> = frame.lines().collect();

        // 22 rows total
        assert_eq!(lines.len(), 22, "UI must render exactly 22 rows");

        // Verify that every single stripped line has the exact same visual width (78 chars with 2-char centering)
        for (idx, line) in lines.iter().enumerate() {
            let stripped = strip_ansi(line);
            assert_eq!(
                stripped.chars().count(),
                78,
                "Row {} has visual width {}, expected 78: {:?}",
                idx + 1,
                stripped.chars().count(),
                stripped
            );
        }
    }

    #[test]
    fn test_button_hitboxes_match_rendered_labels() {
        let cfg = Config::default();
        let state = UiState {
            term_cols: 80,
            curr_state: true,
            game: "steam_app_123",
            svc_status: "inactive",
            cfg: &cfg,
            doctor_summary: "7/8 OK (1 healed)",
            status_msg: "All good.",
            focused: ButtonId::Quit,
        };
        let mut layout = UiLayout {
            buttons: Vec::new(),
            vol_row: 0,
            on_cue_row: 0,
            off_cue_row: 0,
            vol_bar_start: 0,
            vol_bar_end: 0,
        };

        let frame = render_frame(&state, &mut layout);
        let lines: Vec<String> = frame.lines().map(strip_ansi).collect();

        // Check that all buttons in BUTTON_ORDER are registered
        for &btn_id in BUTTON_ORDER {
            assert!(
                layout.buttons.iter().any(|b| b.id == btn_id),
                "Missing button {:?}",
                btn_id
            );
        }

        // Check every registered button:
        // 1. col_start and col_end are within bounds
        // 2. Click detection on (col_start, row) and (col_end, row) finds this exact button
        // 3. Click at col_start - 1 and col_end + 1 does NOT hit this button
        // 4. The characters on screen from (col_start - 1) to col_end match the button text
        for btn in &layout.buttons {
            assert!(btn.col_start <= btn.col_end);
            assert!(btn.row >= 1 && btn.row <= lines.len() as u16);

            // Test hit detection
            assert_eq!(find_button_at(btn.col_start, btn.row, &layout.buttons), Some(btn.id));
            assert_eq!(find_button_at(btn.col_end, btn.row, &layout.buttons), Some(btn.id));
            assert_ne!(find_button_at(btn.col_start.saturating_sub(1), btn.row, &layout.buttons), Some(btn.id));
            assert_ne!(find_button_at(btn.col_end + 1, btn.row, &layout.buttons), Some(btn.id));

            // Test that characters on screen actually start with '[' and end with ']'
            let line = &lines[(btn.row - 1) as usize];
            let start_idx = (btn.col_start - 1) as usize;
            let end_idx = btn.col_end as usize;
            let slice: String = line.chars().skip(start_idx).take(end_idx - start_idx).collect();
            assert!(slice.starts_with('['), "Button {:?} at col {} row {} slice was {:?}", btn.id, btn.col_start, btn.row, slice);
            assert!(slice.ends_with(']'), "Button {:?} at col {} row {} slice was {:?}", btn.id, btn.col_end, btn.row, slice);
        }

        // Verify that no buttons overlap
        for (i, b1) in layout.buttons.iter().enumerate() {
            for (j, b2) in layout.buttons.iter().enumerate() {
                if i != j && b1.row == b2.row {
                    let overlap = !(b1.col_end < b2.col_start || b2.col_end < b1.col_start);
                    assert!(!overlap, "Buttons {:?} and {:?} overlap on row {}", b1.id, b2.id, b1.row);
                }
            }
        }
    }

    #[test]
    fn test_scroll_wheel_rows() {
        let cfg = Config::default();
        let state = UiState {
            term_cols: 80,
            curr_state: false,
            game: "none",
            svc_status: "active",
            cfg: &cfg,
            doctor_summary: "Ready",
            status_msg: "OK",
            focused: ButtonId::ToggleState,
        };
        let mut layout = UiLayout {
            buttons: Vec::new(),
            vol_row: 0,
            on_cue_row: 0,
            off_cue_row: 0,
            vol_bar_start: 0,
            vol_bar_end: 0,
        };
        let _ = render_frame(&state, &mut layout);
        assert_eq!(layout.vol_row, 11);
        assert_eq!(layout.on_cue_row, 12);
        assert_eq!(layout.off_cue_row, 13);
        assert!(layout.vol_bar_start < layout.vol_bar_end);
    }
}
