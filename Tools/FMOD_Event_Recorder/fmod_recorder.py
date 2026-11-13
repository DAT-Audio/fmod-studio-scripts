#!/usr/bin/env python3
# =============================================================
#  FMOD Event Recorder — Python Renderer
#  Version: 0.6.5 (DAT-audio Utilities pack)
#  Lives in: Tools/FMOD_Event_Recorder/ — next to the Scripts/ folder the pack is
#  installed in (per machine or per project). Everything here resolves relative
#  to this file. Requires Python 3.9+; dependencies go in ./venv via setup.command
#  (macOS) or setup.bat (Windows). FMOD libraries: ./libs/<platform>/.
# =============================================================

__version__ = "0.6.5"   # matches Event Recorder.js; see the pack's top-level README

import sys
import os
import re
import glob
import json
import time
import ctypes
from pathlib import Path

# ── Console / output safety ───────────────────────────────────
# When FMOD Studio (a GUI app) launches us, our stdout is an unread pipe using
# the OS locale codepage (e.g. cp1252 on Windows). Two things then break the
# run: printing non-ASCII chars (the "─" separators) raises UnicodeEncodeError,
# and an unread pipe can fill and block print() — either way we'd die after the
# last WAV but before writing results. If we're not on a real console, redirect
# all output to a UTF-8 log file so nothing print()s can crash or stall the run.
try:
    _on_console = bool(sys.stdout) and sys.stdout.isatty()
except Exception:
    _on_console = False
if not _on_console:
    try:
        _logf = open(Path(__file__).resolve().parent / "recorder_console.log",
                     "w", encoding="utf-8")
        sys.stdout = _logf
        sys.stderr = _logf
    except Exception:
        pass

# ── Read manifest ─────────────────────────────────────────────

# Manifest path: from argv if given, otherwise default to
# recorder_manifest.json sitting next to this script. The default lets the
# Studio launcher run us with a single un-quoted argument (the script path),
# which behaves identically on macOS and Windows and avoids shell/quoting pitfalls.
if len(sys.argv) >= 2:
    manifest_path = sys.argv[1]
else:
    manifest_path = str(Path(__file__).resolve().parent / "recorder_manifest.json")

if not os.path.exists(manifest_path):
    print("[Recorder] ERROR: manifest not found: " + manifest_path)
    sys.exit(1)

with open(manifest_path) as f:
    manifest = json.load(f)

output_path  = manifest["outputPath"]
fmt          = manifest["format"].upper()       # WAV / AIFF / FLAC
sample_rate  = manifest["sampleRate"]           # 44100 or 48000
num_channels = manifest["channels"]             # 1 or 2
bank_paths   = manifest["bankPaths"]
event_paths  = manifest["events"]
max_duration = manifest.get("maxDuration", 10.0)  # seconds, for events with no fixed end
sustain_hold = manifest.get("sustainHold", 1.0)   # seconds to hold at each sustain point before key-off
num_takes    = manifest.get("numTakes", 1)         # recordings per event
# Optional loudness/peak normalization (off by default):
#   {"enabled": bool, "peakDb": float|None, "lufs": float|None}
normalize_cfg = manifest.get("normalize") or {}

print(f"[Recorder] Version:  {__version__}")
print(f"[Recorder] Output:   {output_path}")
print(f"[Recorder] Format:   {fmt}  {sample_rate}Hz  {'Stereo' if num_channels == 2 else 'Mono'}")
print(f"[Recorder] Takes:    {num_takes}")
if normalize_cfg.get("enabled"):
    _nl = normalize_cfg.get("lufs"); _np = normalize_cfg.get("peakDb")
    print(f"[Recorder] Normalize: LUFS-I={_nl if _nl is not None else 'off'}  peak={_np if _np is not None else 'off'} dBFS")
print(f"[Recorder] Banks:    {len(bank_paths)}")
print(f"[Recorder] Events:   {len(event_paths)}")

# Check once, up front, whether the built banks exist where the project's build
# settings say. Nothing loading at all almost always means the banks haven't been
# built on this machine (or the build output directory points elsewhere), and
# that should be reported as one clear message, not as N "event not found"s.
_bank_files = [bp for bp in bank_paths if not bp.endswith(".strings.bank")]
_bank_found = [bp for bp in _bank_files if os.path.exists(bp)]
print(f"[Recorder] Banks on disk: {len(_bank_found)} of {len(_bank_files)}")
bank_error = None
if _bank_files and not _bank_found:
    bank_error = ("No built banks were found (looked in "
                  f"{os.path.dirname(_bank_files[0])}). Build the project's banks "
                  "(File > Build), and check its build output directory "
                  "(Preferences > Build).")
    print(f"[Recorder] ERROR: {bank_error}")

# ── Locate the FMOD libraries (cross-platform) ────────────────
# macOS: libs/macos/{libfmod.dylib, libfmodstudio.dylib}
# Windows: libs/windows/{fmod.dll, fmodstudio.dll}

SCRIPT_DIR = Path(__file__).parent.resolve()
IS_WINDOWS = sys.platform.startswith("win")
IS_MAC     = (sys.platform == "darwin")

if IS_WINDOWS:
    PLATFORM_SUBDIR = "windows"
    CORE_LIB_NAME, STUDIO_LIB_NAME = "fmod.dll", "fmodstudio.dll"
    _SYSTEM_LIB_DIRS = []
else:
    PLATFORM_SUBDIR = "macos"
    CORE_LIB_NAME, STUDIO_LIB_NAME = "libfmod.dylib", "libfmodstudio.dylib"
    _SYSTEM_LIB_DIRS = [Path("/usr/local/lib")]

# Windows: the DLL arch must match the running Python. Prefer an arch subfolder
# (libs/windows/x64 or /arm64) so one bundle can serve both; fall back to a flat
# libs/windows for a simple single-arch drop-in.
if IS_WINDOWS:
    import platform
    _arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"

def _platform_lib_dirs(libs_base):
    """Platform-specific subdirs under a libs/ base (arch-aware on Windows)."""
    if IS_WINDOWS:
        return [libs_base / "windows" / _arch, libs_base / "windows"]
    return [libs_base / "macos"]   # macOS dylibs are universal — one flat folder

