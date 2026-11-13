# FMOD Studio scripts & tools

**DAT-audio Utilities** is a free pack of FMOD Studio menu scripts and companion
apps for sound designers. It adds things FMOD Studio doesn't do on its own:

- **Batch editing:** change tags, notes, macros, shared effects, faders and
  automation across many events at once, and spot the events that differ.
- **Copy and paste between instruments and tracks:** trigger conditions,
  automation and modulation, and track automation lanes, saved as named
  profiles.
- **Project checks in one window:** routing straight to the Master Bus, bank
  assignments, trigger probability, long assets in looping events, duplicate
  event names, and loudness (LUFS-I and true peak) of selected assets.
- **See connections:** a live graph of what uses what, from an event or asset
  through the mixer to the Master Bus, with conditions, automation and a gain
  estimate per route.
- **3D first-person views:** walk the Sandbox from its listener, and replay
  profiler sessions in 3D from the game's listener.
- **Automation curve library:** draw curves and apply them to what's selected,
  bringing 2.04-style curve presets to 2.02 and 2.03.
- **Rendering and clipboard:** record selected events to audio files, and copy
  event names in one go.

Everything appears under **Scripts → DAT-audio Utilities** in FMOD Studio.
The chart below says which tools change your project and which only read it.

## Requirements

- **FMOD Studio 2.02, 2.03 or 2.04.**
- **macOS 11 (Big Sur) or later.** The companion apps are built for Apple
  silicon (M1 or newer); the scripts themselves run wherever FMOD Studio runs.
  Tested on macOS 26 (Tahoe).
- **Windows 10 or 11, 64-bit.** Windows builds of the companion apps are
  included, but they haven't been tested on Windows yet.
- **Record Selected Events** also needs **Python 3.9 or newer** and the
  **FMOD Engine** libraries (a free download from fmod.com, see below). It
  renders projects from FMOD Studio 2.02 and 2.03 with a 2.03 engine.

