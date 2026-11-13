# FMOD Event Recorder — v0.6.5 (DAT-audio Utilities pack)

> **Requires Python 3.9 or newer.** That is the one thing you install yourself
> ([python.org](https://www.python.org/downloads/); macOS: `brew install python@3.13`;
> Windows: tick *Add Python to PATH*). The setup script checks the version and
> stops with a clear message if it's older.

> [!IMPORTANT]
> **The FMOD Engine libraries are not included — you add them yourself.** FMOD's
> position is that people who make tools or scripts must not publish or share the
> FMOD libraries (`.dll` / `.dylib`) with them. Download the FMOD Engine from
> [fmod.com](https://www.fmod.com/download) (free account, accepting FMOD's
> licence) and copy two files into `libs/macos/` or `libs/windows/` before running
> setup. See [Setup, step 2](#setup). Each folder has a `PUT_…_HERE.txt` file
> saying exactly which files go there.

Batch-render **FMOD Studio events to audio files on disk**, offline, without a
running game. Select events in the FMOD Studio browser, choose a few options
(including per-event **parameters**), and it writes one audio file per event
(per take, per label) into a folder.

By **Tobey Evans**.

> *Note: this isn't the most elegant of solutions — it's a pragmatic attempt to
> add offline event-rendering functionality on top of FMOD Studio's current
> capabilities.*

---

## What it does

- Renders any number of selected FMOD events to **WAV, AIFF, FLAC, or MP3**.
- **Per-event parameter control** — detects each event's game parameters and lets
  you set them before rendering:
  - **Labeled** parameters (named values) → multi-select; one file is rendered per
    selected label.
  - **Continuous** parameters → a single value applied to every take.
  - Global vs. local parameters are handled automatically; effect-chain knobs are
    never touched.
- **Multiple takes per event**, with genuine variation for events that use
  random / multi-instrument selection.
- **Max duration** control — looping events are captured for the length you set;
  finite events stop at their natural end (the captured duration excludes any
  silence at the start of the event — leading silence is skipped/trimmed for all
  recordings).
- Optional **loudness / peak normalization** (LUFS-I and/or dBFS ceiling), off by
  default.
- Automatically **slices** multi-take renders, **trims** leading/trailing digital
  silence, and names files after the event path.

---

## Requirements

You install **Python 3** and add **the FMOD Engine libraries** yourself.
Everything else is handled by the one-click setup script.

| Requirement | Provided? | Notes |
|---|---|---|
| **Python 3** (3.9 or newer) | ❌ **install this** | [python.org](https://www.python.org/downloads/); macOS: `brew install python@3.13`; Windows: tick *Add Python to PATH* |
| FMOD Engine libraries | ❌ **add these** (not redistributable) | from the FMOD Engine download at [fmod.com](https://www.fmod.com/download). macOS: `libfmod.dylib` + `libfmodstudio.dylib` in `libs/macos/`. Windows: `fmod.dll` + `fmodstudio.dll` in `libs/windows/`. See Setup, step 2 |
| Audio file support (WAV/AIFF/FLAC/MP3) | ✅ | libsndfile ships inside the `soundfile` package |
| Python packages (pyfmodex, numpy, soundfile, …) | ✅ via setup script | installed automatically |
| Internet connection | once | only during setup, to download the Python packages |

- **Platforms:** **macOS (Apple Silicon)** — tested. **Windows (x64)** — supported
  from the same Scripts/Tools folder, but newer/less battle-tested. On both, you
  supply the FMOD libraries (see Setup). The recorder detects
  the OS and uses `libs/macos` or `libs/windows` automatically.
- Any **Python 3.9+** works — the setup script installs library versions matched to
  your Python (e.g. newer NumPy on 3.11+, the older 1.26 line on 3.9).

### Supported FMOD versions

Use FMOD Engine libraries whose version is **≥ the FMOD Studio version that built
your banks**. A newer engine is backward-compatible, so a **2.03.x** engine renders
projects built with **FMOD Studio 2.02.x and 2.03.x**; 2.03.x is the recommended
choice and the recorder is tested with **2.03.20**. The renderer **auto-detects the
engine version**, so you don't configure anything.

---

## Setup

The recorder ships inside the **DAT-audio Utilities** pack. The menu script is
`Scripts/DAT-audio Utilities/Event Recorder.js`, and this folder is
`Tools/FMOD_Event_Recorder/`. The script looks for its tools in a `Tools/`
folder **next to the `Scripts/` folder it's installed in**, so the two folders
must stay side by side.

**1. Install the pack** (skip if you already have it). Copy the pack's
`Scripts/` and `Tools/` folders, merging with any that already exist, into the
folder for your FMOD version:

| Install | Where |
| --- | --- |
| Per machine, macOS, FMOD 2.02.x | `~/Library/Application Support/FMOD Studio/` |
| Per machine, macOS, FMOD 2.03.x | `~/Library/Preferences/FMOD Studio/` |
| Per machine, Windows (2.02.x and 2.03.x) | `%LOCALAPPDATA%\FMOD Studio\`, by default `C:\Users\<YourUsername>\AppData\Local\FMOD Studio\` |

On macOS, 2.02 and 2.03 read different folders. To cover both from one copy,
see "macOS, both 2.02 and 2.03 from one copy" in
`Scripts/DAT-audio Utilities/README.md` (two symlinks). Run this setup once per
real copy of `Tools/FMOD_Event_Recorder/`; a symlinked copy shares the venv.
For a per-project install (inside a project's folder), run this setup in that
project's copy; see *Installing per project* in the top-level README.

**2. Add the FMOD Engine libraries.** *Why this is a manual step:* FMOD's position
is that people who make tools or scripts must not publish or share the FMOD
libraries with them, so this pack doesn't include them. You download your own copy
from FMOD, under your own acceptance of FMOD's licence. It's free and takes a couple
of minutes: sign in at [fmod.com/download](https://www.fmod.com/download), open
**FMOD Engine**, and download it for your OS (2.03.x recommended; see *Supported
FMOD versions*). Use the **release** files, not the logging ones whose names end in
`L` (`libfmodL.dylib`, `fmodL.dll`).

| OS | Copy these files from the FMOD Engine download | Into |
| --- | --- | --- |
| macOS | `api/core/lib/libfmod.dylib`, `api/studio/lib/libfmodstudio.dylib` | `Tools/FMOD_Event_Recorder/libs/macos/` |
| Windows | `api\core\lib\x64\fmod.dll`, `api\studio\lib\x64\fmodstudio.dll` | `Tools\FMOD_Event_Recorder\libs\windows\` |

The target folders already exist and each holds a `PUT_…_HERE.txt` file with these
instructions. Do this **before** step 3; setup checks that the libraries load.

- **macOS:** the libraries are universal (Apple silicon and Intel), so there's one
  folder. Setup clears their quarantine flag for you.
- **Windows:** use the architecture that **matches your Python** (the DLLs load into
  it): **x64** for standard 64-bit Python (the usual case), or **arm64** only if you
  run a native ARM64 Python on Windows-on-ARM. You can drop them straight in
  `libs/windows/`, or keep both arches in `libs/windows/x64/` and
  `libs/windows/arm64/` — the recorder auto-picks the one matching the running
  Python.
- **Sharing or re-uploading this tool?** Leave the libraries out. Copy the folder
  without them (keep the `PUT_…_HERE.txt` files) so the next person adds their own.

**3. Run the setup script** in this folder (`Tools/FMOD_Event_Recorder/`), once
per install location:

- **macOS:** double-click **`setup.command`** (or `./setup.command` in a terminal).
- **Windows:** double-click **`setup.bat`**.

It checks for Python 3.9 or newer, asks to confirm, creates a local Python environment,
installs the required libraries, (macOS) clears library quarantine, and verifies
everything loads. If you've run it before, it detects the existing install and offers
to skip re-downloading.

> **macOS Gatekeeper:** the first time, macOS may block the unsigned script — if so,
> **right-click `setup.command` → Open**, then click *Open*. (The script clears
> quarantine on the `.dylib` files you added. If you add or replace them later, run
> it again.)

That's it — no manual `pip` needed. The setup scripts live inside
`Tools/FMOD_Event_Recorder/`.

> **Optional — share the FMOD libraries across several tools.** By default the
> renderer uses the libraries you put in `Tools/FMOD_Event_Recorder/libs/`. If you
> run more than one FMOD Python tool and would rather keep a single copy of the
> libraries, the renderer also looks in (first match wins):
> 1. the **`FMOD_LIB_DIR`** environment variable (point it at a folder with the
>    libraries, or at a `libs/` base with `macos/` / `windows/` subfolders);
> 2. a shared **`Tools/libs/<platform>/`** folder (next to this one);
> 3. this tool's own `Tools/FMOD_Event_Recorder/libs/<platform>/` (the default).
>
> So you can drop the libraries in `Tools/libs/macos/` or `Tools/libs/windows/` (or
> set `FMOD_LIB_DIR`) instead of in this folder.

<details>
<summary><b>Manual setup (fallback, if you'd rather not run the script)</b></summary>

Add the FMOD libraries first (step 2), then:

macOS — in `Tools/FMOD_Event_Recorder/` (Python 3.9+):
```sh
python3 -m venv venv
venv/bin/python -m pip install -r requirements.txt
xattr -dr com.apple.quarantine libs/macos
```

Windows — in `Tools\FMOD_Event_Recorder\` (Python 3.9+):
```bat
py -3 -m venv venv
venv\Scripts\python.exe -m pip install -r requirements.txt
```

LUFS normalization needs `pyloudnorm` (included in `requirements.txt`; it pulls in
SciPy). Peak/dBFS normalization needs only NumPy — remove the `pyloudnorm` line if
you never use LUFS.
</details>

---

## Launching it

Launch it from FMOD Studio: select events in the **Event Browser**, then
**Scripts → DAT-audio Utilities → Record Selected Events…**. The script opens
the recorder's settings window. When you press **▶ Record N Events**, it
writes `recorder_manifest.json` here and runs `fmod_recorder.py` with this
folder's own Python environment (`venv`, made by Setup).

- **Don't run `fmod_recorder.py` yourself:** it renders what the script's
  manifest asks for, with the banks the script found for the open project.
- **Run Setup once first** (above), after adding the FMOD Engine libraries
  to `libs/`. If Setup hasn't been run, the script says so instead of
  opening. Setup and the renderer both name the folder if the libraries are
  missing.
- **No menu item?** The pack isn't installed or loaded. `DAT-audio
  Utilities.js` and the `DAT-audio Utilities` folder go in your FMOD
  `Scripts` folder, and this folder goes in `Tools/FMOD_Event_Recorder/` next
  to it (see the pack's top-level `README.md`). Then use **Scripts →
  Reload**, or restart Studio.

## How to use

1. In FMOD Studio: **Scripts → Reload** (first time, or after updating the script).
   The Script Output log prints a **Tools folder** line on each run. If a render
   fails to start, check that it points at this folder.
2. **Build your banks** (Studio's *Build* command). The recorder renders whatever
   banks are **currently built on disk**, wherever this project builds them (see
   *Where it reads your banks from* below). If you've changed events and not
   rebuilt, you'll render stale content.
3. **Select one or more events** in the Event Browser.
4. **Scripts → DAT-audio Utilities → Record Selected Events…**
5. Set the output folder, format, sample rate, channels, takes, and max duration.
6. For any event that exposes parameters, click **Configure…** to set them
   (tick labels, type continuous values).
7. (Optional) tick **Normalize** and set a LUFS-I target and/or peak ceiling.
8. Click **Record**. When it finishes, a results dialog lists the files, with an
   **Open Output Folder** button.

Output filenames are based on the event path (e.g.
`SFX_Ambience_Forest_Loop.wav`). Selecting labels on a labeled parameter adds a
`_ParamName-Label` suffix (e.g. `…_Surface-Grass.wav`), and multiple takes add
`_01`, `_02`, ….

### Where it reads your banks from

The recorder doesn't render from the FMOD project itself. It renders from the
project's **built banks**, read from wherever the open project builds them, the
same way FMOD lays them out:

```
<build output directory> / <platform subdirectory> / [<bank folder path> /] <bank>.bank
```

- **Build output directory:** the project's own setting (*Preferences → Build →
  Built banks output directory*). If it's relative, it's relative to the folder
  containing the `.fspro`, so it can point **outside the FMOD project**, e.g.
  `../../game/sound` into a separate game repository. Absolute paths, including
  Windows `C:\…` and network `\\server\…` paths, are used as they are.
- **Platform subdirectory:** the **Desktop** platform's built-banks subdirectory,
  or the first platform's if there's no Desktop. If it's empty, banks sit
  directly in the output directory.
- **Bank folder path:** only when the project has **mirror browser folder
  hierarchy** turned on in its build settings. FMOD then builds each bank into
  subfolders matching the Banks browser (e.g. `…/sound/banks/UI.bank`).

Nothing about this is fixed in the tool: every part comes from the open project,
so it works for any project's setup.

**Before you record, check:**
- **The banks are built on this machine.** If the output directory points into
  another repository (e.g. a game repository), that repository must be checked
  out and the banks built into it.
- **They're current.** The recorder uses what's on disk, not what's in the
  editor, so rebuild after changing events.
- **The events are assigned to a bank** that gets built for the Desktop
  platform (or the first platform).

**If banks are missing,** the results window says so up front:
- **None found:** one message naming the folder it looked in, telling you to
  build the banks (*File → Build*) or check the build output directory
  (*Preferences → Build*).
- **Some not found:** a note saying how many weren't on disk. Events in those
  banks can't be recorded.

The Script Output log shows exactly where it looked: a `Build dir` line, a
`Bank folders` line (*mirrored* or *flat*), and the full path of any missing bank.

> **Fixed in v0.6.2:** earlier versions ignored *mirror browser folder
> hierarchy*. They looked for every bank directly in the platform folder, so
> projects built with mirrored folders failed with "event not found" for every
> event, even with all banks built. They also didn't recognize Windows absolute
> output paths. Both are fixed; see the changelog.

---

## How it works (brief)

Two parts talk through a JSON manifest:

1. **`fmod_event_recorder.js`** — the FMOD Studio menu script. Reads your selection
   and parameters, resolves the built-bank paths from the project's build settings,
   writes a manifest, and launches the renderer directly (no separate watcher/
   terminal needed).
2. **`fmod_recorder.py`** — the renderer. Loads the banks, renders each event/label/
   take via FMOD's non-realtime `WAVWRITER` output into a combined WAV, then slices,
   trims, optionally normalizes, and converts to the chosen format.

Key technical points: all takes of an event render inside a single FMOD system so
random events genuinely vary between takes; streaming assets are decoded
synchronously (`STREAM_FROM_UPDATE`); and looping events are captured for exactly
the requested duration.

## What we had to build ourselves

A Studio script can't render audio, and the FMOD Engine isn't part of
Studio's scripting. These parts are our own:

- **A separate renderer.** The script writes a manifest and starts
  `fmod_recorder.py`, which runs the FMOD Engine itself (through pyfmodex)
  with its non-realtime WAV writer. That's why you add the FMOD Engine
  libraries yourself: FMOD doesn't allow tools to ship them.
- **Finding the banks.** The renderer plays built banks, not the project, so
  the script works out where the open project builds them from its build
  settings (output directory, platform subdirectory and mirrored bank
  folders).
- **Reading parameters.** The script reads each event's game parameters from
  the project (labelled or continuous, global or local) so you can set them
  before rendering.
- **Real variation between takes.** All takes of an event render in one FMOD
  system, so random and multi instruments really change from take to take.
  Silent gaps go between takes, and the result is sliced on them, trimmed
  and named.
- **Streams decode in step.** Streaming assets are decoded synchronously
  (`STREAM_FROM_UPDATE`), or they'd lag or drop out in a non-realtime render.

---

## Normalization

Off by default. Tick **Normalize** and set either or both:

- **LUFS-I target** — integrated loudness (ITU-R BS.1770-4, via `pyloudnorm`).
- **Peak (dBFS)** — sample-peak ceiling.

With both set, the output is loudness-matched to the LUFS target, then turned down
if needed so peaks never exceed the ceiling. With only a peak value, it's a plain
peak normalize. A hard 0 dBFS safety limit always applies so output never clips.

> ⚠️ Normalization and the *Open Output Folder* button are verified in offline
> testing but have not yet been exercised across a wide range of real Studio
> sessions — treat as new. Please report anything unexpected.

---

## Known limitations

- **Silence is load-bearing.** Leading/trailing digital silence is always trimmed,
  and multi-take slicing relies on inserted silent gaps between takes. This makes
  the tool a **poor fit for VO or any assets that use deliberate silence as part of
  the performance** — that silence will be snipped. Worse, when an event's
  performance contains deliberate silence, it can confuse the slice boundaries in a
  multi-take session, causing part of a later take's audio to be baked onto the end
  of an earlier take. Render such events one at a time (single take) to avoid this.
- **16-bit output.** FMOD's non-realtime WAV writer produces 16-bit PCM; true 24-bit
  is a planned rework (see below).
- **Sustain points: initial part only.** Events with sustain points are detected and
  **split into parts** (`_1_start`, `_2_release1`, …), held for the *Sustain hold (s)*
  time at each point before a key-off. The **initial part (`_1_start`) records
  correctly.** However, the **post-key-off release part is currently silent** — even
  for basic key-off sustains. Offline (non-realtime), the key-off advances the event
  past the sustain point (the event ends normally) but no release audio is produced.
  Capturing the release is an open problem and is on the backlog; for now, only the
  initial part is usable.
- **3D events** render short/quiet (they go virtual without a listener).
- Studio's UI is **blocked while a render runs** (the render is synchronous).

---

## Changelog

### v0.6.5
- **The FMOD Engine libraries are no longer shipped** with the tool, on macOS as
  well as Windows. FMOD's position is that tool and script makers must not publish
  or share them, so you now add `libfmod.dylib` + `libfmodstudio.dylib` (macOS) or
  `fmod.dll` + `fmodstudio.dll` (Windows) yourself; see *Setup, step 2*. Each
  `libs/<platform>/` folder has a `PUT_…_HERE.txt` naming the files. Earlier notes
  below that mention a "bundled" engine describe the old packaging.
- **Setup checks for the libraries first.** `setup.command` and `setup.bat` stop
  before building anything if the libraries are missing, and say where to download
  them and which files go where. `setup.bat` also accepts the `x64\` / `arm64\`
  subfolder layout, like the renderer.
- The renderer's "FMOD libraries not found" error and the "renderer did not
  complete" alert now say the libraries must be added and where to get them.

### v0.6.4
- **Menu item renamed** to **Scripts → DAT-audio Utilities → Record Selected
  Events…** (was *Event Recorder…*), so it starts with a verb like the rest of
  the pack. The tool, its window titles and its files keep the name Event
  Recorder. Nothing else changed.

### v0.6.3
**Fixes**
- **Checks that setup has been run before opening.** If this install's
  Python environment (`venv/`) is missing, broken (its Python was removed or
  upgraded), or lacks a required library (pyfmodex, numpy, soundfile), the tool
  now says so straight away and names the folder to run `setup.command` /
  `setup.bat` in. Before, the window opened normally and a render ended in a
  generic *"The renderer did not complete"* alert. The FMOD libraries aren't
  checked here, because they can live in several places; the renderer still
  reports a missing one.

### v0.6.2
**Fixes**
- **Banks built into mirrored folders are now found.** When a project has
  *mirror browser folder hierarchy* turned on in its build settings, FMOD builds
  each bank into subfolders matching the Banks browser (e.g.
  `…/sound/banks/UI.bank`). The recorder ignored this and looked for every bank
  directly in the platform folder, so every event failed with "event not found".
  It now builds each bank's path the same way FMOD does:
  output folder → platform subfolder → bank folder path → bank.
- **The build output folder is read from the open project, wherever it points.**
  A relative setting (e.g. `../../game/sound`, into a separate game repository)
  is resolved against the project's folder. Windows absolute paths (`C:\…`,
  `\\server\…`) are now recognized, and `\` separators are normalized, so a
  project set up on Windows also works on macOS. An empty platform subfolder now
  means "no subfolder", instead of falling back to the platform's name.
- **Clear message when no banks are built.** If none of the project's banks exist
  where its build settings say, the results window opens with one message naming
  the folder and saying to build the banks, instead of a list of "event not found"
  errors. If only some are missing, a note says how many.
- **Plugins are loaded from the running FMOD Studio's own install**, wherever it
  is installed, with the standard install folders kept as a fallback.
- The log now shows the full path of any missing bank, and whether bank folders
  are mirrored.

### v0.6.1 — DAT-audio Utilities pack edition
Same renderer and features as v0.6 below. Changes for the pack:
- **Moved into the pack.** The menu item is now **Scripts → DAT-audio Utilities →
  Event Recorder…**, and the script is `Scripts/DAT-audio Utilities/Event Recorder.js`.
- **Install once per machine.** The script finds `Tools/FMOD_Event_Recorder/` next to
  the `Scripts/` folder it was loaded from, instead of always inside the FMOD
  project. So the same files work installed per machine (every project) or per
  project. If its own location can't be determined, it
  falls back to `<project>/Tools/`.
- **Paths with spaces.** Per-machine install paths always contain spaces
  (`Application Support`, `FMOD Studio`). On macOS the launchers now pass their
  script path in `studio.system.start`'s array form, which keeps it as one argument.
- The Script Output log prints the resolved **Tools folder**.
- This README moved to `Tools/FMOD_Event_Recorder/`. Python 3.9+ is stated up front.
- **Version numbers shown:** in the window titles, the Script Output launch log,
  the renderer log (`[Recorder] Version:`) and the setup scripts.

### v0.6
**New features**
- **Windows support (one codebase).** The same `Scripts/` + `Tools/` folders run on
  macOS and Windows. The tool detects the OS and uses `libs/macos/` (`.dylib`) or
  `libs/windows/` (`.dll`), the matching venv Python, and the right launcher. Added a
  Windows installer (`setup.bat`) alongside `setup.command`, and cross-platform plugin
  discovery and "Open Output Folder". *You supply the Windows FMOD DLLs in
  `libs/windows/`.* (Windows path is new — please report issues.)

**Windows bug fixes & robustness** *(from Windows testing)*
- **Launch mechanism.** Dropped the `cmd.exe`/`.bat` (and `.sh`) launcher wrapper.
  The script now runs `python.exe` directly with a single un-quoted argument (the
  renderer defaults to reading `recorder_manifest.json` from its own folder). This
  avoids `studio.system.start`'s single-argument quoting mangling the command — the
  cause of the original *"renderer did not complete"* failure on Windows.
- **Waiting for results.** `studio.system.start` does not block on Windows (it does
  on macOS), and the renderer only launches when FMOD's event loop runs — so a
  busy-wait froze everything. On Windows the tool now shows a **"Rendering…"** dialog
  (whose event loop lets the renderer run) with a **Show Results** button; the results
  file is written **atomically** so it's never read half-written.
- **Renderer crash on Windows (`UnicodeEncodeError`).** When launched by Studio, the
  renderer's stdout is a locale-codepage pipe (cp1252); printing the `─` separators
  raised `UnicodeEncodeError` and killed the process *after* the WAVs but *before*
  writing results. The renderer now redirects output to a UTF-8 `recorder_console.log`
  when not attached to a real console (also a handy debug log). Manual terminal runs
  still print normally.
- **"Open Output Folder"** opened Documents instead of the output folder on Windows
  (double-quoted path) — now passes the path as a single un-quoted argument.
- **Sample-load timing.** The pre-take sample-load wait used a fixed update-count cap;
  NRT updates outran FMOD's async disk load on a cold cache, so recording started
  before the sound played (a long phantom leading silence, later trimmed). The wait is
  now paced against real wall-clock time so loading actually completes (samples now
  load in a few frames instead of hitting the cap).
- **SPF-correction sanity guard.** The two-pass samples-per-frame correction could
  misfire when a sample wasn't loaded in time (absurd take-SPF, e.g. 148 vs a true
  ~515). Output was still correct for single-segment renders (the trim absorbed it),
  but sliced renders (multi-take / sustain-point) could get misplaced cut points. The
  correction is now rejected when it deviates >15% from the directly-measured global
  SPF, falling back to the reliable global value.
- **OS detection (macOS was misdetected as Windows).** The script chose its launcher
  by probing for `venv/Scripts/python.exe` with `fileExists()`, but `getFile/open`
  does not fault on a missing path on macOS, so it wrongly reported Windows and ran
  the Windows launch branch (`execvp` error, nothing happened). The OS is now derived
  from the project-path root (a Windows drive letter like `D:/` vs a Unix `/`) using
  plain string checks — reliable, and parseable by Studio's scripting engine (which
  rejected the regex first tried). Stale comments describing the old venv-probe method
  were removed.

**Improvements**
- **Shared FMOD-library location (optional).** The renderer now resolves the FMOD
  libraries through a search path — `FMOD_LIB_DIR` env var, then a project-level
  `Tools/libs/<platform>/`, then the bundled `Tools/FMOD_Event_Recorder/libs/<platform>/`.
  Several tools can share one copy of the libraries without the tool losing its
  self-contained, drop-in install (the bundled copy is always the fallback). See
  *Setup → Optional*.
- **Scales to large selections.** Previously the dialog added one "Configure…" row
  per event with parameters, which — for big batches (e.g. 48 events) — pushed the
  Record button off-screen (FMOD dialogs have no scroll area). Parameters are now
  reached through a single **"Configure Parameters…"** button that opens a picker
  (a drop-down of the events that expose parameters), so the dialog height stays
  fixed and Record is always reachable no matter how many events are selected.

### v0.5
**New features**
- **Sustain-point splitting.** Events with sustain points are detected and rendered
  as separate parts (`_1_start`, `_2_release1`, …), with a **"Sustain hold (s)"**
  control for how long each point is held before a key-off. The initial `_1_start`
  part records; the post-key-off **release currently renders silent offline and is
  skipped** (no empty file is written) — see Known limitations.

**Improvements**
- **One-click setup** — a `setup.command` installer detects Python 3, installs the
  required libraries, and clears library quarantine. No manual `pip` step.
- Library versions are **unpinned** so the installer works across Python versions
  (e.g. Python 3.9 through 3.13+), each getting compatible builds.
- **FMOD 2.02.x and 2.03.x support** — bundled engine updated to 2.03.20 (renders
  both lines), with automatic engine-version detection.

**Fixes**
- **Plugin effects were stopping events from resolving — fixed.** If a bank used
  plugin effects (AudioGaming, Resonance, etc.) and those plugins weren't
  registered, the whole bank failed to load (`PLUGIN MISSING`) — so none of its
  event paths could resolve and every event in it reported `EVENT NOTFOUND`. The
  renderer now registers FMOD plugins **before** loading banks — auto-discovered
  from your installed FMOD Studio app (matching the engine version), plus any
  `Plugins` folder in your project — so those banks load and their events record.
- **Events that failed with "EVENT NOTFOUND" despite being assigned to a bank now
  record.** Events are resolved by **GUID** — read directly from the metadata bank
  they live in (e.g. `SFX.bank`) — with path lookup kept as a fallback. An event no
  longer needs its path present in a strings bank (`Master Bank.strings.bank`) to be
  recorded. The not-found message is also clearer now.

### v0.4
**New features**
- Per-event parameter detection & configuration (labeled multi-select, continuous
  values, global/local, automation indicator).
- MP3 export (adds to WAV/AIFF/FLAC).
- "Max duration (s)" control for render/loop length.
- Optional loudness/peak normalization (LUFS-I and/or dBFS), off by default.
- "Open Output Folder" button on the results screen.

**Improvements**
- No more file-watcher / separate terminal — the script launches the renderer
  itself. Simpler setup.
- Built-bank paths are read from the project's build settings instead of assuming
  `Build/Desktop/`.
- Verified AIFF/FLAC export end-to-end (previously untested).
- Narrower, tidier dialog.

**Bug fixes**
- Streaming assets (long VO, looping ambience) no longer render silent or
  stuttering (`STREAM_FROM_UPDATE`).
- Renders no longer truncate to ~half the requested duration (timing rework;
  finite events stop naturally, loops hit the requested length exactly).
- Multi-take slices no longer bleed the start of the next take onto the previous one.
- Rendering an event to MP3/AIFF/FLAC no longer deletes a previously-rendered WAV of
  the same name.
- Results dialog can no longer report a stale/false success if a render fails.

### v0.3
First stable multi-take release. (Details reconstructed from development history.)

**New / reworked**
- **Genuine multi-take variation.** All takes of an event render inside a *single*
  FMOD system into one combined WAV that's then sliced into per-take files. (A fresh
  system per take reset FMOD's PRNG, producing identical takes for random /
  multi-instrument events.)
- **Automatic silence trimming** — leading at −96 dB, trailing at −60 dB.
- WAV output, with AIFF/FLAC groundwork.

**Bug fixes** (the multi-take slicing stabilization)
- Fixed **identical takes** for random events (single-system + combined-WAV approach).
- Fixed a **2× slice error** by measuring the real samples-per-update instead of
  assuming the reported DSP buffer size.
- Fixed **slice drift** by removing `flush_sample_loading` calls that wrote uncounted
  frames into the combined WAV.
- Fixed **empty takes (3rd onward)** by re-requesting sample data and polling the
  loading state before each take.
- Added **drain frames** after a take stops, to capture reverb / FX tails.
- Fixed **over-aggressive leading trim** (−80 → −96 dB) and added trailing trim.
- Fixed **pre-roll bias** that shifted slice starts early (two-pass samples-per-update
  correction).

### v0.2
Initial working version. (Reconstructed from history.)

- **FMOD Studio menu script** to select events in the browser + a **Python renderer**
  (pyfmodex, FMOD's non-realtime `WAVWRITER`) that renders selected events to WAV
  offline, without a game runtime.
- Config dialog: output folder, format, sample rate, channels, takes.
- Rendering was launched by a file-watcher process run alongside Studio (later removed
  in v0.5).
- Known issues at this stage — later fixed in v0.3 — included identical multi-takes
  for random events and slice/timing errors.

*(Earlier prototype work prior to v0.2 isn't separately versioned.)*

---

## Future improvements

- True 24-bit / float output (via a float-capture rework of the renderer).
- Load only the banks each event needs (faster on large projects).
- Per-event takes & max-duration.
- Remember last-used settings between runs.
- Async render with progress (so Studio doesn't block on big batches).
- Encode continuous parameter values in filenames / overwrite protection.
- Optionally auto-build banks before recording (`studio.project.build()`).

---

## Package contents

Within the DAT-audio Utilities pack:

```
Scripts/
  DAT-audio Utilities.js            pack loader
  DAT-audio Utilities/
    Event Recorder.js               FMOD Studio menu script
Tools/
  FMOD_Event_Recorder/              this folder
    README.md                       this file
    setup.command                   one-click installer (macOS)
    setup.bat                       one-click installer (Windows)
    fmod_recorder.py                the renderer
    requirements.txt                Python dependencies
    libs/macos/
      PUT_MACOS_DYLIBS_HERE.txt     you add libfmod.dylib + libfmodstudio.dylib here
    libs/windows/
      PUT_WINDOWS_DLLS_HERE.txt     you add fmod.dll + fmodstudio.dll here
```

The Python virtual environment (`venv/`) is created by the setup script and is not
part of this package. The run files `recorder_manifest.json`, `recorder_results.json`,
`recorder_console.log` and `launch.sh` are written here at render time.

---

## Credits & acknowledgements

Written by **Tobey Evans**.

Working out the FMOD scripting and engine methods behind this tool leaned heavily
on the generous public writing of two people, whose material was invaluable for
method support and understanding:

- **Javier Zúmer** — [javierzumer.com](https://javierzumer.com) (his FMOD API articles)
- **Alessandro Famà** — [alessandrofama.com](https://alessandrofama.com) (his FMOD Studio scripting API tutorials)

Reference material consulted:

- FMOD Studio Scripting API reference — <https://www.fmod.com/docs/2.02/studio/scripting-api-reference.html>
- Alessandro Famà, *FMOD Studio Scripting API Basics* — <https://alessandrofama.com/tutorials/fmod/fmod-studio/scripting-api-basics>
- Javier Zúmer, *Introduction to the FMOD API* — <https://javierzumer.com/blog/2021/7/31/introduction-to-the-fmod-api>

Anthropic's **Claude** was used to help automate, tidy, audit, and test the code, and
to help maintain versioning, write and update this README, and create the installers.
