#!/usr/bin/env bash
set -euo pipefail

NAME="cosmic-mousetrap"
PURGE=0

for arg in "$@"; do
  case "$arg" in
    --purge)
      PURGE=1
      ;;
    -h|--help)
      echo "Usage: ./uninstall.sh [--purge]"
      echo "  --purge   Also delete configuration and cached voice assets"
      exit 0
      ;;
  esac
done

systemctl --user disable --now "$NAME.service" 2>/dev/null || true
"$HOME/.local/bin/$NAME" off --quiet 2>/dev/null || true
rm -f "$HOME/.local/bin/$NAME" "$HOME/.config/systemd/user/$NAME.service"
systemctl --user daemon-reload 2>/dev/null || true

if [ "$PURGE" = "1" ]; then
  rm -rf "$HOME/.config/$NAME" "$HOME/.local/share/$NAME" "$HOME/.cache/$NAME"
  echo "Removed cosmic-mousetrap binary, service, configuration, and data."
else
  echo "Removed cosmic-mousetrap binary and service."
  echo "(Your settings in ~/.config/$NAME/ and voice data in ~/.local/share/$NAME/ were kept; pass --purge to wipe them.)"
fi
