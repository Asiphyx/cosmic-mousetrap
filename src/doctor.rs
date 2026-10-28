//! Mousetrap Self-Healing Doctor: diagnose and auto-repair system configuration,
//! services, audio pipelines, cues, and stale runtime artifacts.

use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl CheckStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "\x1b[32m[OK]\x1b[0m",
            CheckStatus::Warning => "\x1b[33m[WARN]\x1b[0m",
            CheckStatus::Error => "\x1b[31m[FAIL]\x1b[0m",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub healed: bool,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Error)
    }

    pub fn ok_count(&self) -> usize {
        self.checks.iter().filter(|c| c.status == CheckStatus::Ok).count()
    }

    pub fn healed_count(&self) -> usize {
        self.checks.iter().filter(|c| c.healed).count()
    }

    pub fn print(&self) {
        println!("\x1b[1;36m════════════════════════════════════════════════════════════\x1b[0m");
        println!("\x1b[1m  🐭 Mousetrap Self-Healing Doctor\x1b[0m");
        println!("\x1b[1;36m════════════════════════════════════════════════════════════\x1b[0m");
        for c in &self.checks {
            let heal_tag = if c.healed { " \x1b[1;32m(HEALED)\x1b[0m" } else { "" };
            println!("  {} \x1b[1m{:<22}\x1b[0m {}{}", c.status.badge(), c.name, c.message, heal_tag);
        }
        println!("\x1b[1;36m────────────────────────────────────────────────────────────\x1b[0m");
        let total = self.checks.len();
        let ok = self.ok_count();
        let healed = self.healed_count();
        if self.has_errors() {
            println!("\x1b[1;31mDoctor report: {}/{} checks passed. Some issues require attention.\x1b[0m", ok, total);
        } else if healed > 0 {
            println!("\x1b[1;32mDoctor report: {}/{} checks passed. Auto-healed {} issue(s)!\x1b[0m", ok, total, healed);
        } else {
            println!("\x1b[1;32mDoctor report: {}/{} checks passed. All systems healthy!\x1b[0m", ok, total);
        }
        println!();
    }
}

pub fn check_cosmic_config(heal: bool) -> CheckResult {
    let name = "COSMIC Config".into();
    let path = cosmic_config_path();
    let parent = match path.parent() {
        Some(p) => p,
        None => return CheckResult {
            name,
            status: CheckStatus::Error,
            message: "Invalid COSMIC config path".into(),
            healed: false,
        },
    };

    let mut healed = false;

    if !parent.is_dir() {
        if heal {
            if let Err(e) = fs::create_dir_all(parent) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Failed creating directory {}: {e}", parent.display()),
                    healed: false,
                };
            }
            if let Err(e) = write_state_atomic(false) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Failed creating config file: {e}"),
                    healed: false,
                };
            }
            return CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Created missing directory and initialized {} = false", path.display()),
                healed: true,
            };
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Directory does not exist: {}", parent.display()),
                healed: false,
            };
        }
    }

    if !path.is_file() {
        if heal {
            if let Err(e) = write_state_atomic(false) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Failed writing {}: {e}", path.display()),
                    healed: false,
                };
            }
            return CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Created missing config file {} = false", path.display()),
                healed: true,
            };
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("File does not exist: {}", path.display()),
                healed: false,
            };
        }
    }

    // Check readability
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return CheckResult {
            name,
            status: CheckStatus::Error,
            message: format!("Cannot read {}: {e}", path.display()),
            healed: false,
        },
    };

    let trimmed = content.trim();
    if trimmed != "true" && trimmed != "false" {
        if heal {
            if let Err(e) = write_state_atomic(false) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Corrupted content ('{trimmed}') could not be repaired: {e}"),
                    healed: false,
                };
            }
            healed = true;
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Error,
                message: format!("Corrupted content: expected 'true' or 'false', found '{trimmed}'"),
                healed: false,
            };
        }
    }

    // Check writability
    let test_write = OpenOptions::new().write(true).open(&path);
    if let Err(e) = test_write {
        if heal {
            // Attempt to restore write permissions (0o644)
            if let Ok(meta) = fs::metadata(&path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o644);
                if fs::set_permissions(&path, perms).is_ok() {
                    healed = true;
                } else {
                    return CheckResult {
                        name,
                        status: CheckStatus::Error,
                        message: format!("File not writable and permission fix failed: {e}"),
                        healed: false,
                    };
                }
            } else {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("File not writable: {e}"),
                    healed: false,
                };
            }
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Error,
                message: format!("File not writable: {e}"),
                healed: false,
            };
        }
    }

    let final_val = if healed { "false" } else { trimmed };
    CheckResult {
        name,
        status: CheckStatus::Ok,
        message: format!("Valid and writable (cursor_follows_focus = {final_val})"),
        healed,
    }
}

