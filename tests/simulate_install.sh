#!/usr/bin/env bash
set -euo pipefail

TEST_DIR="$(mktemp -d /tmp/cosmic-test-XXXXXX)"
echo "Test environment directory: $TEST_DIR"

REAL_HOME="$HOME"
FAKE_HOME="$TEST_DIR/user"
mkdir -p "$FAKE_HOME"

export HOME="$FAKE_HOME"
export PATH="$HOME/.local/bin:$PATH"
export RUSTUP_HOME="$REAL_HOME/.rustup"
export CARGO_HOME="$REAL_HOME/.cargo"

echo "=== 1. Testing clean install without voice ==="
$SCRIPT_DIR/install.sh --no-service

echo "=== 2. Verifying installed artifacts ==="
test -x "$HOME/.local/bin/cosmic-mousetrap"
test -f "$HOME/.config/cosmic-mousetrap/config.toml"
test -f "$HOME/.config/cosmic-mousetrap/games.txt"
test -f "$HOME/.config/cosmic-mousetrap/lines.txt"
test -f "$HOME/.local/share/cosmic-mousetrap/on-cue.wav"
test -f "$HOME/.local/share/cosmic-mousetrap/off-cue.wav"
test -f "$HOME/.config/systemd/user/cosmic-mousetrap.service"
echo "All required files installed correctly!"

echo "=== 3. Testing status in fresh environment ==="
"$HOME/.local/bin/cosmic-mousetrap" status

echo "=== 4. Testing voice status fallback behavior ==="
"$HOME/.local/bin/cosmic-mousetrap" voice status

echo "=== 5. Testing on / off / toggle config writes ==="
"$HOME/.local/bin/cosmic-mousetrap" on
setting="$(cat "$HOME/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus")"
test "$setting" = "true"
echo "Turned ON: $setting"

"$HOME/.local/bin/cosmic-mousetrap" off
setting="$(cat "$HOME/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus")"
test "$setting" = "false"
echo "Turned OFF: $setting"

"$HOME/.local/bin/cosmic-mousetrap" toggle
setting="$(cat "$HOME/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus")"
test "$setting" = "true"
echo "Toggled: $setting"

echo "=== 6. Testing voice lines ==="
count="$("$HOME/.local/bin/cosmic-mousetrap" voice lines | grep -E '^  (on|off)' | wc -l)"
echo "Found $count voice lines."
test "$count" -eq 106

echo "=== 7. Testing install with voice pack ==="
$SCRIPT_DIR/install.sh --with-voice --no-service
"$HOME/.local/bin/cosmic-mousetrap" voice status
installed_clips="$(find "$HOME/.local/share/cosmic-mousetrap/voice" -name "*.mp3" | wc -l)"
echo "Installed clips count: $installed_clips"
test "$installed_clips" -eq 106

echo "=== 8. Testing uninstall ==="
$SCRIPT_DIR/uninstall.sh
test ! -f "$HOME/.local/bin/cosmic-mousetrap"
test ! -f "$HOME/.config/systemd/user/cosmic-mousetrap.service"
test -f "$HOME/.config/cosmic-mousetrap/config.toml"
echo "Uninstall kept config as expected!"

echo "=== 9. Testing uninstall --purge ==="
$SCRIPT_DIR/uninstall.sh --purge
test ! -d "$HOME/.config/cosmic-mousetrap"
test ! -d "$HOME/.local/share/cosmic-mousetrap"
echo "Purge wiped config and data directories!"

echo "ALL SIMULATION PHASES PASSED WITH FLYING COLORS!"
