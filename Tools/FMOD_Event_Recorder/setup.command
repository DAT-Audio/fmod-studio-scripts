#!/bin/bash
# ============================================================
#  FMOD Event Recorder — one-click setup (macOS)
#  Version: 0.6.5 (DAT-audio Utilities pack)
#
#  Double-click this file in Finder, or run:  ./setup.command
#  (add -y to skip the prompts / auto-confirm).
#
#  It finds your Python 3, sets up a local environment with the libraries the
#  tool needs, and clears macOS quarantine on the FMOD libraries you added.
#  Needs the internet once (to download the Python libraries). You install
#  Python yourself and add the FMOD Engine libraries to libs/macos/ first:
#  they aren't included, because FMOD doesn't allow tool makers to share them
#  (see libs/macos/PUT_MACOS_DYLIBS_HERE.txt).
# ============================================================

set -u
# This script lives inside Tools/FMOD_Event_Recorder/, so its own folder IS the
# tool folder. Resolve everything relative to it.
cd "$(dirname "$0")" || exit 1
SELF="$(pwd)"
VENV="$SELF/venv"
REQ="$SELF/requirements.txt"
LIBS="$SELF/libs/macos"

ASSUME_YES=0
case "${1:-}" in -y|--yes) ASSUME_YES=1 ;; esac

pause_exit() { code="${1:-0}"; if [ "$ASSUME_YES" -ne 1 ]; then printf "\nPress Return to close.\n"; read -r _; fi; exit "$code"; }

# Import-check the environment. Prints a one-line summary on success (stdout);
# returns non-zero if anything's missing or broken.
verify_install() {
    "$VENV/bin/python" - "$LIBS" <<'PYEOF'
import os, sys
libs = sys.argv[1]
os.environ["PYFMODEX_DLL_PATH"]        = os.path.join(libs, "libfmod.dylib")
os.environ["PYFMODEX_STUDIO_DLL_PATH"] = os.path.join(libs, "libfmodstudio.dylib")
from pyfmodex.studio import StudioSystem   # noqa: F401
import numpy, soundfile
try:
    import pyloudnorm  # noqa: F401
    lufs = "available"
except Exception:
    lufs = "not installed (LUFS normalization off)"
print("  pyfmodex OK  |  numpy %s  |  soundfile %s  |  LUFS: %s"
      % (numpy.__version__, soundfile.__version__, lufs))
PYEOF
}

print_next_steps() {
    echo ""
    echo "Next up:"
    echo "  1. Make sure the DAT-audio Utilities Scripts/ folder sits next to this"
    echo "     Tools/ folder. Per machine: ~/Library/Application Support/FMOD Studio/"
    echo "     for FMOD 2.02, ~/Library/Preferences/FMOD Studio/ for FMOD 2.03"
    echo "     (or both via symlinks; see the top-level README)."
    echo "  2. In FMOD Studio:  Scripts → Reload  →  DAT-audio Utilities → Record Selected Events…"
}

echo "┌───────────────────────────────────────────────┐"
echo "│      FMOD Event Recorder — Setup (macOS)       │"
echo "└───────────────────────────────────────────────┘"
echo "  Event Recorder v0.6.5"
echo ""

# ── 1. Find Python 3 ──────────────────────────────────────────
PY="$(command -v python3 || true)"
if [ -z "$PY" ]; then
    echo "❌ Python 3 was not found."
    echo "   Install it, then run this again:"
    echo "  • https://www.python.org/downloads/   (or)"
    echo "  • brew install python@3.13"
    pause_exit 1
fi
PYVER="$("$PY" -c 'import sys;print("%d.%d.%d"%sys.version_info[:3])' 2>/dev/null)"
if ! "$PY" -c 'import sys; raise SystemExit(0 if sys.version_info[:2] >= (3,9) else 1)' 2>/dev/null; then
    echo "❌ Python 3.9 or newer is required (found $PYVER)."
    pause_exit 1
fi
echo "Found Python $PYVER at: $PY"