# Library search order (first match wins). The FMOD libraries are NOT shipped
# with the tool (FMOD doesn't allow tool makers to redistribute them): users add
# their own to the tool's libs/ folder, which is the default; the earlier
# entries let several tools share one copy of the FMOD libraries when you want:
#   1. $FMOD_LIB_DIR          — central/shared location (environment variable)
#   2. manifest "fmodLibDir"  — explicit per-run override
#   3. <project>/Tools/libs/  — project-level shared folder, if present
#   4. Tools/FMOD_Event_Recorder/libs/  — the default, where users add them
#   5. system library dirs
search_dirs = []
_env_lib = os.environ.get("FMOD_LIB_DIR")
if _env_lib:
    search_dirs.append(Path(_env_lib))                  # point straight at the libs…
    search_dirs += _platform_lib_dirs(Path(_env_lib))   # …or at a libs/ base
if manifest.get("fmodLibDir"):
    search_dirs.append(Path(manifest["fmodLibDir"]))
search_dirs += _platform_lib_dirs(SCRIPT_DIR.parent / "libs")   # <project>/Tools/libs/
search_dirs += _platform_lib_dirs(SCRIPT_DIR / "libs")          # this tool's libs/
search_dirs += _SYSTEM_LIB_DIRS

def find_lib(name):
    for d in search_dirs:
        p = d / name
        if p.exists():
            return str(p)
    return None

core_lib   = find_lib(CORE_LIB_NAME)
studio_lib = find_lib(STUDIO_LIB_NAME)

if not core_lib or not studio_lib:
    print(f"[Recorder] ERROR: FMOD libraries not found ({CORE_LIB_NAME} / {STUDIO_LIB_NAME}).")
    print("  Searched:")
    for d in search_dirs:
        print(f"    {d}")
    print(f"  Put {CORE_LIB_NAME} + {STUDIO_LIB_NAME} in Tools/FMOD_Event_Recorder/libs/{PLATFORM_SUBDIR}/")
    print(f"  (or share them via Tools/libs/{PLATFORM_SUBDIR}/ or the FMOD_LIB_DIR environment variable)")
    print("  They aren't included with the tool: FMOD doesn't allow tool makers to share them.")
    print("  Download the FMOD Engine from https://www.fmod.com/download (free account) and")
    print(f"  follow libs/{PLATFORM_SUBDIR}/PUT_*_HERE.txt, or README.md (Setup, step 2).")
    sys.exit(1)

print(f"[Recorder] Core lib:   {core_lib}")
print(f"[Recorder] Studio lib: {studio_lib}")

# FMOD's create-time header check only requires the major.minor to match the
# loaded library — but pyfmodex hardcodes a 2.02 header, which throws a
# HEADER MISMATCH against a 2.03 engine. The engine in libs/ may be 2.02 or 2.03,
# so probe the library for its real version and pass a matching <major.minor.0>
# header when creating the Studio system (via StudioSystem(version=...)).
def detect_fmod_header_version(core_lib_path):
    try:
        probe = ctypes.CDLL(core_lib_path)
    except Exception:
        return None
    for cand in (0x00020300, 0x00020200, 0x00020400, 0x00020100, 0x00020000):
        ptr = ctypes.c_void_p()
        try:
            if probe.FMOD_System_Create(ctypes.byref(ptr), ctypes.c_uint(cand)) == 0:
                ver = ctypes.c_uint()
                probe.FMOD_System_GetVersion(ptr, ctypes.byref(ver))
                try:
                    probe.FMOD_System_Release(ptr)
                except Exception:
                    pass
                v = ver.value
                return (((v >> 16) & 0xFFFF) << 16) | (((v >> 8) & 0xFF) << 8)
        except Exception:
            pass
    return None

FMOD_HEADER_VERSION = detect_fmod_header_version(core_lib)
if FMOD_HEADER_VERSION:
    print(f"[Recorder] Lib version:  {(FMOD_HEADER_VERSION>>16)&0xFFFF}.{(FMOD_HEADER_VERSION>>8)&0xFF:02d}.xx  (header 0x{FMOD_HEADER_VERSION:08X})")

# Some banks use plugin effects (AudioGaming, Resonance, etc.). Without those
# plugins registered, the bank fails to load ("PLUGIN MISSING") and its events
# become unrecordable. Gather plugin directories to load before banks: the ones
# the Studio script passes in the manifest (project Plugins/ and the running
# Studio app's own plugins folder), then standard install locations as a
# fallback for manual runs.
PLUGIN_EXT = "*.dll" if IS_WINDOWS else "*.dylib"

def gather_plugin_dirs():
    dirs = list(manifest.get("pluginDirs") or [])
    maj = (FMOD_HEADER_VERSION >> 16) & 0xFFFF if FMOD_HEADER_VERSION else 2
    minr = (FMOD_HEADER_VERSION >> 8) & 0xFF if FMOD_HEADER_VERSION else 2
    if IS_WINDOWS:
        # Fallback: standard FMOD Studio install locations on Windows; prefer the version-
        # matched install (newest first), then any, under Program Files (x86 too).
        for base in (r"C:\Program Files\FMOD SoundSystem",
                     r"C:\Program Files (x86)\FMOD SoundSystem"):
            dirs += sorted(glob.glob(base + f"\\FMOD Studio {maj}.{minr:02d}*\\plugins"), reverse=True)
            dirs += sorted(glob.glob(base + "\\FMOD Studio*\\plugins"), reverse=True)
    else:
        # Fallback: standard macOS app location; prefer version-matched (newest patch first).
        dirs += sorted(glob.glob(f"/Applications/FMOD Studio/FMOD {maj}_{minr:02d}_*.app/Contents/Plugins"), reverse=True)
        dirs += sorted(glob.glob("/Applications/FMOD Studio/FMOD*.app/Contents/Plugins"), reverse=True)
    out, seen = [], set()
    for d in dirs:
        ad = os.path.abspath(d)
        if ad not in seen and os.path.isdir(ad):
            seen.add(ad)
            out.append(ad)
    return out

PLUGIN_DIRS = gather_plugin_dirs()

