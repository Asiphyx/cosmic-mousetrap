# Mousetrap

Reliable in-game mouse capture on **COSMIC (Pop!_OS 24.04 / cosmic-comp)** *without* breaking desktop tab dragging. Install once, never touch it again.

> **The symptom this fixes:** with `cursor_follows_focus` permanently **on**, Proton/Wine games capture the mouse correctly, but on the desktop browser tabs misbehave: tearing a tab off a Chrome window makes it snap/teleport to the center of the screen (or across to the other monitor) in the middle of the drag. Turn the setting **off** and tab dragging is fine, but games can lose the mouse. You shouldn't have to choose.

## The problem
- Games (especially Wine/Proton/gamescope/emulators) can lose the pointer, or it stays on another window/monitor, unless COSMIC's `cursor_follows_focus` is **on**.
- Leaving it on all the time makes the cursor jump on the normal desktop (dragging browser tabs, switching windows).

## What it does
A tiny user-level daemon checks every ~2.5 s for a running game and flips `cursor_follows_focus`:
**game running -> on** (immediately), **back on desktop -> off** (after ~5 s, so Steam/Proton launcher-to-game handoffs don't flap it). Stopping the service or logging out resets it to off. It writes
`~/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus`, which cosmic-comp picks up live (no logout).

Detected as a game: anything running from `steamapps/common`, plus `wineserver`, `wine-preloader`, `wine64-preloader`, `gamescope`,
`retroarch`, `dolphin-emu`, `pcsx2`, `rpcs3`, `yuzu`, `ryujinx`, `cemu`, `duckstation`. Add your own in
`~/.config/cosmic-mousetrap/games.txt`.

## Notifications and sounds
Every switch shows a small popup with an icon and says exactly what happened:

| Mode | Title | Icon | Text |
|---|---|---|---|
| ON | **Game mode ON** | gamepad | `<game> detected` / `cursor_follows_focus: ON (mouse stays with the game)` |
| OFF | **Desktop mode** | mouse | `No game running` / `cursor_follows_focus: OFF (tabs and windows won't snap)` |

New popups replace the previous one instead of stacking. Each switch can also play a sound:

```bash
cosmic-mousetrap sound list                 # presets: pop, unpop, chime, ping, bell, attention, ...
cosmic-mousetrap sound set on chime         # pick a preset for ON
cosmic-mousetrap sound set off ~/Music/whoosh.mp3   # your own file (wav/ogg/oga/opus/mp3/flac, <= 10 MB)
cosmic-mousetrap sound set off none         # silence one of them
cosmic-mousetrap sound volume 40            # 0-100
cosmic-mousetrap sound test                 # hear both cues now
cosmic-mousetrap sound mute | unmute | reset
cosmic-mousetrap notifications disable      # popups off, sound stays
```

Custom files are copied into `~/.config/cosmic-mousetrap/sounds/`, so moving the original is fine. Point a cue at a **folder** instead (`sound set on ~/Music/mousetrap-on/`) and a random file from it plays each time. Playback uses `pw-play`, `paplay`, `ffplay` or `aplay` (first that works). Everything is also editable in `~/.config/cosmic-mousetrap/config.toml` (applies live), including the icons (`icon_on` / `icon_off`: a theme icon name or an image path).

### Audio cues & Synthetic GLaDOS Voice Lines
Mousetrap includes custom mechanical signature cues (chime+snap for ON, latch+sigh for OFF) and support for 106 synthetic GLaDOS-style voice lines (parody/fan tribute) across 3 tiers (80% standard, 18% rare, 2% existential anomalies):

> **ON:** "Welcome to the chamber. Please leave your excuses at the loading screen." / "MouseTrap armed. Your cursor has been sentenced to a single window." ...
> **OFF:** "Simulation closed. Your cursor has been granted parole." / "Game over. Your tabs are safe." ...

```bash
# Option 1: Download the pre-rendered 106-clip synthetic voice pack
cosmic-mousetrap voice download

# Option 2: Build voice clips locally using Piper TTS
cosmic-mousetrap voice setup

# Voice management
cosmic-mousetrap voice status       # check installed voice clips
cosmic-mousetrap voice lines        # inspect or edit ~/.config/cosmic-mousetrap/lines.txt
cosmic-mousetrap sound set on pop   # switch back to a standard preset ("voice" to switch back)
```

- When voice mode is active without installed voice packs, Mousetrap gracefully falls back to signature chimes or system desktop presets (`pop` / `unpop`).
- Spoken lines use persistent shuffle-bags (every line in a tier plays before repeating).

## Install

```bash
git clone https://github.com/Asiphyx/cosmic-mousetrap.git && cd cosmic-mousetrap
./install.sh
```

To automatically install with the full 106-clip GLaDOS voice pack:
```bash
./install.sh --with-voice
```

Installs, in order: a source build (if `cargo` is present), otherwise the prebuilt ~480 KB x86_64 binary from GitHub Releases (checksum-verified), otherwise the Python version in `legacy/` (core switching only, no sound). Requires systemd (user); `notify-send` is optional. Upgrading from the pre-release `cosmic-game-mode` name is handled automatically.

Without the service: `cargo install --git https://github.com/Asiphyx/cosmic-mousetrap.git`, then run `cosmic-mousetrap daemon` yourself.

## Visual Control Center (TUI)

Mousetrap includes an interactive visual terminal UI with clickable buttons and mouse wheel support built entirely in standard Rust (zero external crates):

```bash
cosmic-mousetrap ui      # or: cosmic-mousetrap tui / menu
```

- **Mouse support:** Full SGR 1006 mouse tracking—click any button directly on screen, or scroll up/down on volume and cue selectors to adjust them.
- **Keyboard navigation:** Move with `Tab` / `Shift-Tab` / Arrow keys, press `Enter` or `Space` to select.
- **Hotkeys:**
  - `[S]` Toggle state (`cursor_follows_focus` ON / OFF)
  - `[M]` Toggle mute / unmute
  - `[+]` / `[-]` Adjust master volume
  - `[T]` / `[Y]` Test ON / OFF audio cues
  - `[N]` Toggle notifications
  - `[V]` Download GLaDOS voice pack
  - `[D]` Run Self-Healing Doctor
  - `[R]` Restart systemd service
  - `[Q]` or `Esc` Cleanly quit
- **Live dashboard:** Polls and updates in real time—shows current setting, running game process name, systemd service state, and volume bar.
- **Bulletproof terminal restoration:** Automatically restores terminal cooked mode, shows cursor, and exits alternate screen on exit, signals (SIGINT, SIGTERM, SIGHUP), or panic.

## Self-Healing Doctor

Diagnose and automatically repair system configuration, services, audio backends, and cues:

```bash
cosmic-mousetrap doctor         # run all diagnostic checks in read-only mode
cosmic-mousetrap doctor --heal  # automatically repair any discovered issues
```

The doctor validates 8 critical subsystems:
1. **COSMIC desktop setting:** Checks readability and writability of `~/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus`; auto-heal initializes missing directories and repairs corrupted values.
2. **App configuration:** Validates `~/.config/cosmic-mousetrap/config.toml`; auto-heal writes default or sanitized configuration if missing or damaged.
3. **Systemd user service:** Verifies unit file installation and running state; auto-heal installs the unit file and reloads/restarts the service.
4. **Audio pipeline:** Detects available audio backends (`pw-play`, `paplay`, `ffplay`, `aplay`).
5. **Desktop notifications:** Checks `notify-send` availability.
6. **Audio cues & voice pack:** Checks signature chimes (`on-cue.wav`, `off-cue.wav`) and voice clips; auto-heal restores missing cues and downloads the GLaDOS voice pack if voice mode has 0 clips.
7. **Daemon instance monitor:** Detects duplicate or orphaned daemon instances and terminates stale processes.
8. **Stale temporary files:** Scans for and cleans up orphaned `.tmp` files.

## Performance & Low-Overhead Net/Proc Engine

Mousetrap is designed to run continuously in the background with near-zero CPU and memory overhead:
- **`last_game_pid` fast-path:** Once a game process is identified, subsequent polls check that PID directly (<0.1 ms), bypassing the entire `/proc` directory traversal for the remainder of your gaming session.
- **UID filtering & `comm`-first scan:** When scanning `/proc`, system and kernel processes belonging to other users are skipped immediately. The 16-byte `comm` is checked before expensive symlink resolution (`exe` and `cwd`).
- **File mtime caching:** `config.toml` and `games.txt` are cached in memory; re-parsing only happens when their modification timestamp changes.

## Use
```bash
cosmic-mousetrap ui              # visual control center with clickable settings
cosmic-mousetrap doctor [--heal] # diagnose and self-repair configuration and service
cosmic-mousetrap status          # current setting, detected game, sound/notification config
cosmic-mousetrap on|off|toggle   # manual override (the daemon corrects it on the next change)
systemctl --user status cosmic-mousetrap
journalctl --user -u cosmic-mousetrap -f
```

## Uninstall
```bash
./uninstall.sh
```
To also remove user configuration and cached voice clips:
```bash
./uninstall.sh --purge
```

## Limits
- Polling-based; detection is by process name/path, so non-Steam native games need an entry in `games.txt`.
- Only toggles the one cosmic-comp setting; it won't fix unrelated input bugs.
- Write path is cosmic-comp's config schema v1; a future COSMIC update may change it.

## License & Provenance

### Software Code
The core daemon, CLI, and TUI are dual-licensed under **MIT OR Apache-2.0** ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

### Voice Assets & Fan Tribute Notice
- The announcement dialogue lines are original parody scripts written for Mousetrap COSMIC.
- The optional 106-clip voice pack consists of synthetic, non-commercial fan-tribute audio synthesized using neural text-to-speech as an homage to the iconic GLaDOS character from Valve Corporation's *Portal* series.
- *Portal* and *GLaDOS* are trademarks and copyrights of Valve Corporation. This project is unaffiliated with, endorsed by, or sponsored by Valve Corporation. All voice clips are distributed strictly for non-commercial fan recreation under fair use and Valve's video/fan content policy conventions.
