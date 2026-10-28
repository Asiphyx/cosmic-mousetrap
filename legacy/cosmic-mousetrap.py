#!/usr/bin/env python3
"""cosmic-mousetrap: auto-toggle COSMIC's `cursor_follows_focus` for games.

Many games (especially under Wine/Proton/gamescope) lose or fight over the
mouse on COSMIC unless `cursor_follows_focus` is on, while the same setting
makes the pointer jump around on the normal desktop (e.g. between browser
tabs/windows). This daemon enables it while a game is running and disables it
when you are back on the desktop.
"""

from __future__ import annotations

import argparse
import glob
import os
import subprocess
import time
from pathlib import Path

__version__ = "1.0.0"

CONFIG_FILE = (
    Path.home() / ".config" / "cosmic" / "com.system76.CosmicComp" / "v1" / "cursor_follows_focus"
)
USER_GAMES_FILE = Path.home() / ".config" / "cosmic-mousetrap" / "games.txt"
POLL_SECONDS = 2.5

# Process names (exe basename or /proc/PID/comm, lowercase) that mean "a game is running".
GAME_BINARIES = {
    "wineserver", "wine-preloader", "wine64-preloader", "gamescope",
    "rpcs3", "yuzu", "ryujinx", "dolphin-emu", "retroarch",
    "pcsx2", "cemu", "duckstation",
}
# Path fragments (exe or cwd) that mean "a game is running" (Steam library games).
GAME_PATH_MARKERS = ("steamapps/common",)


def load_user_games() -> set[str]:
    """Extra process names / path fragments from ~/.config/cosmic-mousetrap/games.txt."""
    try:
        lines = USER_GAMES_FILE.read_text(encoding="utf-8").splitlines()
    except OSError:
        return set()
    return {l.strip().lower() for l in lines if l.strip() and not l.lstrip().startswith("#")}


def is_game_running(extra: set[str]) -> tuple[bool, str]:
    """Return (running, description) if any game or game runtime process is active."""
    names = GAME_BINARIES | {e for e in extra if "/" not in e}
    markers = GAME_PATH_MARKERS + tuple(e for e in extra if "/" in e)
    for pid_dir in glob.glob("/proc/[0-9]*"):
        try:
            exe = os.readlink(f"{pid_dir}/exe")
            if any(m in exe for m in markers):
                return True, os.path.basename(exe)
            exe_name = os.path.basename(exe).lower()
            if exe_name in names:
                return True, exe_name
            with open(f"{pid_dir}/comm", encoding="utf-8", errors="ignore") as f:
                comm = f.read().strip().lower()
            if comm in names:
                return True, comm
            cwd = os.readlink(f"{pid_dir}/cwd")
            if any(m in cwd for m in markers):
                return True, os.path.basename(cwd)
        except OSError:
            continue
    return False, ""


def get_setting() -> bool:
    try:
        return CONFIG_FILE.read_text().strip().lower() == "true"
    except OSError:
        return False


def set_setting(enabled: bool) -> None:
    """Atomically write the COSMIC config value (cosmic-comp watches this file live)."""
    CONFIG_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONFIG_FILE.with_suffix(".tmp")
    tmp.write_text("true" if enabled else "false")
    tmp.replace(CONFIG_FILE)


def notify(title: str, msg: str, enabled: bool) -> None:
    if not enabled:
        return
    try:
        subprocess.run(
            ["notify-send", "-u", "low", "-a", "Mousetrap", "-i", "input-gaming", title, msg],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1.5,
        )
    except (OSError, subprocess.SubprocessError):
        pass


def apply(game: bool, detail: str, quiet: bool) -> None:
    if game:
        set_setting(True)
        print(f"[+] Game detected ({detail}): cursor_follows_focus = true", flush=True)
        notify("Game Mode ON", f"{detail}\ncursor_follows_focus enabled", not quiet)
    else:
        set_setting(False)
        print("[-] No game running: cursor_follows_focus = false", flush=True)
        notify("Desktop Mode", "cursor_follows_focus disabled", not quiet)


def run_daemon(quiet: bool) -> None:
    print(f"cosmic-mousetrap {__version__} daemon started.", flush=True)
    last: bool | None = None
    while True:
        try:
            game, detail = is_game_running(load_user_games())
            if last is None:
                if game != get_setting():
                    apply(game, detail, quiet)
                last = game
            elif game != last:
                apply(game, detail, quiet)
                last = game
            time.sleep(POLL_SECONDS)
        except KeyboardInterrupt:
            print("Stopped.")
            break
        except Exception as e:  # keep the daemon alive
            print(f"error: {e}", flush=True)
            time.sleep(3)


def main() -> None:
    p = argparse.ArgumentParser(description="Auto-toggle COSMIC cursor_follows_focus while games run.")
    p.add_argument("action", nargs="?", default="status",
                   choices=["status", "daemon", "on", "off", "toggle"])
    p.add_argument("--quiet", action="store_true", help="no desktop notifications (daemon)")
    p.add_argument("--version", action="version", version=__version__)
    a = p.parse_args()

    if a.action == "daemon":
        run_daemon(a.quiet)
    elif a.action == "status":
        game, detail = is_game_running(load_user_games())
        print(f"cursor_follows_focus: {'true' if get_setting() else 'false'}")
        print(f"game detected:        {detail if game else 'none'}")
    elif a.action == "toggle":
        set_setting(not get_setting())
        print(f"cursor_follows_focus = {str(get_setting()).lower()}")
    else:
        set_setting(a.action == "on")
        print(f"cursor_follows_focus = {str(a.action == 'on').lower()}")


if __name__ == "__main__":
    main()