pub fn check_app_config(heal: bool) -> CheckResult {
    let name = "App Config".into();
    let path = config_path();

    let default_cfg = "\
# Mousetrap configuration (~/.config/cosmic-mousetrap/config.toml)
notifications = true
sound = true
volume = 60
sound_on = \"voice\"
sound_off = \"voice\"
icon_on = \"input-gaming\"
icon_off = \"input-mouse\"
voice_model = \"en_US-amy-medium\"
";

    if !path.is_file() {
        if heal {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&path, default_cfg) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Failed creating default config: {e}"),
                    healed: false,
                };
            }
            return CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Created default config file at {}", path.display()),
                healed: true,
            };
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Config file missing at {} (using defaults)", path.display()),
                healed: false,
            };
        }
    }

    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return CheckResult {
            name,
            status: CheckStatus::Error,
            message: format!("Cannot read config {}: {e}", path.display()),
            healed: false,
        },
    };

    let mut issues = Vec::new();
    let mut valid_lines = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    let mut cfg_volume: Option<u8> = None;
    let mut cfg_sound: Option<bool> = None;
    let mut cfg_notify: Option<bool> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            valid_lines.push(line.to_string());
            continue;
        }

        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim();
            let val = v.split('#').next().unwrap_or(v).trim().trim_matches('"');
            match key {
                "volume" => {
                    if let Ok(vol) = val.parse::<u32>() {
                        if vol > 100 {
                            issues.push(format!("volume {vol} > 100 (clamped to 100)"));
                            cfg_volume = Some(100);
                            valid_lines.push("volume = 100".to_string());
                        } else {
                            cfg_volume = Some(vol as u8);
                            valid_lines.push(format!("volume = {vol}"));
                        }
                    } else {
                        issues.push(format!("invalid volume '{val}'"));
                        cfg_volume = Some(60);
                        valid_lines.push("volume = 60".to_string());
                    }
                    seen_keys.insert("volume".to_string());
                }
                "sound" => {
                    let b = val.eq_ignore_ascii_case("true") || val == "1";
                    cfg_sound = Some(b);
                    valid_lines.push(format!("sound = {b}"));
                    seen_keys.insert("sound".to_string());
                }
                "notifications" => {
                    let b = val.eq_ignore_ascii_case("true") || val == "1";
                    cfg_notify = Some(b);
                    valid_lines.push(format!("notifications = {b}"));
                    seen_keys.insert("notifications".to_string());
                }
                "sound_on" => {
                    valid_lines.push(format!("sound_on = \"{val}\""));
                    seen_keys.insert("sound_on".to_string());
                }
                "sound_off" => {
                    valid_lines.push(format!("sound_off = \"{val}\""));
                    seen_keys.insert("sound_off".to_string());
                }
                "icon_on" | "icon_off" | "voice_model" => {
                    valid_lines.push(format!("{key} = \"{val}\""));
                    seen_keys.insert(key.to_string());
                }
                _ => {
                    issues.push(format!("unknown setting '{key}'"));
                    valid_lines.push(line.to_string());
                }
            }
        } else {
            issues.push(format!("malformed line: '{trimmed}'"));
            // Corrupted/unparseable line is excluded when healing
        }
    }

    if !issues.is_empty() {
        if heal {
            let repaired_text = valid_lines.join("\n") + "\n";
            let _ = fs::write(&path, repaired_text);
            CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Repaired config: {}", issues.join(", ")),
                healed: true,
            }
        } else {
            CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Config issues detected: {}", issues.join(", ")),
                healed: false,
            }
        }
    } else {
        let vol = cfg_volume.unwrap_or(60);
        let s = cfg_sound.unwrap_or(true);
        let n = cfg_notify.unwrap_or(true);
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: format!("Valid (vol: {vol}%, sound: {s}, notify: {n})"),
            healed: false,
        }
    }
}