def load_fmod_plugins(core):
    """Register FMOD plugins so banks that use plugin effects will load. Loads
    every plugin library found in PLUGIN_DIRS (deduped by filename); unused or
    incompatible plugins simply fail to register and are skipped."""
    loaded, seen = 0, set()
    for d in PLUGIN_DIRS:
        for pl in sorted(glob.glob(os.path.join(d, PLUGIN_EXT))):
            name = os.path.basename(pl)
            if name in seen:
                continue
            seen.add(name)
            try:
                core.load_plugin(pl, 0)
                loaded += 1
            except Exception:
                pass
    if loaded:
        print(f"[Recorder]   Loaded {loaded} FMOD plugin(s)")
    return loaded

# Tell pyfmodex where to find the libraries (must be set before import)
os.environ["PYFMODEX_DLL_PATH"]        = core_lib
os.environ["PYFMODEX_STUDIO_DLL_PATH"] = studio_lib

# ── Import pyfmodex ───────────────────────────────────────────

try:
    from pyfmodex.studio import StudioSystem
    from pyfmodex.studio.flags import STUDIO_INIT_FLAGS, LOAD_BANK_FLAGS
    from pyfmodex.studio.enums import PLAYBACK_STATE, LOADING_STATE
    from pyfmodex.flags import INIT_FLAGS
    from pyfmodex.enums import OUTPUTTYPE, SPEAKERMODE
    from pyfmodex.structobject import Structobject as so
    from pyfmodex.structures import GUID
    from pyfmodex.studio.event_description import EventDescription
except ImportError as e:
    print(f"[Recorder] ERROR: pyfmodex not importable: {e}")
    sys.exit(1)

# prepare_str encodes a Python str to the bytes FMOD's C API expects.
# pyfmodex uses it internally; import it, with a safe fallback.
try:
    from pyfmodex.utils import prepare_str
except Exception:
    def prepare_str(s, encoding="utf-8"):
        return s.encode(encoding) if isinstance(s, str) else s

# ── Setup ─────────────────────────────────────────────────────

os.makedirs(output_path, exist_ok=True)

speaker_mode = SPEAKERMODE.STEREO if num_channels == 2 else SPEAKERMODE.MONO

DSP_BUFFER_SIZE = 1024   # samples per update step
step_time = DSP_BUFFER_SIZE / sample_rate

# Silent gap written between takes so the combined WAV can be sliced cleanly.
#
# The frame→sample mapping used for slicing accumulates a small error over long
# takes (~1%), so a take's computed end point can drift a few hundred ms past
# the true boundary. If the inter-take gap is tiny, that drift bleeds the start
# of the next take onto the end of the current one. A generous silent gap (~2.5s)
# absorbs the drift: each take's slice ends *within silence* (removed by per-slice
# trimming) instead of into the next take's audio. The pad is pure silence (no
# instance playing) and is trimmed away, so it costs nothing in the output.
# Frames write ~actual_spf (~500) samples each, so we size in frames accordingly.
SILENCE_PAD_FRAMES = max(2, int(2.5 * sample_rate / 512))  # ~2.5 s of silence

def sanitize(event_path):
    """event:/Folder/Name  →  Folder_Name"""
    return event_path.replace("event:/", "").replace("/", "_")

def sanitize_token(s):
    """Make a parameter name or label safe for a filename fragment."""
    out = "".join(c if (c.isalnum() or c in "-.") else "" for c in str(s))
    return out or "x"

def parse_guid(guid_str):
    """Parse an FMOD GUID string '{8-4-4-4-12}' into a pyfmodex GUID struct,
    or return None if it isn't a valid GUID. Looking events up by GUID reads
    them straight from the loaded metadata bank and needs NO strings bank."""
    if not guid_str:
        return None
    m = re.fullmatch(
        r"\{?([0-9A-Fa-f]{8})-([0-9A-Fa-f]{4})-([0-9A-Fa-f]{4})-([0-9A-Fa-f]{4})-([0-9A-Fa-f]{12})\}?",
        str(guid_str).strip())
    if not m:
        return None
    g = GUID()
    g.data1 = int(m.group(1), 16)
    g.data2 = int(m.group(2), 16)
    g.data3 = int(m.group(3), 16)
    g.data4 = bytes.fromhex(m.group(4) + m.group(5))
    return g

# ── Parameter application ─────────────────────────────────────
#
# Global parameters (isGlobal) must be set on the Studio System; local
# parameters are set on the event instance. pyfmodex only wraps the instance
# setter, so global params go through the generic _call() which invokes
# FMOD_Studio_System_SetParameterByName on the system pointer.

def set_continuous_param(studio, instance, name, value, is_global):
    """Set a numeric (continuous) parameter value."""
    if is_global:
        studio._call("SetParameterByName", prepare_str(name), ctypes.c_float(float(value)), 0)
    else:
        instance.set_parameter_by_name(name, float(value))

def set_labeled_param(studio, instance, name, label, is_global, labels):
    """Set a labeled parameter by its label string. Falls back to setting the
    label's index if the WithLabel C function isn't available on this FMOD build."""
    try:
        if is_global:
            studio._call("SetParameterByNameWithLabel", prepare_str(name), prepare_str(label), 0)
        else:
            instance._call("SetParameterByNameWithLabel", prepare_str(name), prepare_str(label), 0)
    except Exception:
        idx = labels.index(label) if (labels and label in labels) else 0
        set_continuous_param(studio, instance, name, idx, is_global)

def build_render_slots(event_cfg):
    """Expand an event's parameter config into a flat list of render slots.

    Continuous params are constant across all slots. Each selected label of a
    labeled param multiplies the slot count (cross product across labeled
    params), and each combination is then repeated `takes` times.

    Returns (slots, continuous_params, labeled_params) where each slot is:
        {"labels": {paramName: labelString, ...}, "suffix": "<filename fragment>"}
    """
    params  = event_cfg.get("params", []) or []
    n_takes = max(1, int(event_cfg.get("takes", 1)))

    continuous = [p for p in params if not p.get("labeled")]
    labeled    = [p for p in params if p.get("labeled") and p.get("selected")]

    # Cross product of selected labels across all labeled params.
    combos = [{}]
    for lp in labeled:
        expanded = []
        for base in combos:
            for lab in lp["selected"]:
                nc = dict(base)
                nc[lp["name"]] = lab
                expanded.append(nc)
        combos = expanded

    slots = []
    for combo in combos:
        label_suffix = ""
        for lp in labeled:
            if lp["name"] in combo:
                label_suffix += "_" + sanitize_token(lp["name"]) + "-" + sanitize_token(combo[lp["name"]])
        for t in range(n_takes):
            take_suffix = f"_{t + 1:02d}" if n_takes > 1 else ""
            slots.append({"labels": combo, "suffix": label_suffix + take_suffix})

    return slots, continuous, labeled

