#!/usr/bin/env bash
#
# Installs dictation-tool into the XDG user directories: the binary on PATH,
# the icon in the hicolor theme, a launcher in the applications menu, and the
# models somewhere that survives this checkout being moved or deleted.
#
# Everything lands under $HOME. No root, nothing outside the user's own
# directories, and --uninstall puts it all back.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"
STATE_DIR="$DATA_DIR/dictation-tool"
UNIT="$HOME/.config/systemd/user/dictation-tool.service"
NAME="dictation-tool"

say()  { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33m warning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[31m error:\033[0m %s\n' "$*" >&2; exit 1; }

# ── uninstall ───────────────────────────────────────────────────────────────

if [ "${1:-}" = "--uninstall" ]; then
    say "Removing dictation-tool"
    systemctl --user disable --now "$NAME" 2>/dev/null || true
    pkill -x "$NAME" 2>/dev/null || true
    rm -fv "$BIN_DIR/$NAME" "$APP_DIR/$NAME.desktop" "$ICON_DIR/$NAME.svg" "$UNIT"
    update-desktop-database "$APP_DIR" 2>/dev/null || true
    systemctl --user daemon-reload 2>/dev/null || true
    say "Left alone: $STATE_DIR (models, history, config)"
    echo "    remove it yourself with: rm -rf '$STATE_DIR'"
    exit 0
fi

if [ "${1:-}" = "--help" ]; then sed -n '2,10p' "$0" | sed 's/^# \?//'; exit 0; fi

# ── ROCm: what this GPU needs at runtime ────────────────────────────────────

ROCM="${ROCM_PATH:-/opt/rocm}"

# gfx1151 -> 11.5.1. Only meaningful for all-digit architectures; anything
# else (gfx90a and friends) is left to the caller's own env.
gfx_to_version() {
    local n="${1#gfx}"
    [[ "$n" =~ ^[0-9]+$ ]] || return 1
    printf '%s.%s.%s' "${n:0:${#n}-2}" "${n: -2:1}" "${n: -1}"
}

# rocBLAS ships prebuilt kernels per architecture and simply has none for some
# newer parts (gfx1152, at the time of writing). Inference then dies at the
# first matrix multiply unless the runtime is told to present the GPU as a
# sibling that *is* shipped. Detected here rather than hardcoded so this stays
# correct on a machine that needs no override at all.
detect_override() {
    [ -n "${HSA_OVERRIDE_GFX_VERSION:-}" ] && { printf '%s' "$HSA_OVERRIDE_GFX_VERSION"; return; }
    local arch lib_dir best
    arch="$("$ROCM/bin/offload-arch" 2>/dev/null | head -1)" || return 0
    [ -n "$arch" ] || return 0
    lib_dir="$ROCM/lib/rocblas/library"
    [ -d "$lib_dir" ] || return 0
    ls "$lib_dir" 2>/dev/null | grep -q "$arch" && return 0   # shipped: nothing to do
    # Highest architecture rocBLAS *does* ship that is no newer than ours and
    # in the same family (same first three digits, i.e. same ISA generation).
    best="$(ls "$lib_dir" 2>/dev/null \
        | grep -oE 'gfx[0-9]+' | sort -u \
        | awk -v a="${arch#gfx}" '
            { n = substr($0,4) }
            n+0 <= a+0 && substr(n,1,3) == substr(a,1,3) { if (n+0 > best+0) best = n }
            END { if (best) print "gfx" best }')"
    [ -n "$best" ] || return 0
    gfx_to_version "$best" || return 0
}

# ── build ───────────────────────────────────────────────────────────────────

build() {
    # rustup puts cargo here and wires it up through the shell profile, which
    # a non-interactive or freshly-installed shell has not necessarily read.
    if [ -x "$HOME/.cargo/bin/cargo" ]; then PATH="$HOME/.cargo/bin:$PATH"; fi
    command -v cargo >/dev/null || die "cargo not found; install Rust from https://rustup.rs"
    [ -x "$ROCM/bin/hipcc" ] || die "$ROCM/bin/hipcc not found; install the ROCm HIP SDK (hipcc hip-dev rocblas hipblas)"

    local arch="${AMDGPU_TARGETS:-}"
    if [ -z "$arch" ]; then
        # Build for whatever the runtime will claim to be, not for the silicon:
        # an overridden GPU only loads code objects matching the override.
        local ov; ov="$(detect_override)"
        if [ -n "$ov" ]; then
            arch="gfx$(printf '%s' "$ov" | tr -d '.')"
        else
            arch="$("$ROCM/bin/offload-arch" 2>/dev/null | head -1)"
        fi
    fi
    [ -n "$arch" ] || die "could not determine the GPU architecture; set AMDGPU_TARGETS"

    # hipcc's clang picks the newest GCC directory it can see, which on Ubuntu
    # may be one whose libstdc++ development files were never installed. Point
    # it at the newest one that actually has them, via a wrapper, so this does
    # not depend on the host having libstdc++-N-dev for hipcc's preferred N.
    local shim="" gccdir="" probe
    probe="$(mktemp --suffix=.cpp)"
    printf '#include <cstdio>\nint main(){return 0;}\n' > "$probe"
    if ! "$ROCM/bin/hipcc" "$probe" -o /dev/null >/dev/null 2>&1; then
        for d in /usr/lib/gcc/x86_64-linux-gnu/*/; do
            [ -e "${d}libstdc++.so" ] && gccdir="${d%/}"
        done
        [ -n "$gccdir" ] || die "hipcc cannot link, and no GCC directory with libstdc++.so was found"
        shim="$(mktemp -d)"
        printf '#!/bin/sh\nexec %s/bin/hipcc --gcc-install-dir=%s "$@"\n' "$ROCM" "$gccdir" > "$shim/hipcc"
        chmod +x "$shim/hipcc"
        say "hipcc needs --gcc-install-dir=$gccdir; using a wrapper for this build"
    fi
    rm -f "$probe"

    say "Building for $arch (this takes a few minutes the first time)"
    # LD_PRELOAD/LD_LIBRARY_PATH from the ambient shell get injected into the
    # compiler itself and can crash it; the built binary needs neither.
    ( cd "$REPO" && env -u LD_PRELOAD -u LD_LIBRARY_PATH \
        PATH="${shim:+$shim:}$ROCM/bin:$PATH" AMDGPU_TARGETS="$arch" \
        cargo build --release )
    # Not `[ -n "$shim" ] && rm -rf "$shim"`: with no shim that leaves the
    # function returning 1, which `set -e` takes for a failed build.
    if [ -n "$shim" ]; then rm -rf "$shim"; fi
}

[ "${1:-}" = "--no-build" ] || build
[ -x "$REPO/target/release/$NAME" ] || die "no binary at target/release/$NAME; run without --no-build"

# ── install ─────────────────────────────────────────────────────────────────

say "Installing"
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR" "$STATE_DIR"
install -m755 "$REPO/target/release/$NAME" "$BIN_DIR/$NAME"
install -m644 "$REPO/assets/icons/hicolor/scalable/apps/$NAME.svg" "$ICON_DIR/$NAME.svg"
echo "    $BIN_DIR/$NAME"
echo "    $ICON_DIR/$NAME.svg"

# The launcher carries the runtime override, because nothing else will: a
# desktop launch inherits no login shell, and the systemd unit the app writes
# on first run copies its environment from whatever started it.
OVERRIDE="$(detect_override)"
if [ -n "$OVERRIDE" ]; then
    EXEC="env HSA_OVERRIDE_GFX_VERSION=$OVERRIDE $BIN_DIR/$NAME"
    echo "    runtime override: HSA_OVERRIDE_GFX_VERSION=$OVERRIDE"
else
    EXEC="$BIN_DIR/$NAME"
fi
sed "s|@EXEC@|$EXEC|" "$REPO/assets/$NAME.desktop.in" > "$APP_DIR/$NAME.desktop"
chmod 644 "$APP_DIR/$NAME.desktop"
echo "    $APP_DIR/$NAME.desktop"

# Models are the one large, user-supplied part. Moved rather than copied, so
# a 1.5 GB model is not duplicated, and only when the destination is empty.
if [ ! -d "$STATE_DIR/models" ] && [ -d "$REPO/whisper.cpp/models" ]; then
    mv "$REPO/whisper.cpp/models" "$STATE_DIR/models"
    say "Moved models to $STATE_DIR/models"
elif [ ! -d "$STATE_DIR/models" ]; then
    mkdir -p "$STATE_DIR/models"
    warn "no models found; fetch one into $STATE_DIR/models (see README)"
fi

update-desktop-database "$APP_DIR" 2>/dev/null || true
gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) warn "$BIN_DIR is not on your PATH; the launcher works regardless, but '$NAME' from a shell will not" ;;
esac

say "Done. Find \"Dictation\" in your applications, or run: $NAME"
echo "    Closing the window leaves it in the status tray; quit from there."
