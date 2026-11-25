#!/bin/sh
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  TARGET_USER="${SUDO_USER:-}"
  if [ -z "$TARGET_USER" ] && [ -n "${PKEXEC_UID:-}" ] && [ "$PKEXEC_UID" != "0" ]; then
    TARGET_USER=$(getent passwd "$PKEXEC_UID" 2>/dev/null | cut -d: -f1 || true)
  fi
  if [ -n "$TARGET_USER" ] && [ "$TARGET_USER" != "root" ]; then
    USER_HOME=$(getent passwd "$TARGET_USER" | cut -d: -f6 || true)
    if [ -n "$USER_HOME" ]; then
      rm -f "$USER_HOME/Desktop/app-fabrikasi-ofis.desktop" "$USER_HOME/Masaüstü/app-fabrikasi-ofis.desktop" 2>/dev/null || true
    fi
  fi
  update-desktop-database /usr/share/applications 2>/dev/null || true
fi

exit 0