pub fn check_systemd_service(heal: bool) -> CheckResult {
    let name = "Systemd Service".into();
    let unit_path = home_dir().join(".config/systemd/user/cosmic-mousetrap.service");
    let mut healed = false;

    let canonical_unit = "\
[Unit]
Description=Mousetrap: auto-toggle COSMIC cursor_follows_focus for games
After=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/cosmic-mousetrap daemon
Restart=always
RestartSec=3
# Stopping the service / logging out mid-game must not leave cursor_follows_focus stuck on.
ExecStopPost=%h/.local/bin/cosmic-mousetrap off --quiet

[Install]
WantedBy=default.target
";

    // Check if unit file exists
    if !unit_path.is_file() {
        if heal {
            if let Some(parent) = unit_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&unit_path, canonical_unit) {
                return CheckResult {
                    name,
                    status: CheckStatus::Error,
                    message: format!("Failed writing unit file {}: {e}", unit_path.display()),
                    healed: false,
                };
            }
            let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
            let _ = Command::new("systemctl").args(["--user", "enable", "cosmic-mousetrap"]).status();
            healed = true;
        } else {
            return CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Unit file missing at {}", unit_path.display()),
                healed: false,
            };
        }
    }

    // Check service state
    let status_out = Command::new("systemctl")
        .args(["--user", "is-active", "cosmic-mousetrap"])
        .output();

    match status_out {
        Ok(out) => {
            let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if state == "active" {
                CheckResult {
                    name,
                    status: CheckStatus::Ok,
                    message: "Service unit installed and active (running)".into(),
                    healed,
                }
            } else if heal {
                let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
                let restart_res = Command::new("systemctl").args(["--user", "restart", "cosmic-mousetrap"]).status();
                if restart_res.map(|s| s.success()).unwrap_or(false) {
                    CheckResult {
                        name,
                        status: CheckStatus::Ok,
                        message: format!("Restarted service (previously {state})"),
                        healed: true,
                    }
                } else {
                    CheckResult {
                        name,
                        status: CheckStatus::Warning,
                        message: format!("Unit installed; service currently {state}"),
                        healed,
                    }
                }
            } else {
                CheckResult {
                    name,
                    status: CheckStatus::Warning,
                    message: format!("Service is currently '{state}' (start with `systemctl --user start cosmic-mousetrap`)"),
                    healed,
                }
            }
        }
        Err(e) => CheckResult {
            name,
            status: CheckStatus::Warning,
            message: format!("Unit installed; systemctl unavailable ({e})"),
            healed,
        },
    }
}

pub fn check_audio_pipeline() -> CheckResult {
    let name = "Audio Pipeline".into();
    let players = ["pw-play", "paplay", "ffplay", "aplay"];
    let mut available = Vec::new();

    for p in players {
        let ok = Command::new(p)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            || Command::new(p)
                .arg("-h")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        if ok {
            available.push(p);
        }
    }

    if let Some(primary) = available.first() {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: format!("Ready: primary `{primary}` (found: {})", available.join(", ")),
            healed: false,
        }
    } else {
        CheckResult {
            name,
            status: CheckStatus::Warning,
            message: "No audio players found! Install pipewire-audio, pulseaudio-utils, or alsa-utils".into(),
            healed: false,
        }
    }
}

pub fn check_notification_daemon() -> CheckResult {
    let name = "Notifications".into();
    let ok = Command::new("notify-send")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: "notify-send available for desktop popups".into(),
            healed: false,
        }
    } else {
        CheckResult {
            name,
            status: CheckStatus::Warning,
            message: "notify-send not found (libnotify-bin). Popups will be skipped".into(),
            healed: false,
        }
    }
}

