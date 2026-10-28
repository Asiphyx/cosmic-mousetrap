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

### Audio cues & GLaDOS lines
Mousetrap includes custom mechanical signature cues (chime+snap for ON, latch+sigh for OFF) and support for 106 authentic GLaDOS voice lines across 3 tiers (80% standard, 18% rare, 2% existential anomalies):

> **ON:** "Welcome to the chamber. Please leave your excuses at the loading screen." / "MouseTrap armed. Your cursor has been sentenced to a single window." ...
> **OFF:** "Simulation closed. Your cursor has been granted parole." / "Game over. Your tabs are safe." ...

```bash
# Option 1: Download the pre-rendered 106-clip GLaDOS voice pack
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

## Use
```bash
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

License: MIT OR Apache-2.0 ([MIT](LICENSE-MIT), [Apache](LICENSE-APACHE)).
