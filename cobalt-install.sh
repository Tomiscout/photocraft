#!/usr/bin/env bash
# cobalt fork: builds this branch in release mode and installs it for the current user as
# "PhotoCraft - Cobalt", next to (not instead of) an official PhotoCraft.
#
#   scripts/cobalt-install.sh              # build and install (run again after every update)
#   scripts/cobalt-install.sh --uninstall  # remove it (keeps its settings folder)
#
# - Window/app id ai.storyteller.photocraft.cobalt (built in through PHOTOCRAFT_APP_ID), so
#   docks and taskbars show Cobalt's own launcher and icon, not the official one.
# - Its own settings, given by PHOTOCRAFT_CONFIG_DIR=~/.config/photocraft-cobalt. That also
#   gives it its own single-instance lock: with a shared one, opening Cobalt while the official
#   PhotoCraft runs would hand the files to the official window.
# - Binaries go in ~/.local/lib/photocraft-cobalt. Wrappers that set the settings folder go in
#   ~/.local/bin (photocraft-cobalt, photocraft-cobalt-cli). The .desktop file and recoloured
#   icons go in ~/.local/share.
# Builds in target/cobalt so the id doesn't force rebuilds of normal (target/) builds. Fonts:
# set CRAFT_FONTS_DIR to a craft-fonts checkout to embed them, as official releases do.
set -euo pipefail

ID=ai.storyteller.photocraft.cobalt
NAME="PhotoCraft - Cobalt"
DATA=${XDG_DATA_HOME:-$HOME/.local/share}
LIB=$HOME/.local/lib/photocraft-cobalt
BIN=$HOME/.local/bin
APPS=$DATA/applications
ICONS=$DATA/icons/hicolor

refresh() {
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
    command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -rf "$LIB"
    rm -f "$BIN/photocraft-cobalt" "$BIN/photocraft-cobalt-cli" "$APPS/$ID.desktop"
    rm -f "$ICONS"/*/apps/"$ID".png
    refresh
    echo "cobalt-install: removed $NAME (settings kept in ${XDG_CONFIG_HOME:-$HOME/.config}/photocraft-cobalt)"
    exit 0
fi
[ $# -eq 0 ] || { echo "usage: $0 [--uninstall]" >&2; exit 2; }

root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
cd "$root"
[ -f ../image-as-audio/crates/cobalt-core/Cargo.toml ] || { echo "cobalt-install: needs the Image as Audio app at ../image-as-audio" >&2; exit 1; }
command -v magick >/dev/null || { echo "cobalt-install: needs ImageMagick (magick) for the icon" >&2; exit 1; }

echo "cobalt-install: building (release, this takes a while)"
PHOTOCRAFT_APP_ID=$ID CARGO_TARGET_DIR=target/cobalt \
    cargo build --release -p photocraft -p photocraft-cli --features heif --bins

mkdir -p "$LIB" "$BIN" "$APPS"
install -m 755 target/cobalt/release/photocraft "$LIB/photocraft"
install -m 755 target/cobalt/release/photocraft-cli "$LIB/photocraft-cli"
for tool in photocraft photocraft-cli; do
    cat > "$BIN/${tool/photocraft/photocraft-cobalt}" <<WRAPPER
#!/bin/sh
# $NAME (installed by scripts/cobalt-install.sh): its own settings and instance lock.
export PHOTOCRAFT_CONFIG_DIR="\${PHOTOCRAFT_CONFIG_DIR:-\${XDG_CONFIG_HOME:-\$HOME/.config}/photocraft-cobalt}"
exec "$LIB/$tool" "\$@"
WRAPPER
    chmod 755 "$BIN/${tool/photocraft/photocraft-cobalt}"
done

# PhotoCraft's icon with its azure background turned violet, so the two are told apart: the hue
# shifts only where the icon is saturated (the background), through a mask of its saturation.
for dir in assets/app-icon/hicolor/*x*; do
    size=${dir##*/}
    src="$dir/apps/ai.storyteller.photocraft.png"
    mkdir -p "$ICONS/$size/apps"
    magick "$src" \( +clone -modulate 100,105,135 \) \
        \( "$src" -colorspace HSL -channel G -separate +channel -level 35%,60% \) -composite "$ICONS/$size/apps/$ID.png"
done

# The official entry, renamed, pointed at the wrapper and the new window class
sed -e "s|^Name=PhotoCraft$|Name=$NAME|" \
    -e "s|^Exec=photocraft %F$|Exec=$BIN/photocraft-cobalt %F|" \
    -e "s|^TryExec=photocraft$|TryExec=$BIN/photocraft-cobalt|" \
    -e "s|^Icon=ai.storyteller.photocraft$|Icon=$ID|" \
    -e "s|^StartupWMClass=ai.storyteller.photocraft$|StartupWMClass=$ID|" \
    -e "s|^Comment=.*|Comment=PhotoCraft with Image as Audio's audio effects (Filter › Audio Effects…)|" \
    -e '/^#/d' -e '/^X-AppImage-Integrate=/d' \
    packaging/linux/ai.storyteller.photocraft.desktop > "$APPS/$ID.desktop"
for key in "Name=$NAME" "Exec=$BIN/photocraft-cobalt %F" "Icon=$ID" "StartupWMClass=$ID"; do
    grep -qxF "$key" "$APPS/$ID.desktop" || { echo "cobalt-install: upstream's .desktop file changed; '$key' was not set" >&2; exit 1; }
done
refresh

echo "cobalt-install: installed $NAME ($(git rev-parse --short HEAD))"
echo "  launcher: $APPS/$ID.desktop"
echo "  command:  $BIN/photocraft-cobalt (CLI: photocraft-cobalt-cli)"