> [!IMPORTANT]
> **FMOD Engine libraries (`.dll` / `.dylib`) are never included here.** FMOD's
> official position is that people who make tools or scripts must not publish or
> share the FMOD libraries with them. A tool that needs them (today: **Record
> Selected Events…**, the Event Recorder) ships with an empty folder holding a
> `PUT_…_HERE.txt` file, and each user downloads the FMOD Engine from
> [fmod.com](https://www.fmod.com/download) and copies the named files in. See
> *How to install*, step 2.

## What's included

### DAT-audio Utilities — pack v1.7.0 (*Scripts → DAT-audio Utilities*)

| Script | Tool | Version | What it does |
| --- | --- | --- | --- |
| Audit & Detect… | — | 1.0.0 (checks 1.1.0) | Opens one window that runs the project checks listed here, each with what it's for, a live Ready / Not ready line saying what it will scan, and a **Run** button. Stays open while you work. Read-only.<br><br>• **Audit Events Routed Directly to Master Bus** (1.1.0): Lists events that output straight to the Master Bus.<br>• **Audit Events vs. Bank Assignments** (1.1.0): Checks that events are in a suitable bank, using folder-path rules derived from common naming conventions (not tag-based).<br>• **Audit Instrument Probability** (1.1.0): Finds Trigger Probability set pointlessly high (≥ 100 %) or suspiciously low.<br>• **Audit Looping Events for Long Asset presence** (1.1.0): Flags looping events that contain assets over 10 s.<br>• **Detect Duplicate Event Names** (1.1.0): Finds events that share a name anywhere in the project.<br>• **Detect Loudness of Selected Assets** (1.1.0): Reports LUFS-I, true peak and sample peak for the assets and asset folders selected in the Audio Bin window. |
| Copy Selected Event Names | — | 1.0.0 | Copies the selected events' names to the clipboard, without full paths or GUIDs. Useful for copy-pasting many event names at once. |
| Copy-Paste Conditions & Automation | — | 1.5.0 | Copies an instrument's trigger conditions, its automation and modulation, or both at once, onto other instruments, with a visual preview of saved profiles. Also copies a track's automation lanes (volume, pan, insert effects) onto other tracks, adding missing lanes. Has saved, named profiles per project, detects playlist entries in Multi instruments, and can optionally add missing parameter sheets. **Edits the project.** |
| Edit Events in Batch… | — | 0.10.1 | Opens the Batch Event Editor, a live window beside Studio that inspects and batch-edits the events selected in the Event Browser. Shows effect chains (shared and unique), spatializer settings, sheets, macros, tags and notes across the selection, groups events that share a master chain, and writes a Differences brief naming outliers and the event that differs most. Edits tags, notes, macros, shared effects and track faders and automation across the selection or one group, with previews, read-back and batch Undo. Built to standardise events on shared effects. **Edits the project.** |
| Open Automation Curve Library… | [`AutomationCurveLibrary/`](Tools/AutomationCurveLibrary) (Curve Library app) | 0.9.7 (script 0.8.5) | Opens the Curve Library, a companion app for drawing automation curves and applying them to what's selected in Studio: effects, instruments, tracks, buses. Brings 2.04-style curve presets to 2.02 and 2.03, and reads and writes 2.04's own. 99 Factory curves plus your own libraries. macOS (Apple silicon) and 64-bit Windows. **Edits the project.** |
| Open Profiler First Person View… | [`ProfilerFirstPerson/`](Tools/ProfilerFirstPerson) (3D view app) | 0.6.0 (script 0.5.0) | Opens a floating first-person 3D view of a profiler session: a 3D version of the Profiler's 3D View panel, from the game's listener at the playhead. Shows every event instance with its min/max distance spheres in the event's colour, the listener(s) and the listener probe, and follows play and scrubbing. A camera offset matches the game's camera; an Events window and selected profiler tracks choose what's drawn. Needs recordings made with API capture; unsaved sessions and recordings in progress work too. A detachable camera shows the listener's view from outside. macOS (Apple silicon) and 64-bit Windows. Read-only. |
| Open Reference Viewer… | [`ReferenceViewer/`](Tools/ReferenceViewer) (viewer app) | 0.5.4 (script 0.5.4) | Opens a live window beside Studio that follows what you select and shows everything connected to it, as a graph (signal flow, left to right) or a list: the events that use an event and the events it uses, chained to the top-level events and on through the mixer to the Master Bus, with each link's conditions, automation, modulation, track and effects; every use of a parameter; what a snapshot changes and what starts it; where an asset, shared effect, bus, return, send or VCA is used. Shows bank status and Studio colours, and a stage-by-stage gain estimate per route (curve maths measured against the FMOD runtime). macOS (Apple silicon) and 64-bit Windows. Read-only. |
| Open Sandbox First Person View… | [`SandboxFirstPerson/`](Tools/SandboxFirstPerson) (3D view app) | 0.3.0 (script 0.3.0) | Opens a floating first-person 3D view of the open Sandbox scene, seen from its listener and kept in sync with it both ways: fly with WASD and the mouse, or drag in the Sandbox. Shows every emitter's min/max distance and sound size as spheres, and has auto spin/circle movements. An optional, off-by-default *Elevation (beta)* fakes height by temporarily moving emitters along the ground so every one is heard at its true 3D distance. macOS (Apple silicon) and 64-bit Windows. **Moves the Sandbox listener** (and, with Elevation on, emitters; one undo step per session). |
| Record Selected Events... | [`FMOD_Event_Recorder/`](Tools/FMOD_Event_Recorder) (Python renderer) | 0.6.5 | Renders the events selected in the Event Editor window to audio files in the project's `Recordings` folder. |

Usage details, setup and release notes:
[`Scripts/DAT-audio Utilities/README.md`](Scripts/DAT-audio%20Utilities/README.md).

Every tool shows its version in its window title or in its first line of
**Scripts → Show Script Output**.

## How to install

Download the latest `DAT-Audio_FMOD_Utilities-<version>.zip` from this repository's
**Releases** page. Cloning or downloading the source gets the scripts but not the
built companion apps, which are only in the release zip.

You can install **per machine**, so the tools are there in every project you
open (recommended), or **per project**, so they only appear in one project (see
*Installing per project* below).

**Per machine:** copy its **`Scripts`** and **`Tools`** folders, merging with
any already there, into the per-user FMOD folder for each FMOD version you use:

| FMOD | macOS | Windows |
| --- | --- | --- |
| **2.02.x** | `~/Library/Application Support/FMOD Studio/` | `%LOCALAPPDATA%\FMOD Studio\`, by default `C:\Users\<YourUsername>\AppData\Local\FMOD Studio\` |
| **2.03.x** | `~/Library/Preferences/FMOD Studio/` | `%LOCALAPPDATA%\FMOD Studio\`, by default `C:\Users\<YourUsername>\AppData\Local\FMOD Studio\` |
| **2.04.x** | `~/Library/Preferences/FMOD Studio/` (same as 2.03) | `%LOCALAPPDATA%\FMOD Studio\`, by default `C:\Users\<YourUsername>\AppData\Local\FMOD Studio\` |

- **macOS:** 2.02 reads a different folder from 2.03 and 2.04, so if you use
  2.02 alongside a later version, copy into both. Alternatively, copy into
  Application Support only and link the Preferences folder to it, once:
  ```sh
  cd ~/Library/Preferences/"FMOD Studio" && rmdir Scripts 2>/dev/null
  ln -s ~/Library/"Application Support"/"FMOD Studio"/Scripts Scripts
  ln -s ~/Library/"Application Support"/"FMOD Studio"/Tools   Tools
  ```
  In Finder, open these folders with **Go → Go to Folder…** (⇧⌘G).
- **Windows:** one folder serves every version. `%LOCALAPPDATA%` is your own
  user's local AppData folder, so the default per-machine location is
  `C:\Users\<YourUsername>\AppData\Local\FMOD Studio\`. `AppData` is hidden
  in File Explorer by default, so the easiest way in is to paste
  `%LOCALAPPDATA%\FMOD Studio` into File Explorer's address bar. (Not yet
  tested on Windows.)
- **Then, in FMOD Studio, choose Scripts → Reload.** Restarting Studio also
  works, and is a good idea if it was already running. If an updated tool still
  shows its old version in its window title after Reload, restart Studio.
- **Add the FMOD Engine libraries for tools that need them.** These aren't
  included, because FMOD's position is that tool and script makers must not
  publish or share them; you download your own copy, under your own acceptance
  of FMOD's licence. Today only the Event Recorder needs them:
  1. Sign in at [fmod.com/download](https://www.fmod.com/download) (free) and
     download the **FMOD Engine** for your OS. 2.03.x is recommended: it works
     with projects from FMOD Studio 2.02 and 2.03.
  2. Copy the **release** libraries (not the ones ending in `L`) into the folder
     marked with a `PUT_…_HERE.txt` file:

     | OS | Files (from the FMOD Engine download) | Into |
     | --- | --- | --- |
     | macOS | `api/core/lib/libfmod.dylib`, `api/studio/lib/libfmodstudio.dylib` | `Tools/FMOD_Event_Recorder/libs/macos/` |
     | Windows | `api\core\lib\x64\fmod.dll`, `api\studio\lib\x64\fmodstudio.dll` | `Tools\FMOD_Event_Recorder\libs\windows\` |

  Do this before the tool's own setup step. Full details, including ARM64
  Windows, are in `Tools/FMOD_Event_Recorder/README.md` (*Setup*, step 2).
- Some tools need a one-time setup step; see each pack's own README.

**Updating:** download the newer release zip and copy its `Scripts` and `Tools` folders **over** the old
ones, replacing files when asked, rather than deleting anything. Any setup a
tool keeps in `Tools/`, including the FMOD libraries you added, is preserved. Then choose **Scripts → Reload** in FMOD
Studio.

> **macOS Finder:** dragging a folder onto one with the same name offers
> **Replace**, which deletes the old folder's contents first. Hold **Option**
> while dragging and choose **Merge** instead. (Windows File Explorer merges
> folders by default.)

### Installing per project

FMOD Studio also loads scripts from a `Scripts` folder inside a project's
folder, and the tools look for their apps in the `Tools` folder next to
whichever `Scripts` folder they were loaded from. So you can install into a
single project instead:

1. Copy the **`Scripts`** and **`Tools`** folders into the project's folder,
   the one that holds its `.fspro` file:

   ```
   MyProject/
     MyProject.fspro
     Scripts/
     Tools/
   ```
2. Add the FMOD Engine libraries and do any one-time tool setup as above, in
   the project's `Tools` folder.
3. Open the project, or choose **Scripts → Reload** if it's already open.

Things to know before choosing this:

- **Don't install both ways at once.** A project with its own copy, opened on a
  machine that also has the per-machine install, shows every menu item twice.
  Pick one.
- **Version control:** if the project is in Git, Perforce or similar, the
  `Tools` folder adds about 140 MB of apps. Commit it if the whole team should
  get the tools with the project; otherwise ignore `Scripts/` and `Tools/` there.
  Never commit the FMOD Engine libraries you add to `Tools/`.
- **Everything is per project:** each project needs its own copy to update, its
  own tool setup (the Event Recorder's one-time setup runs once per copy), and
  its own Curve Library user libraries, which live in
  `Tools/AutomationCurveLibrary/Libraries/`.
- **To update,** copy the newer `Scripts` and `Tools` over the project's copies,
  merging as above.

## Licence

The DAT-audio Utilities pack and its tools are released
under the MIT licence: see [`LICENSE`](LICENSE), which lists exactly what it
covers. The FMOD Engine and FMOD Studio aren't included and stay under FMOD's
own licence.

The four companion apps are built with open-source Rust libraries. Each app's
folder has a `THIRD_PARTY_NOTICES.txt` with those libraries' licences, which
should be kept with the app when it's shared.
