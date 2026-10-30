#!/usr/bin/env bash
set -euo pipefail

NAME="cosmic-mousetrap"
PURGE=0
NO_SERVICE=0

for arg in "$@"; do
  case "$arg" in
    --purge)
      PURGE=1
      ;;
    --no-service)
      NO_SERVICE=1
      ;;
    -h|--help)
      echo "Usage: ./uninstall.sh [--purge] [--no-service]"
      echo "  --purge       Also delete configuration and cached voice assets"
      echo "  --no-service  Do not touch systemd user services"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: ./uninstall.sh [--purge] [--no-service]" >&2
      exit 1
      ;;
  esac
done

if [ "$NO_SERVICE" = "0" ]; then
  systemctl --user disable --now "$NAME.service" 2>/dev/null || true
fi
"$HOME/.local/bin/$NAME" off --quiet 2>/dev/null || true
rm -f "$HOME/.local/bin/$NAME" "$HOME/.config/systemd/user/$NAME.service"
if [ "$NO_SERVICE" = "0" ]; then
  systemctl --user daemon-reload 2>/dev/null || true
fi

if [ "$PURGE" = "1" ]; then
  rm -rf "$HOME/.config/$NAME" "$HOME/.local/share/$NAME" "$HOME/.cache/$NAME"
  echo "Removed cosmic-mousetrap binary, service, configuration, and data."
else
  echo "Removed cosmic-mousetrap binary and service."
  echo "(Your settings in ~/.config/$NAME/ and voice data in ~/.local/share/$NAME/ were kept; pass --purge to wipe them.)"
fi
