#!/usr/bin/env bash
set -euo pipefail

SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

mkdir -p "$SYSTEMD_USER_DIR"

cp media-control-daemon.service "$SYSTEMD_USER_DIR/"

echo "Installed systemd user service."
echo ""
echo "To enable and start the daemon:"
echo "  systemctl --user daemon-reload"
echo "  systemctl --user enable media-control-daemon.service"
echo "  systemctl --user start media-control-daemon.service"
echo ""
echo "To check status:"
echo "  systemctl --user status media-control-daemon.service"
echo "  journalctl --user -u media-control-daemon.service -f"