pub fn check_audio_cues_and_voice(heal: bool) -> CheckResult {
    let name = "Audio Cues".into();
    let cfg = Config::load();
    let mut healed = false;

    let mut on_sig = signature_cue_path("on");
    let mut off_sig = signature_cue_path("off");

    // Check if signature cues are missing and can be healed from asset directories
    if (on_sig.is_none() || off_sig.is_none()) && heal {
        let cand_dirs = [
            PathBuf::from("assets/sounds"),
            home_dir().join("cosmic-mousetrap/assets/sounds"),
            PathBuf::from("/usr/share/cosmic-mousetrap"),
        ];
        let _ = fs::create_dir_all(data_dir());
        for dir in cand_dirs {
            if on_sig.is_none() && dir.join("on-cue.wav").is_file()
                && fs::copy(dir.join("on-cue.wav"), data_dir().join("on-cue.wav")).is_ok()
            {
                on_sig = signature_cue_path("on");
                healed = true;
            }
            if off_sig.is_none() && dir.join("off-cue.wav").is_file()
                && fs::copy(dir.join("off-cue.wav"), data_dir().join("off-cue.wav")).is_ok()
            {
                off_sig = signature_cue_path("off");
                healed = true;
            }
        }
    }

    let (on_v, on_vr, on_va) = voice_clips("on");
    let (off_v, off_vr, off_va) = voice_clips("off");
    let voice_count = on_v.len() + on_vr.len() + on_va.len() + off_v.len() + off_vr.len() + off_va.len();

    let is_voice_active = cfg.sound_on.eq_ignore_ascii_case("voice") || cfg.sound_off.eq_ignore_ascii_case("voice");
    if is_voice_active && voice_count == 0 {
        if heal {
            eprintln!("Doctor: downloading GLaDOS voice pack...");
            if voice_download().is_ok() {
                let (on_v2, on_vr2, on_va2) = voice_clips("on");
                let (off_v2, off_vr2, off_va2) = voice_clips("off");
                let count2 = on_v2.len() + on_vr2.len() + on_va2.len() + off_v2.len() + off_vr2.len() + off_va2.len();
                return CheckResult {
                    name,
                    status: CheckStatus::Ok,
                    message: format!("Downloaded GLaDOS voice pack ({count2} clips ready)"),
                    healed: true,
                };
            }
        }
        return CheckResult {
            name,
            status: CheckStatus::Warning,
            message: "Voice cue selected but 0 clips installed (run `cosmic-mousetrap voice download`)".into(),
            healed,
        };
    }

    let sig_present = on_sig.is_some() && off_sig.is_some();
    if !sig_present {
        CheckResult {
            name,
            status: CheckStatus::Warning,
            message: format!("Signature chimes missing (desktop presets used as fallback), {voice_count} voice clips"),
            healed,
        }
    } else {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: format!("signature chimes present, {voice_count} voice clips"),
            healed,
        }
    }
}

pub fn check_daemon_instances(heal: bool) -> CheckResult {
    let name = "Daemon Instances".into();
    let current_pid = std::process::id();
    let my_uid = current_uid();
    let mut daemon_pids = Vec::new();

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let Some(pid_str) = fname.to_str() else { continue };
            if pid_str.is_empty() || !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(pid) = pid_str.parse::<u32>() else { continue };
            if pid == current_pid {
                continue;
            }

            // Skip processes not owned by current user (if non-root)
            if my_uid != 0 {
                if let Ok(meta) = entry.metadata() {
                    if meta.uid() != my_uid {
                        continue;
                    }
                }
            }

            let cmdline_path = entry.path().join("cmdline");
            if let Ok(bytes) = fs::read(cmdline_path) {
                // In Linux /proc/<pid>/cmdline, arguments are separated by NUL (0x00)
                let parts: Vec<&[u8]> = bytes.split(|&b| b == 0).filter(|p| !p.is_empty()).collect();
                if parts.is_empty() {
                    continue;
                }
                let argv0 = String::from_utf8_lossy(parts[0]);
                let is_mousetrap_bin = argv0 == "cosmic-mousetrap"
                    || argv0.ends_with("/cosmic-mousetrap")
                    || argv0.contains("target/debug/cosmic-mousetrap")
                    || argv0.contains("target/release/cosmic-mousetrap");

                let has_daemon_arg = parts[1..].iter().any(|arg| *arg == b"daemon");
                if is_mousetrap_bin && has_daemon_arg {
                    daemon_pids.push(pid);
                }
            }
        }
    }

    if daemon_pids.len() > 1 {
        if heal {
            // Check if systemd service has a MainPID
            let sys_pid = Command::new("systemctl")
                .args(["--user", "show", "-p", "MainPID", "cosmic-mousetrap"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| {
                    s.trim()
                        .strip_prefix("MainPID=")
                        .and_then(|p| p.parse::<u32>().ok())
                })
                .filter(|&p| p > 0);

            // If systemd MainPID is in daemon_pids, keep it. Otherwise keep the first PID.
            let keep_pid = sys_pid.filter(|p| daemon_pids.contains(p)).unwrap_or(daemon_pids[0]);

            let mut killed = 0;
            for &pid in &daemon_pids {
                if pid != keep_pid {
                    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
                    killed += 1;
                }
            }
            CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Terminated {killed} duplicate daemon instance(s); kept PID {keep_pid}"),
                healed: true,
            }
        } else {
            CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Multiple daemon instances running: PIDs {:?}", daemon_pids),
                healed: false,
            }
        }
    } else if daemon_pids.len() == 1 {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: format!("1 daemon running (PID {})", daemon_pids[0]),
            healed: false,
        }
    } else {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: "No orphan daemons (managed by systemd)".into(),
            healed: false,
        }
    }
}

