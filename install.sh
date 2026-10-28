#!/usr/bin/env bash
# Install Mousetrap (cosmic-mousetrap) for the current user (no root needed).
#
# Binary source, in order:
#   1. build from source with cargo        (when run from a git checkout)
#   2. existing binary in target/release   (pre-compiled from cargo)
#   3. a prebuilt binary next to this script (release tarball)
#   4. download the prebuilt GitHub release  (x86_64 only)
#   5. the Python fallback in legacy/        (no sound / custom notifications)
#
# Options:
#   --with-voice   Download or extract the 106-line GLaDOS voice pack
#   --no-service   Install binary and assets only; do not enable systemd service
#   --help, -h     Show usage information
set -euo pipefail

NAME="cosmic-mousetrap"
REPO="${COSMIC_MOUSETRAP_REPO:-Asiphyx/cosmic-mousetrap}"

WITH_VOICE=0
ENABLE_SERVICE=1

for arg in "$@"; do
  case "$arg" in
    --with-voice)
      WITH_VOICE=1
      ;;
    --no-service)
      ENABLE_SERVICE=0
      ;;
    -h|--help)
      echo "Usage: ./install.sh [--with-voice] [--no-service]"
      echo "  --with-voice   Install full 106-clip GLaDOS voice pack"
      echo "  --no-service   Skip enabling/starting the systemd user service"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: ./install.sh [--with-voice] [--no-service]" >&2
      exit 1
      ;;
  esac
done

here="$(cd "$(dirname "$0")" && pwd)"
bin_dst="$HOME/.local/bin/$NAME"
cfg_dir="$HOME/.config/$NAME"
data_dir="$HOME/.local/share/$NAME"
mkdir -p "$HOME/.local/bin" "$HOME/.config/systemd/user" "$cfg_dir" "$data_dir"

# --- migrate from the pre-release name (cosmic-game-mode) ---------------------------
old_unit="$HOME/.config/systemd/user/cosmic-game-mode.service"
if [ -f "$old_unit" ] || [ -e "$HOME/.local/bin/cosmic-game-mode" ]; then
  echo "Migrating from cosmic-game-mode..."
  systemctl --user disable --now cosmic-game-mode.service 2>/dev/null || true
  rm -f "$old_unit" "$HOME/.local/bin/cosmic-game-mode"
  if [ -f "$HOME/.config/cosmic-game-mode/games.txt" ] && [ ! -f "$cfg_dir/games.txt" ]; then
    cp "$HOME/.config/cosmic-game-mode/games.txt" "$cfg_dir/games.txt"
  fi
  systemctl --user daemon-reload 2>/dev/null || true
fi

download_release() {
  [ "$(uname -m)" = "x86_64" ] || return 1
  case "$REPO" in *"<"*) return 1 ;; esac
  local base="https://github.com/$REPO/releases/latest/download"
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$base/$NAME-x86_64-linux" -o "$tmp/bin" || return 1
    curl -fsSL "$base/$NAME-x86_64-linux.sha256" -o "$tmp/sum" || return 1
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$tmp/bin" "$base/$NAME-x86_64-linux" || return 1
    wget -qO "$tmp/sum" "$base/$NAME-x86_64-linux.sha256" || return 1
  else
    return 1
  fi
  local want got
  want="$(cut -d' ' -f1 "$tmp/sum")"
  got="$(sha256sum "$tmp/bin" | cut -d' ' -f1)"
  [ -n "$want" ] && [ "$want" = "$got" ] || { echo "Checksum mismatch; ignoring download." >&2; return 1; }
  install -m 755 "$tmp/bin" "$bin_dst"
}