# ── Trim silence ──────────────────────────────────────────────
#
# Leading: -96 dB (16-bit noise floor) removes only true digital silence,
#   preserving soft attacks and quiet instrument layers.
# Trailing: -60 dB catches inaudible reverb/decay tails.

def trim_silence(wav_path, lead_db=-96, trail_db=-60):
    """Trim leading digital silence and trailing inaudible decay in one pass.
    Returns (lead_ms_trimmed, trail_ms_trimmed)."""
    try:
        import soundfile as sf
        import numpy as np
        data, sr = sf.read(wav_path)
        abs_data = np.abs(data) if data.ndim == 1 else np.abs(data).max(axis=1)

        lead_thresh  = 10 ** (lead_db  / 20.0)
        trail_thresh = 10 ** (trail_db / 20.0)

        leading  = np.where(abs_data > lead_thresh)[0]
        trailing = np.where(abs_data > trail_thresh)[0]

        start = leading[0]  if len(leading)  > 0 else 0
        end   = trailing[-1] + 1 if len(trailing) > 0 else len(data)

        lead_ms  = round(start / sr * 1000)
        trail_ms = round((len(data) - end) / sr * 1000)

        if start != 0 or end != len(data):
            sf.write(wav_path, data[start:end], sr)
            parts = []
            if lead_ms  > 0: parts.append(f"{lead_ms}ms leading")
            if trail_ms > 0: parts.append(f"{trail_ms}ms trailing")
            if parts:
                print(f"[Recorder]   Trimmed {', '.join(parts)} silence")

        return lead_ms, trail_ms
    except Exception as e:
        print(f"[Recorder]   WARNING: could not trim silence: {e}")
        return 0, 0

# ── Optional loudness / peak normalization ────────────────────
#
# Off by default. Two independent controls:
#   lufs  — integrated loudness target (LUFS, BS.1770-4 via pyloudnorm).
#   peakDb — sample-peak level in dBFS.
# When both are set: loudness-normalize to `lufs`, then pull the gain DOWN if
# the peak would exceed `peakDb` (never boost past the ceiling). When only
# peakDb is set: peak-normalize to that level. A hard 0 dBFS safety limit always
# applies so output never clips. Operates in-place on the WAV before conversion.

def normalize_wav(wav_path, cfg):
    if not cfg or not cfg.get("enabled"):
        return None
    lufs_target = cfg.get("lufs")
    peak_target = cfg.get("peakDb")
    if lufs_target is None and peak_target is None:
        return None
    try:
        import soundfile as sf
        import numpy as np

        info = sf.info(wav_path)
        data, sr = sf.read(wav_path)
        if data.size == 0:
            return None

        def peak_db(x):
            pk = float(np.max(np.abs(x)))
            return (20.0 * np.log10(pk)) if pk > 0 else float("-inf")

        gain_db       = 0.0
        measured_lufs = None
        limited       = False

        if lufs_target is not None:
            try:
                import pyloudnorm as pyln
                measured_lufs = pyln.Meter(sr).integrated_loudness(data)
            except ImportError:
                print("[Recorder]   normalize: pyloudnorm not installed — LUFS skipped "
                      "(venv/bin/python -m pip install pyloudnorm). Peak still applied if set.")
                measured_lufs = None
            if measured_lufs is not None and np.isfinite(measured_lufs):
                gain_db = lufs_target - measured_lufs
            gained  = data * (10 ** (gain_db / 20.0))
            ceiling = peak_target if peak_target is not None else 0.0   # 0 dBFS safety
            pk = peak_db(gained)
            if np.isfinite(pk) and pk > ceiling:
                gained *= 10 ** ((ceiling - pk) / 20.0)
                gain_db += (ceiling - pk)
                limited = True
        else:
            # peak-only: normalize the sample peak to peak_target
            pk = peak_db(data)
            gain_db = (peak_target - pk) if np.isfinite(pk) else 0.0
            gained  = data * (10 ** (gain_db / 20.0))

        # Absolute clip safety (should be a no-op given the logic above).
        mx = float(np.max(np.abs(gained)))
        if mx > 1.0:
            gained /= mx

        sf.write(wav_path, gained, sr, subtype=info.subtype)

        parts = [f"gain {gain_db:+.1f} dB"]
        if measured_lufs is not None and np.isfinite(measured_lufs):
            parts.append(f"{measured_lufs:.1f}→{lufs_target} LUFS")
        parts.append(f"peak {peak_db(gained):.1f} dBFS")
        if limited:
            parts.append("(peak-limited)")
        print(f"[Recorder]   normalized: {', '.join(parts)}")
        return {"gainDb": round(gain_db, 2),
                "measuredLufs": round(measured_lufs, 2) if (measured_lufs is not None and np.isfinite(measured_lufs)) else None}
    except Exception as e:
        print(f"[Recorder]   WARNING: normalization failed: {e}")
        return None

# ── Optional format conversion (AIFF / FLAC / MP3) ─────────────

def convert(src_wav, dst_path, target_fmt):
    """Convert the temp WAV at src_wav to dst_path (target_fmt), then delete the
    temp WAV. src_wav is always a private intermediate, never a user-facing file."""
    if target_fmt == "WAV":
        return True
    try:
        import soundfile as sf
        data, sr = sf.read(src_wav)
        sf.write(dst_path, data, sr)
        if os.path.abspath(src_wav) != os.path.abspath(dst_path) and os.path.exists(src_wav):
            os.remove(src_wav)
        print(f"[Recorder]   Converted → {os.path.basename(dst_path)}")
        return True
    except Exception as e:
        print(f"[Recorder]   WARNING: conversion to {target_fmt} failed: {e}")
        print(f"             intermediate WAV kept at {src_wav}")
        return False