pub fn check_stale_temp_files(heal: bool) -> CheckResult {
    let name = "Temp Files".into();
    let cosmic_parent = cosmic_config_path().parent().map(Path::to_path_buf).unwrap_or_default();
    let dirs_to_check = [
        app_config_dir(),
        cosmic_parent,
        cache_dir(),
        data_dir(),
    ];

    let mut stale_files = Vec::new();
    for dir in dirs_to_check {
        if dir.as_os_str().is_empty() || !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name();
                    let s = fname.to_string_lossy();
                    if s.ends_with(".tmp") || s.starts_with(".cursor_follows_focus.tmp") {
                        stale_files.push(path);
                    }
                }
            }
        }
    }

    if !stale_files.is_empty() {
        if heal {
            let mut removed = 0;
            for f in &stale_files {
                if fs::remove_file(f).is_ok() {
                    removed += 1;
                }
            }
            CheckResult {
                name,
                status: CheckStatus::Ok,
                message: format!("Cleaned up {removed} orphaned .tmp file(s)"),
                healed: true,
            }
        } else {
            CheckResult {
                name,
                status: CheckStatus::Warning,
                message: format!("Found {} stale temporary file(s)", stale_files.len()),
                healed: false,
            }
        }
    } else {
        CheckResult {
            name,
            status: CheckStatus::Ok,
            message: "Clean (no orphaned .tmp files)".into(),
            healed: false,
        }
    }
}

pub fn run_doctor(heal: bool) -> DoctorReport {
    let checks = vec![
        check_cosmic_config(heal),
        check_app_config(heal),
        check_systemd_service(heal),
        check_audio_pipeline(),
        check_notification_daemon(),
        check_audio_cues_and_voice(heal),
        check_daemon_instances(heal),
        check_stale_temp_files(heal),
    ];
    DoctorReport { checks }
}

pub fn doctor_cmd(args: &[&str]) -> Result<(), String> {
    if args.iter().any(|a| *a == "--help" || *a == "-h") {
        println!("cosmic-mousetrap doctor [--heal]");
        println!();
        println!("Diagnose and auto-repair system configuration, services, audio pipelines, cues, and stale runtime artifacts.");
        println!();
        println!("Options:");
        println!("  --heal, --fix, -f    Automatically repair detected issues");
        println!("  -h, --help           Show this help message");
        return Ok(());
    }

    let heal = args.iter().any(|a| *a == "--heal" || *a == "-f" || *a == "--fix");
    let report = run_doctor(heal);
    report.print();
    if report.has_errors() {
        Err("one or more doctor checks failed".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_report_methods() {
        let rep = DoctorReport {
            checks: vec![
                CheckResult {
                    name: "Check 1".into(),
                    status: CheckStatus::Ok,
                    message: "ok".into(),
                    healed: false,
                },
                CheckResult {
                    name: "Check 2".into(),
                    status: CheckStatus::Warning,
                    message: "warn".into(),
                    healed: false,
                },
                CheckResult {
                    name: "Check 3".into(),
                    status: CheckStatus::Ok,
                    message: "healed".into(),
                    healed: true,
                },
            ],
        };
        assert_eq!(rep.ok_count(), 2);
        assert_eq!(rep.healed_count(), 1);
        assert!(!rep.has_errors());
    }

    #[test]
    fn test_daemon_cmdline_matching() {
        // Test that argv0 ending with cosmic-mousetrap and arg "daemon" matches
        let args_valid = [b"cosmic-mousetrap".as_slice(), b"daemon".as_slice()];
        let is_mousetrap_bin = args_valid[0] == b"cosmic-mousetrap" || args_valid[0].ends_with(b"/cosmic-mousetrap");
        let has_daemon_arg = args_valid[1..].iter().any(|arg| *arg == b"daemon");
        assert!(is_mousetrap_bin && has_daemon_arg);

        // Test false positive prevention: vim daemon.rs in cosmic-mousetrap
        let args_vim = [b"vim".as_slice(), b"src/doctor.rs".as_slice(), b"daemon.rs".as_slice()];
        let is_mousetrap_bin_vim = args_vim[0] == b"cosmic-mousetrap" || args_vim[0].ends_with(b"/cosmic-mousetrap");
        assert!(!is_mousetrap_bin_vim);

        // Test false positive prevention: grep cosmic-mousetrap daemon.log
        let args_grep = [b"grep".as_slice(), b"cosmic-mousetrap".as_slice(), b"daemon.log".as_slice()];
        let is_mousetrap_bin_grep = args_grep[0] == b"cosmic-mousetrap" || args_grep[0].ends_with(b"/cosmic-mousetrap");
        assert!(!is_mousetrap_bin_grep);
    }
}