# ── 1b. Check the FMOD libraries are in place ─────────────────
# They aren't shipped with the tool (FMOD doesn't allow tool makers to share
# them), so each user adds their own before running setup.
if [ ! -f "$LIBS/libfmod.dylib" ] || [ ! -f "$LIBS/libfmodstudio.dylib" ]; then
    echo ""
    echo "❌ The FMOD Engine libraries aren't in place yet."
    echo "   They aren't included with this tool: FMOD's position is that tool and"
    echo "   script makers must not publish or share them, so you add your own copy."
    echo ""
    echo "   1. Sign in at https://www.fmod.com/download (free) and download the"
    echo "      FMOD Engine for Mac (2.03.x recommended)."
    echo "   2. From the download, copy these two files:"
    echo "        api/core/lib/libfmod.dylib"
    echo "        api/studio/lib/libfmodstudio.dylib"
    echo "      into:"
    echo "        $LIBS/"
    echo "   3. Run this setup again."
    echo ""
    echo "   Details: libs/macos/PUT_MACOS_DYLIBS_HERE.txt, or README.md (Setup, step 2)."
    pause_exit 1
fi
echo "FMOD libraries found in libs/macos/."

# ── 2. Check for an existing install ────────────────────────────────────
SKIP_CONFIRM=0
if [ -d "$VENV" ]; then
    SUMMARY="$(verify_install 2>/dev/null)"
    if [ $? -eq 0 ]; then
        echo ""
        echo "Looks like you're already set up 👍"
        echo "$SUMMARY"
        # keep the FMOD libs unquarantined (cheap, harmless if already clear)
        xattr -dr com.apple.quarantine "$LIBS" 2>/dev/null || true
        if [ "$ASSUME_YES" -eq 1 ]; then
            echo ""
            echo "Nothing to do — you're ready."
            print_next_steps
            pause_exit 0
        fi
        printf "\nEverything's already working. Reinstall from scratch anyway? [y/N] "
        read -r ans
        case "$ans" in
            y|Y|yes|YES) echo "OK — reinstalling fresh…"; SKIP_CONFIRM=1 ;;
            *) echo "OK, leaving as-is."; print_next_steps; pause_exit 0 ;;
        esac
    fi
fi

# ── 3. Confirm (unless already agreed to reinstall) ───────────
if [ "$SKIP_CONFIRM" -ne 1 ] && [ "$ASSUME_YES" -ne 1 ]; then
    echo ""
    echo "Steps to install:"
    echo "  • set up a local Python environment in Tools/FMOD_Event_Recorder/venv"
    echo "  • download the libraries the tool needs (this is the part that needs internet)"
    echo "  • clear macOS quarantine on the FMOD libraries you added"
    echo ""
    printf "Ready to go? [y/N] "
    read -r ans
    case "$ans" in y|Y|yes|YES) ;; *) echo "No problem — cancelled."; pause_exit 1 ;; esac
fi

# ── 4. Build the environment ──────────────────────────────────
if [ -d "$VENV" ]; then rm -rf "$VENV"; fi
echo ""
echo "Setting up the local Python environment…"
if ! "$PY" -m venv "$VENV"; then
    echo "❌ Failed to create the virtual environment."
    pause_exit 1
fi

echo "Installing libraries (this can take a minute)..."
"$VENV/bin/python" -m pip install --upgrade pip >/dev/null 2>&1
if ! "$VENV/bin/python" -m pip install -r "$REQ"; then
    echo "❌ Library installation failed. Check your internet connection and try again."
    pause_exit 1
fi

# ── 5. Clear quarantine + verify ──────────────────────────────
xattr -dr com.apple.quarantine "$LIBS" 2>/dev/null || true

echo "Checking everything loads…"
SUMMARY="$(verify_install 2>&1)"
if [ $? -ne 0 ]; then
    echo "❌ Verification failed — see the errors above."
    echo "$SUMMARY"
    pause_exit 1
fi
echo "$SUMMARY"

# ── Done ──────────────────────────────────────────────────────
echo ""
echo "✅ Everything installed correctly"
print_next_steps
pause_exit 0