# ── Render all takes for one event ───────────────────────────
#
# All takes are rendered inside a SINGLE StudioSystem so that FMOD's internal
# PRNG advances between takes. This is the key requirement for genuine variation
# in multi-instrument random selection: a fresh StudioSystem always resets to
# the same seed, producing identical takes.
#
# WAVWRITER_NRT writes one output file per system, so we render all takes into
# one combined temp WAV, track per-take frame boundaries, then slice it into
# separate files with soundfile.

def render_event_takes(event_cfg):
    """Render one event. event_cfg = {"path", "takes", "params"}.
    Returns a list of result dicts, one per render slot."""
    event_path   = event_cfg["path"]
    base_name    = sanitize(event_path)
    combined_wav = os.path.join(output_path, base_name + "_tmp_combined.wav")

    slots, continuous_params, labeled_params = build_render_slots(event_cfg)
    num_slots = len(slots)
    n_sustain = int(event_cfg.get("sustainPoints", 0) or 0)

    pdesc = ""
    if continuous_params or labeled_params:
        pdesc = f"  [{len(continuous_params)} continuous, {len(labeled_params)} labeled param(s)]"
    if n_sustain > 0:
        pdesc += f"  [{n_sustain} sustain point(s) → {n_sustain + 1} parts]"
    print(f"\n[Recorder] {event_path}  ({num_slots} render{'s' if num_slots > 1 else ''}){pdesc}")

    take_frame_ranges   = []   # (start_frame, end_frame) per slot — overall range, for the debug log
    take_segment_bounds = []   # per slot: list of boundary frames; consecutive pairs = one output segment
    take_capped         = []   # True if the take's final segment hit the max_duration cap
    total_frames        = 0
    init_error        = None
    samples_per_frame = DSP_BUFFER_SIZE   # updated after init with FMOD's actual value

    studio = StudioSystem(version=FMOD_HEADER_VERSION) if FMOD_HEADER_VERSION else StudioSystem()
    try:
        core = studio.core_system
        core.output = OUTPUTTYPE.WAVWRITER_NRT
        core.software_format = so(
            sample_rate=sample_rate,
            speaker_mode=speaker_mode,
            raw_speakers=0,
        )

        # STREAM_FROM_UPDATE is essential for non-realtime rendering: it makes
        # FMOD decode streaming assets (e.g. long VO) synchronously from each
        # studio.update() instead of a background thread. Without it, streams
        # are not refilled during NRT capture and the ~0.5s stream buffer just
        # repeats — producing a stuttering loop instead of the full performance.
        studio.initialize(
            512,
            STUDIO_INIT_FLAGS.SYNCHRONOUS_UPDATE,
            INIT_FLAGS.STREAM_FROM_UPDATE,
            ctypes.c_char_p(combined_wav.encode("utf-8")),
        )

        # Read FMOD's actual DSP buffer size. WAVWRITER_NRT writes exactly this many
        # samples per studio.update() call, so slicing must use the real value.
        # DSP_BUFFER_SIZE is just a fallback default.
        try:
            samples_per_frame, _ = core.dsp_buffer_size
        except Exception:
            samples_per_frame = DSP_BUFFER_SIZE
        actual_step_time = samples_per_frame / sample_rate
        print(f"[Recorder]   DSP buffer: {samples_per_frame} samples  ({actual_step_time * 1000:.1f} ms/frame)")

        # Register plugins BEFORE loading banks, so banks using plugin effects
        # (AudioGaming, Resonance, etc.) load instead of failing PLUGIN MISSING.
        load_fmod_plugins(core)

        # Load all banks once for all takes.
        # Do NOT call flush_sample_loading() here — that call internally invokes
        # studio.update(), writing frames to the combined WAV that we cannot count.
        # In SYNCHRONOUS_UPDATE mode, bank loading completes synchronously inside
        # load_bank_file() itself, so no flush is needed.
        for bp in bank_paths:
            if not os.path.exists(bp):
                if not bp.endswith(".strings.bank"):
                    print(f"[Recorder]   WARNING: bank not found: {bp}")   # full path: a wrong folder is the usual cause
                continue
            try:
                studio.load_bank_file(bp, LOAD_BANK_FLAGS.NORMAL)
            except Exception as e:
                print(f"[Recorder]   WARNING: could not load {os.path.basename(bp)}: {e}")

        # Resolve the event. Prefer GUID lookup — it reads the event straight
        # from the loaded metadata bank (e.g. SFX.bank) and does NOT require the
        # event's path to be present in a strings bank. Fall back to path lookup.
        desc = None
        guid_obj = parse_guid(event_cfg.get("guid"))
        if guid_obj is not None:
            try:
                _dptr = ctypes.c_void_p()
                studio._call("GetEventByID", ctypes.byref(guid_obj), ctypes.byref(_dptr))
                desc = EventDescription(_dptr)
            except Exception as e:
                print(f"[Recorder]   GUID lookup failed ({event_cfg.get('guid')}): {e}; trying path")
        if desc is None:
            try:
                desc = studio.get_event(event_path)
            except Exception as e:
                raise RuntimeError(
                    "Event not found in the loaded banks. Check the event is assigned to a "
                    "bank and the banks have been built."
                ) from e

        for take_idx, slot in enumerate(slots):
            # Re-request sample data for this take. FMOD reference-counts loaded
            # samples and decrements when an instance releases; without re-requesting,
            # takes 3+ render with partially or fully unloaded sample data.
            #
            # We poll desc.sample_loading_state with counted update() calls until
            # LOADED (or we hit the cap). This guarantees the sample is fully loaded
            # before the take starts, regardless of how many updates FMOD needs.
            # All updates here are counted and fall before take_start, so they do
            # not appear inside any take's slice.
            try:
                desc.load_sample_data()
            except Exception as e:
                print(f"[Recorder]   WARNING: load_sample_data raised: {e} (continuing)")

            # Wait for the sample to finish loading before recording. NRT
            # update() calls run far faster than real time, so a pure "poll N
            # times" loop can outrun FMOD's async disk load and never observe
            # LOADED (seen on Windows with a cold cache) — recording then starts
            # before the sound plays, leaving a long leading silence. We give the
            # loader real wall-clock time between updates so LOADED actually
            # trips, bounded by both a frame cap and a wall-clock cap.
            MAX_LOAD_FRAMES  = 2048    # hard cap on update() calls while loading
            MAX_LOAD_SECONDS = 8.0     # real-time cap so a cold/slow load can finish
            load_frames = 0
            loaded = False
            _load_start = time.monotonic()
            while load_frames < MAX_LOAD_FRAMES:
                studio.update()
                total_frames += 1
                load_frames += 1
                try:
                    if desc.sample_loading_state == LOADING_STATE.LOADED:
                        loaded = True
                        break
                except Exception:
                    loaded = True   # state not queryable — proceed anyway
                    break
                if time.monotonic() - _load_start > MAX_LOAD_SECONDS:
                    break
                # Yield real time so the async loader thread can make progress.
                time.sleep(0.003)

            if load_frames > 1 or not loaded:
                _how = "loaded" if loaded else "NOT confirmed (cap hit)"
                print(f"[Recorder]   take {take_idx + 1}: sample {_how} after "
                      f"{load_frames} flush frames / {time.monotonic() - _load_start:.2f}s")

            # take_start is set AFTER all flush frames so they are not
            # included in this take's slice.
            take_start = total_frames

            instance = desc.create_instance()

            # Apply parameters for this slot BEFORE starting the instance.
            # Continuous params are constant across slots; labeled params take
            # the value for this slot's label combination.
            for p in continuous_params:
                try:
                    set_continuous_param(studio, instance, p["name"], p.get("value", 0), p.get("global", False))
                except Exception as pe:
                    print(f"[Recorder]   WARNING: could not set param '{p.get('name')}'={p.get('value')}: {pe}")
            for p in labeled_params:
                label = slot["labels"].get(p["name"])
                if label is None:
                    continue
                try:
                    set_labeled_param(studio, instance, p["name"], label, p.get("global", False), p.get("labels"))
                except Exception as pe:
                    print(f"[Recorder]   WARNING: could not set labeled param '{p.get('name')}'='{label}': {pe}")
            if slot["labels"]:
                print(f"[Recorder]   slot {take_idx + 1}: " + ", ".join(f"{k}={v}" for k, v in slot["labels"].items()))

            instance.start()
            studio.update()
            total_frames += 1

            DRAIN_FRAMES = 200   # extra updates after a natural stop to capture reverb/FX tail

            # Frame budget for the (final) capture. FMOD reports a DSP buffer of
            # `samples_per_frame` (e.g. 1024) but WAVWRITER_NRT actually writes
            # fewer samples per update (~500, a consistent discrepancy here), so
            # a time estimate from the reported buffer runs ~2x too fast and
            # quits at roughly half the real duration. Instead we budget frames
            # using a conservative lower bound on samples-written-per-update, so
            # at least `max_duration` seconds are always captured. Finite events
            # end earlier via STOPPED detection; looping events run to the budget
            # and get clamped to exactly max_duration when sliced.
            SPF_FLOOR    = 256.0   # true lower bound on samples/update (observed 477+); keep conservative
            # + LEAD_HEADROOM_S so a looping event still yields max_duration of
            # actual content after any leading silence (stream spin-up) is skipped.
            LEAD_HEADROOM_S = 12.0
            frame_budget = int((max_duration + LEAD_HEADROOM_S) * sample_rate / SPF_FLOOR) + 2

            bounds = [take_start]   # segment boundaries; consecutive pairs = one output segment

            # Sustain points: hold at each for `sustain_hold`s, then key-off to
            # advance the timeline. Each key-off frame is a segment boundary, so
            # the take splits into N+1 parts (start, release1, release2, …).
            if n_sustain > 0:
                HOLD_SPF = 512.0   # rough samples/update, only used to time the hold
                hold_frames = max(1, int(round(sustain_hold * sample_rate / HOLD_SPF)))
                for k in range(n_sustain):
                    held = 0
                    while held < hold_frames:
                        studio.update()
                        total_frames += 1
                        held += 1
                        try:
                            if instance.playback_state == PLAYBACK_STATE.STOPPED:
                                break
                        except Exception:
                            break
                    try:
                        instance._call("KeyOff")
                    except Exception as ke:
                        print(f"[Recorder]   WARNING: key-off {k + 1} failed: {ke}")
                    bounds.append(total_frames)
                print(f"[Recorder]   take {take_idx + 1}: {n_sustain} sustain point(s) → {n_sustain + 1} parts (held {sustain_hold}s each)")

            # Play out the final segment until the event stops (or the budget cap).
            capped = True
            frames_this_take = 0
            while frames_this_take < frame_budget:
                studio.update()
                total_frames     += 1
                frames_this_take += 1
                try:
                    stopped = (instance.playback_state == PLAYBACK_STATE.STOPPED)
                except Exception:
                    stopped = True   # instance auto-released (one-shot) — natural end
                if stopped:
                    for _ in range(DRAIN_FRAMES):
                        studio.update()
                        total_frames += 1
                    capped = False
                    break

            if capped:
                print(f"[Recorder]   take {take_idx + 1}: reached max_duration cap ({max_duration}s) — looping/long event, slice trimmed to length.")
            elif n_sustain == 0:
                print(f"[Recorder]   take {take_idx + 1}: ended naturally after {frames_this_take} frames.")

            try:
                instance.stop()
            except Exception:
                pass
            try:
                instance.release()
            except Exception:
                pass

            take_end = total_frames
            bounds.append(take_end)
            take_frame_ranges.append((take_start, take_end))
            take_segment_bounds.append(bounds)
            take_capped.append(capped)

            # Silence padding between takes so slices don't bleed into each other.
            # Last take still gets padding — it costs nothing and keeps slice logic uniform.
            for _ in range(SILENCE_PAD_FRAMES):
                studio.update()
                total_frames += 1

    except Exception as e:
        init_error = str(e)
        print(f"[Recorder]   ERROR: {e}")
    finally:
        studio.release()   # flushes and closes the combined WAV

    # ── Slice combined WAV into per-take files ────────────────
    results = []

    combined_ok = os.path.exists(combined_wav) and os.path.getsize(combined_wav) > 44

    combined_data = None
    actual_spf    = float(samples_per_frame)  # fallback; overwritten below

    if combined_ok:
        try:
            import soundfile as sf
            import numpy as np
            combined_data, _ = sf.read(combined_wav)
        except Exception as e:
            print(f"[Recorder]   ERROR reading combined WAV: {e}")
            combined_data = None

    _preroll_frames     = 0
    _preroll_end_sample = 0.0
    _corrected_spf      = actual_spf

    if combined_data is not None and total_frames > 0:
        actual_spf     = len(combined_data) / total_frames
        _corrected_spf = actual_spf
        print(f"[Recorder]   Combined: {len(combined_data)} samples / {total_frames} updates = {actual_spf:.4f} spf  (dsp_buffer_size reported {samples_per_frame})")

        # Pre-roll frames (sample-loading poll before take 1) write at a higher
        # samples/frame rate than take frames, inflating actual_spf and shifting
        # every slice start too early. Probe take 1's leading silence to
        # back-calculate the true take-frame SPF.
        _preroll_frames = take_frame_ranges[0][0] if take_frame_ranges else 0
        if _preroll_frames > 0:
            _t1_s = round(take_frame_ranges[0][0] * actual_spf)
            _t1_e = round(take_frame_ranges[0][1] * actual_spf)
            _t1   = combined_data[_t1_s : min(_t1_e, len(combined_data))]
            if len(_t1) > 0:
                _abs1 = np.abs(_t1) if _t1.ndim == 1 else np.abs(_t1).max(axis=1)
                _nz   = np.where(_abs1 > 10 ** (-96 / 20.0))[0]
                _lead = int(_nz[0]) if len(_nz) > 0 else 0
                if _lead > 0:
                    _P = actual_spf + _lead / _preroll_frames
                    _T = (len(combined_data) - _preroll_frames * _P) / (total_frames - _preroll_frames)
                    # Sanity guard: this two-pass correction misfires badly when
                    # the sample wasn't loaded before the take (a huge phantom
                    # lead → absurd take SPF, e.g. 148 vs a true ~515). The
                    # directly-measured global SPF is reliable, so only accept the
                    # correction when it stays within ±15% of it; otherwise fall
                    # back to the global SPF uniformly (safe for sliced renders).
                    if _T > 0 and 0.85 * actual_spf <= _T <= 1.15 * actual_spf:
                        _corrected_spf      = _T
                        _preroll_end_sample = _preroll_frames * _P
                        print(f"[Recorder]   SPF correction: preroll {_preroll_frames}f @ {_P:.2f}spf  take {_T:.4f}spf  (global was {actual_spf:.4f})")
                    else:
                        print(f"[Recorder]   SPF correction rejected (take {_T:.1f}spf vs global {actual_spf:.1f}spf) — using global SPF uniformly")

    def to_sample(frame):
        if _preroll_end_sample > 0 and frame > _preroll_frames:
            return round(_preroll_end_sample + (frame - _preroll_frames) * _corrected_spf)
        return round(frame * actual_spf)

    def segment_suffix(j, n_segs):
        # No sustain split → no extra suffix (unchanged single-file behaviour).
        # With sustain points: part 1 = "_1_start", then "_2_release1", "_3_release2", …
        if n_segs <= 1:
            return ""
        return "_1_start" if j == 0 else f"_{j + 1}_release{j}"

    for slot_idx, bounds in enumerate(take_segment_bounds):
        slot_suffix = slots[slot_idx]["suffix"] if slot_idx < len(slots) else f"_{slot_idx + 1:02d}"
        n_segs = len(bounds) - 1

        for j in range(n_segs):
            seg_start_f = bounds[j]
            seg_end_f   = bounds[j + 1]
            is_last_seg = (j == n_segs - 1)
            suffix      = slot_suffix + segment_suffix(j, n_segs)
            final_ext   = fmt.lower()
            final_name  = base_name + suffix + "." + final_ext
            # WAV renders straight to the final name. Non-WAV renders write to a temp
            # WAV that gets converted then deleted — so a previously & deliberately
            # rendered <base><suffix>.wav is NEVER overwritten or deleted.
            if fmt == "WAV":
                take_wav = os.path.join(output_path, final_name)
            else:
                take_wav = os.path.join(output_path, base_name + suffix + ".__render_tmp__.wav")

            result = {
                "eventPath":   event_path,
                "suffix":      suffix,
                "filename":    None,
                "format":      fmt,
                "sampleRate":  sample_rate,
                "channels":    num_channels,
                "bitDepth":    None,
                "durationSec": None,
                "sizeKB":      None,
                "success":     False,
                "error":       init_error,
            }

            if combined_data is not None:
                start_s = to_sample(seg_start_f)
                end_s   = to_sample(seg_end_f)
                # The loop content-cut only applies to a capped (looping) take's
                # FINAL segment; sustain segments and finite events use normal trim.
                is_capped = is_last_seg and slot_idx < len(take_capped) and take_capped[slot_idx]
                take_data = combined_data[start_s : min(end_s, len(combined_data))]
                try:
                    import soundfile as sf
                    import numpy as np

                    # Skip an all-silent release segment. Offline (NRT), the
                    # post-key-off release of a sustain event currently renders
                    # silent (see README/backlog); rather than emit a useless
                    # empty file, drop it. The initial "_1_start" part (j==0)
                    # always writes, even if silent.
                    if j > 0 and n_segs > 1:
                        _m = np.abs(take_data) if take_data.ndim == 1 else np.abs(take_data).max(axis=1)
                        if _m.size == 0 or float(_m.max()) < 10 ** (-90 / 20.0):
                            print(f"[Recorder]   {final_name}: release segment is silent (offline sustain limitation) — skipped")
                            continue

                    if is_capped:
                        # Looping/long event: the loop over-captures (budget includes
                        # headroom). Keep exactly max_duration seconds of ACTUAL
                        # CONTENT — skip past leading silence to the first audible
                        # sample, then take max_duration from there. No trailing trim.
                        _abs = np.abs(take_data) if take_data.ndim == 1 else np.abs(take_data).max(axis=1)
                        _nz  = np.where(_abs > 10 ** (-96 / 20.0))[0]
                        lead = int(_nz[0]) if len(_nz) > 0 else 0
                        want = round(max_duration * sample_rate)
                        take_data = take_data[lead : lead + want]
                        sf.write(take_wav, take_data, sample_rate)
                        lead_trim_ms  = round(lead / sample_rate * 1000)
                        trail_trim_ms = 0
                        if len(take_data) < want:
                            print(f"[Recorder]   {final_name}: WARNING only {len(take_data)/sample_rate:.1f}s of content available (< {max_duration}s requested)")
                    else:
                        sf.write(take_wav, take_data, sample_rate)
                        lead_trim_ms, trail_trim_ms = trim_silence(take_wav)

                    # Optional loudness/peak normalization (in place, before convert).
                    normalize_wav(take_wav, normalize_cfg)

                    info = sf.info(take_wav)
                    kb   = os.path.getsize(take_wav) // 1024
                    result.update({
                        "filename":     final_name,                    # intended FINAL name (target ext)
                        "srcWav":       os.path.basename(take_wav),     # intermediate the main loop converts/deletes
                        "bitDepth":     info.subtype_info.split()[0] if info.subtype_info else None,
                        "durationSec":  round(info.duration, 3),
                        "leadTrimMs":   lead_trim_ms,
                        "trailTrimMs":  trail_trim_ms,
                        "sizeKB":       kb,
                        "success":      True,
                        "error":        None,
                    })
                    print(f"[Recorder]   {kb} KB  ({result['durationSec']}s)  → {final_name}")
                except Exception as e:
                    result["error"] = str(e)
                    print(f"[Recorder]   {final_name}: ERROR {e}")
            else:
                if not result["error"]:
                    result["error"] = "Output file missing or empty after render"

            results.append(result)

    # ── Debug log ─────────────────────────────────────────────────
    # Written after slicing so trimmed duration is available alongside slice duration.
    try:
        debug_lines = [
            f"event: {event_path}",
            f"total_frames: {total_frames}",
            f"combined_samples: {len(combined_data) if combined_data is not None else 0}",
            f"actual_spf: {actual_spf:.4f}",
            f"corrected_spf: {_corrected_spf:.4f}  preroll_frames: {_preroll_frames}",
            f"dsp_buffer_size_reported: {samples_per_frame}",
            f"sample_rate: {sample_rate}",
        ]
        # One line per output segment (a slot may produce several with sustain points).
        ri = 0
        for slot_idx, bounds in enumerate(take_segment_bounds):
            n_segs = len(bounds) - 1
            for j in range(n_segs):
                sf_val, ef_val = bounds[j], bounds[j + 1]
                s = to_sample(sf_val); e = to_sample(ef_val)
                slice_dur = (e - s) / sample_rate
                res = results[ri] if ri < len(results) else {}
                ri += 1
                debug_lines.append(
                    f"  slot {slot_idx+1} seg {j+1}/{n_segs} [{res.get('suffix','')}]: frames {sf_val}-{ef_val}"
                    f"  slice={slice_dur:.3f}s  trimmed={res.get('durationSec','?')}s"
                    f"  lead_trim={res.get('leadTrimMs',0)}ms  trail_trim={res.get('trailTrimMs',0)}ms"
                )
        debug_path = str(SCRIPT_DIR / "recorder_debug.log")
        with open(debug_path, "a") as dbf:
            dbf.write("\n".join(debug_lines) + "\n---\n")
    except Exception:
        pass

    # If no takes were tracked (init failed before any loop ran) fill placeholders
    if not take_frame_ranges:
        for take_idx in range(num_slots):
            suffix = slots[take_idx]["suffix"] if take_idx < len(slots) else f"_{take_idx + 1:02d}"
            results.append({
                "eventPath":   event_path,
                "suffix":      suffix,
                "filename":    None,
                "format":      fmt,
                "sampleRate":  sample_rate,
                "channels":    num_channels,
                "bitDepth":    None,
                "durationSec": None,
                "sizeKB":      None,
                "success":     False,
                "error":       init_error or "Init failed before recording started",
            })

    # Clean up combined temp file
    try:
        if os.path.exists(combined_wav):
            os.remove(combined_wav)
    except Exception:
        pass

    return results

