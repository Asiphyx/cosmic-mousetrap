#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DIR="$(mktemp -d /tmp/cosmic-test-XXXXXX)"
echo "Test environment directory: $TEST_DIR"
trap 'rm -rf "$TEST_DIR"' EXIT

REAL_HOME="$HOME"
FAKE_HOME="$TEST_DIR/user"
mkdir -p "$FAKE_HOME"

export HOME="$FAKE_HOME"
export PATH="$HOME/.local/bin:$PATH"
export RUSTUP_HOME="$REAL_HOME/.rustup"
export CARGO_HOME="$REAL_HOME/.cargo"

echo "=== 1. Testing clean install without voice ==="
"$SCRIPT_DIR/install.sh" --no-service

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

echo "=== 4. Testing sound show and sound list ==="
"$HOME/.local/bin/cosmic-mousetrap" sound show
"$HOME/.local/bin/cosmic-mousetrap" sound list

echo "=== 5. Testing sound test fallback behavior (signature chimes) ==="
"$HOME/.local/bin/cosmic-mousetrap" sound test

echo "=== 6. Testing voice status fallback behavior ==="
"$HOME/.local/bin/cosmic-mousetrap" voice status

echo "=== 7. Testing on / off / toggle config writes ==="
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

echo "=== 8. Testing notifications toggle ==="
"$HOME/.local/bin/cosmic-mousetrap" notifications disable
grep -q '^notifications = false' "$HOME/.config/cosmic-mousetrap/config.toml"
"$HOME/.local/bin/cosmic-mousetrap" notifications enable
grep -q '^notifications = true' "$HOME/.config/cosmic-mousetrap/config.toml"

echo "=== 9. Testing sound set ==="
"$HOME/.local/bin/cosmic-mousetrap" sound set on pop
grep -q '^sound_on = "pop"' "$HOME/.config/cosmic-mousetrap/config.toml"
"$HOME/.local/bin/cosmic-mousetrap" sound set on voice
grep -q '^sound_on = "voice"' "$HOME/.config/cosmic-mousetrap/config.toml"

echo "=== 10. Testing voice lines count ==="
count="$("$HOME/.local/bin/cosmic-mousetrap" voice lines | grep -E '^  (on|off)' | wc -l)"
echo "Found $count voice lines."
test "$count" -eq 106

echo "=== 11. Testing install with voice pack ==="
"$SCRIPT_DIR/install.sh" --with-voice --no-service
"$HOME/.local/bin/cosmic-mousetrap" voice status
installed_clips="$(find "$HOME/.local/share/cosmic-mousetrap/voice" -name "*.mp3" | wc -l)"
echo "Installed clips count: $installed_clips"
test "$installed_clips" -eq 106

echo "=== 12. Testing sound test with voice clips installed ==="
"$HOME/.local/bin/cosmic-mousetrap" sound test

echo "=== 13. Testing self-healing doctor in installed environment ==="
"$HOME/.local/bin/cosmic-mousetrap" doctor
"$HOME/.local/bin/cosmic-mousetrap" doctor --heal

# Inject corrupted setting and stale tmp file, then verify doctor auto-heals them
echo "corrupted_val" > "$HOME/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus"
touch "$HOME/.config/cosmic-mousetrap/stale_test.tmp"
echo -e "volume = 999\nmalformed_broken_line" >> "$HOME/.config/cosmic-mousetrap/config.toml"

echo "Verifying doctor heals corrupted files..."
"$HOME/.local/bin/cosmic-mousetrap" doctor --heal

test "$(cat "$HOME/.config/cosmic/com.system76.CosmicComp/v1/cursor_follows_focus")" = "false"
test ! -f "$HOME/.config/cosmic-mousetrap/stale_test.tmp"
if grep -q "malformed_broken_line" "$HOME/.config/cosmic-mousetrap/config.toml"; then
    echo "Doctor failed to remove malformed line!"
    exit 1
fi
echo "Doctor verified: auto-healed corrupted config, settings, and stale files!"

echo "=== 14. Testing UI non-interactive error handling ==="
if "$HOME/.local/bin/cosmic-mousetrap" ui < /dev/null 2>/dev/null; then
    echo "Expected UI without tty to fail!"
    exit 1
fi
echo "UI correctly required a terminal tty."

echo "=== 15. Testing uninstall ==="
"$SCRIPT_DIR/uninstall.sh"
test ! -f "$HOME/.local/bin/cosmic-mousetrap"
test ! -f "$HOME/.config/systemd/user/cosmic-mousetrap.service"
test -f "$HOME/.config/cosmic-mousetrap/config.toml"
echo "Uninstall kept config as expected!"

echo "=== 16. Testing uninstall --purge ==="
"$SCRIPT_DIR/uninstall.sh" --purge
test ! -d "$HOME/.config/cosmic-mousetrap"
test ! -d "$HOME/.local/share/cosmic-mousetrap"
echo "Purge wiped config and data directories!"

echo "ALL SIMULATION PHASES PASSED WITH FLYING COLORS!"
