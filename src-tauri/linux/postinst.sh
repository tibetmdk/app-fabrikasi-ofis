#!/bin/sh
set -e

SRC_DESKTOP="/usr/share/applications/App Fabrikasi Ofisi.desktop"
if [ ! -f "$SRC_DESKTOP" ]; then
  SRC_DESKTOP=$(find /usr/share/applications -maxdepth 1 -iname '*fabrikasi*' -name '*.desktop' 2>/dev/null | head -n 1 || true)
fi

if [ -n "$SRC_DESKTOP" ] && [ -f "$SRC_DESKTOP" ]; then
  TARGET_USER="${SUDO_USER:-}"
  if [ -n "$TARGET_USER" ] && [ "$TARGET_USER" != "root" ]; then
    USER_HOME=$(getent passwd "$TARGET_USER" | cut -d: -f6 || true)
    USER_UID=$(id -u "$TARGET_USER" 2>/dev/null || true)
    if [ -n "$USER_HOME" ] && [ -d "$USER_HOME" ]; then
      DESKTOP_DIR=$(su - "$TARGET_USER" -c 'xdg-user-dir DESKTOP' 2>/dev/null || true)
      if [ -z "$DESKTOP_DIR" ] || [ ! -d "$DESKTOP_DIR" ]; then
        if [ -d "$USER_HOME/Desktop" ]; then
          DESKTOP_DIR="$USER_HOME/Desktop"
        elif [ -d "$USER_HOME/Masaüstü" ]; then
          DESKTOP_DIR="$USER_HOME/Masaüstü"
        fi
      fi
      if [ -n "$DESKTOP_DIR" ] && [ -d "$DESKTOP_DIR" ]; then
        DEST="$DESKTOP_DIR/app-fabrikasi-ofis.desktop"
        cp "$SRC_DESKTOP" "$DEST" || true
        chown "$TARGET_USER:$TARGET_USER" "$DEST" 2>/dev/null || true
        chmod +x "$DEST" 2>/dev/null || true
        if [ -n "$USER_UID" ] && [ -S "/run/user/$USER_UID/bus" ]; then
          su - "$TARGET_USER" -c "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$USER_UID/bus gio set '$DEST' metadata::trusted true" 2>/dev/null || true
        fi
      fi
    fi
  fi
fi

update-desktop-database /usr/share/applications 2>/dev/null || true
exit 0