# --- Install binary -----------------------------------------------------------------
if [ -f "$here/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
  echo "Attempting source build with cargo..."
  if (cd "$here" && cargo build --release --quiet 2>/dev/null); then
    install -m 755 "$here/target/release/$NAME" "$bin_dst"
  fi
fi

if [ ! -f "$bin_dst" ]; then
  if [ -x "$here/target/release/$NAME" ]; then
    echo "Installing existing target/release binary..."
    install -m 755 "$here/target/release/$NAME" "$bin_dst"
  elif [ -x "$here/$NAME" ]; then
    echo "Installing bundled prebuilt binary..."
    install -m 755 "$here/$NAME" "$bin_dst"
  elif download_release; then
    echo "Installed prebuilt release binary."
  elif [ -f "$here/legacy/$NAME.py" ] && command -v python3 >/dev/null 2>&1; then
    echo "Falling back to the Python version (no sound / custom notifications)."
    install -m 755 "$here/legacy/$NAME.py" "$bin_dst"
  else
    echo "Could not install: need cargo (https://rustup.rs), a release download, or python3." >&2
    exit 1
  fi
fi

# --- Install bundled assets ---------------------------------------------------------
if [ -d "$here/assets/sounds" ]; then
  echo "Installing signature audio cues..."
  install -m 644 "$here/assets/sounds/"*.wav "$data_dir/" 2>/dev/null || true
fi

if [ -f "$here/assets/lines.txt" ] && [ ! -f "$cfg_dir/lines.txt" ]; then
  install -m 644 "$here/assets/lines.txt" "$cfg_dir/lines.txt"
fi

if [ "$WITH_VOICE" = "1" ]; then
  mkdir -p "$data_dir/voice"
  echo "Installing GLaDOS voice pack..."
  voice_url="https://github.com/$REPO/releases/latest/download/cosmic-mousetrap-glados-voice.tar.gz"
  if [ -f "$here/cosmic-mousetrap-glados-voice.tar.gz" ]; then
    tar -xzf "$here/cosmic-mousetrap-glados-voice.tar.gz" -C "$data_dir/voice"
    echo "Extracted local voice pack into $data_dir/voice"
  elif command -v curl >/dev/null 2>&1; then
    if curl -fsSL "$voice_url" | tar -xz -C "$data_dir/voice" 2>/dev/null; then
      echo "Downloaded and installed voice pack into $data_dir/voice"
    else
      echo "Note: Could not download voice pack from release (may not be published yet)."
    fi
  elif command -v wget >/dev/null 2>&1; then
    if wget -qO- "$voice_url" | tar -xz -C "$data_dir/voice" 2>/dev/null; then
      echo "Downloaded and installed voice pack into $data_dir/voice"
    else
      echo "Note: Could not download voice pack from release (may not be published yet)."
    fi
  fi
fi

# --- Default configs ----------------------------------------------------------------
if [ ! -f "$cfg_dir/games.txt" ]; then
  cat > "$cfg_dir/games.txt" <<'T'
# Extra games, one per line. Lines without "/" are process names (e.g. mygame.x86_64),
# lines with "/" are path fragments matched against the process exe/cwd (e.g. /Games/Heroic/).
T
fi

if [ ! -f "$cfg_dir/config.toml" ]; then
  cat > "$cfg_dir/config.toml" <<'T'
# Mousetrap settings. Changes apply live (no restart needed).
# Easier: `cosmic-mousetrap sound list | set | test | volume`.

notifications = true      # popup when the mode switches
sound = true              # play a cue when the mode switches
volume = 60               # 0-100

# A preset name (see `cosmic-mousetrap sound list`), a path to your own
# .wav/.ogg/.oga/.opus/.mp3/.flac file, a folder (random file each time), "none" for
# silence, or "voice" for spoken lines (run `cosmic-mousetrap voice download` or
# `voice setup`; until then "voice" falls back gracefully to signature chimes / presets).
sound_on = "voice"
sound_off = "voice"
voice_model = "en_US-amy-medium"

# Notification icons: an icon theme name or an image path.
icon_on = "input-gaming"
icon_off = "input-mouse"
T
fi

# --- Systemd integration ------------------------------------------------------------
if [ -f "$here/systemd/$NAME.service" ]; then
  install -m 644 "$here/systemd/$NAME.service" "$HOME/.config/systemd/user/$NAME.service"
fi

if [ "$ENABLE_SERVICE" = "1" ] && [ -f "$HOME/.config/systemd/user/$NAME.service" ]; then
  if systemctl --user daemon-reload >/dev/null 2>&1; then
    systemctl --user enable "$NAME.service" >/dev/null 2>&1 || true
    systemctl --user restart "$NAME.service" >/dev/null 2>&1 || true
    echo "Service $NAME enabled and restarted."
  else
    echo "Note: systemd user session not available; service file installed but not started."
  fi
fi

echo "Installed. Check with: $NAME status    Try the sounds: $NAME sound test"
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Note: add ~/.local/bin to your PATH to run '$NAME' manually.";; esac