# ── Main loop ─────────────────────────────────────────────────

success = 0
failed  = 0
recording_results = []

def normalize_event(entry):
    """Accept either an old-style event path string or a new-style
    {path, takes, params} object. Returns a normalized cfg dict."""
    if isinstance(entry, str):
        return {"path": entry, "takes": num_takes, "params": []}
    cfg = dict(entry)
    cfg.setdefault("path", entry.get("path"))
    cfg.setdefault("takes", num_takes)
    cfg.setdefault("params", [])
    return cfg

for entry in event_paths:
    event_cfg = normalize_event(entry)
    take_results = render_event_takes(event_cfg)
    for result in take_results:
        if result["success"]:
            if fmt != "WAV":
                # result["filename"] is already the final name; convert the
                # private temp WAV (srcWav) into it, then drop the temp.
                src_wav    = os.path.join(output_path, result.get("srcWav", result["filename"]))
                final_path = os.path.join(output_path, result["filename"])
                if convert(src_wav, final_path, fmt) and os.path.exists(final_path):
                    result["sizeKB"] = os.path.getsize(final_path) // 1024
            result.pop("srcWav", None)
            success += 1
        else:
            failed += 1
        recording_results.append(result)

print(f"\n[Recorder] ─────────────────────────────────────────")
print(f"[Recorder] Done.  {success} rendered,  {failed} failed.")
print(f"[Recorder] Output: {output_path}")
print(f"[Recorder] ─────────────────────────────────────────")

# ── Write results JSON ────────────────────────────────────────

results_path = str(SCRIPT_DIR / "recorder_results.json")
results_payload = {
    "success":    success,
    "failed":     failed,
    "outputPath": output_path,
    "recordings": recording_results,
    "banksFound":    len(_bank_found),
    "banksExpected": len(_bank_files),
    "bankError":     bank_error,
}
try:
    # Write atomically: the Studio script polls this file while we run, so it
    # must only ever see an empty file or the complete JSON — never a partial
    # write. Write to a temp file, then os.replace() (atomic on the same volume).
    tmp_path = results_path + ".tmp"
    with open(tmp_path, "w") as f:
        json.dump(results_payload, f, indent=2)
    os.replace(tmp_path, results_path)
    print(f"[Recorder] Results: {results_path}")
except Exception as e:
    print(f"[Recorder] WARNING: could not write results JSON: {e}")
