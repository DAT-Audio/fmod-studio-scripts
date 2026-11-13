# DAT-audio Utilities

FMOD Studio script pack, **version 1.7.0**. All tools appear under **Scripts →
DAT-audio Utilities**.

By **Tobey Evans**, who maintains the pack and its tools.
Released under the MIT licence (see `LICENSE` two folders up).

**Install and update:** see the `README.md` two folders up, at the top of the
scripts-and-tools folder. The versions below match the ones listed there.

Rows marked ↳ are checks inside the **Audit & Detect…** window, not menu items.

| Menu item | Version | File |
| --- | --- | --- |
| Audit & Detect… | 1.0.0 | `Audit & Detect.js` |
| ↳ Audit Events Routed Directly to Master Bus | 1.1.0 | `Audit & Detect/Audit Events Routed Directly to Master.js` |
| ↳ Audit Events vs. Bank Assignments | 1.1.0 | `Audit & Detect/Audit Events vs Bank Assignments.js` |
| ↳ Audit Instrument Probability | 1.1.0 | `Audit & Detect/Audit Instrument Probability.js` |
| ↳ Audit Looping Events for Long Asset presence | 1.1.0 | `Audit & Detect/Audit Looping Events for Long Asset presence.js` |
| ↳ Detect Duplicate Event Names | 1.1.0 | `Audit & Detect/Detect Duplicate Event Names.js` |
| ↳ Detect Loudness of Selected Assets | 1.1.0 | `Audit & Detect/Detect Loudness of Selected Assets.js` |
| Copy Selected Event Names | 1.0.0 | `Copy Selected Event Names.js` |
| Copy-Paste Conditions & Automation | 1.5.0 | `Copy-Paste Conditions & Automation.js` |
| Edit Events in Batch… | 0.10.1 | `Edit Events in Batch.js` |
| Open Automation Curve Library… | 0.9.7 (script 0.8.5) | `Open Automation Curve Library.js` + `../../Tools/AutomationCurveLibrary/` |
| Open Profiler First Person View… | 0.6.0 (script 0.5.0) | `Open Profiler First Person View.js` + `../../Tools/ProfilerFirstPerson/` |
| Open Reference Viewer… | 0.5.4 (script 0.5.4) | `Open Reference Viewer.js` + `../../Tools/ReferenceViewer/` |
| Open Sandbox First Person View… | 0.3.0 (script 0.3.0) | `Open Sandbox First Person View.js` + `../../Tools/SandboxFirstPerson/` |
| Record Selected Events… | 0.6.5 | `Event Recorder.js` + `../../Tools/FMOD_Event_Recorder/` |

All tools are **read-only** (they inspect the project and report) **except
Copy-Paste Conditions & Automation**, which edits the instruments and tracks you paste
onto, **Edit Events in Batch**, which edits the events you apply it to
(tags, notes, macros, effects, track faders and automation), **Open Automation
Curve Library**, which writes automation when you press Apply, **and Open Sandbox First Person View**, which moves the Sandbox
listener (and, with Elevation on, emitters; one undo step per session). Event Recorder doesn't change the project either; it writes audio files
to a folder you choose.

**Menu names** start with a verb that says what kind of job the tool does:
**Audit** checks the project and reports problems, **Detect** finds or
measures something, **Copy** works with the clipboard, **Edit** changes
events in batch, **Record** renders audio, and **Open** opens a companion
app. The Audit and Detect checks are gathered in one window, **Audit &
Detect…**, rather than listed one by one in the menu. New tools follow the
same pattern (see *Adding scripts* in the top-level
README).

## Install

Install as described in the `README.md` two folders up: copy `Scripts` and
`Tools` into the per-user FMOD folder for each FMOD version, then choose
**Scripts → Reload** in FMOD Studio (or restart it). For DAT-audio Utilities
specifically:

- **Check it loaded:** **Scripts → Show Script Output** shows
  `[DAT-audio Utilities] v1.7.0 loaded from …`, which also tells you which
  install is active.
- **Event Recorder setup (once per install location):** first add the FMOD
  Engine libraries to `Tools/FMOD_Event_Recorder/libs/macos/` or `libs/windows/`.
  They're not included because FMOD's position is that tool and script makers must
  not publish or share them; you download your own from
  [fmod.com](https://www.fmod.com/download). Then run
  `Tools/FMOD_Event_Recorder/setup.command` (macOS) or `setup.bat` (Windows).
  It needs **Python 3.9 or newer**. See `Tools/FMOD_Event_Recorder/README.md`,
  *Setup*.
- **When updating:** copy the new files over the old ones (merge, don't replace).
  Event Recorder's `venv/` folder and the FMOD libraries you added are kept, so
  its setup doesn't need re-running.

## The tools

### Audit & Detect

**Audit & Detect…** (v1.0.0) opens one window holding all the project checks.
Each check has a card saying what it's for, a status line and a **Run**
button. The status line follows your selection live: green **Ready:** says
what Run will scan (for example *Ready: scans the whole project. No selection
needed.*), and orange **Not ready:** says what to select first. A choice
you can make follows in blue, as **Optional:** (Instrument Probability can
be limited to the events or folders you select). The window
stays open while you work in Studio, so you can change the selection and run
another check. Each check shows its results in its own scrollable window
and mirrors them to **Scripts → Show Script Output**. None of them change the
project.

The checks:

**Audit Events Routed Directly to Master Bus** (v1.1.0) lists every event whose output
goes straight to the Master Bus instead of through an intermediate bus.

**Audit Events vs. Bank Assignments** (v1.1.0) checks that each event appears to be in a
suitable bank, based on keywords in its folder path. The keyword rules are a
table at the top of the script; extend it to match your project's naming
conventions.

**Audit Looping Events for Long Asset presence** (v1.1.0) flags looping events that
contain an asset longer than 10 seconds. "Looping" means a loop region on the
timeline, or (for events with no timeline) a continuous parameter, which is
reported as "possibly looping". Events with no loop keyword in their name or
path are listed first, and Music/VO events get their own sub-section.

**Audit Instrument Probability** (v1.1.0) reports instruments with Trigger Probability
switched on but set to a value that's almost certainly unintended. **Redundant**
means it's at or above 100%, so it always plays and the gate does nothing.
**Suspiciously low** means it's under 10%, which is usually a typo. It scans
the whole project or just the Event browser selection, optionally following
event-reference instruments out of the selection.

**Detect Duplicate Event Names** (v1.1.0) reports events that share a name anywhere in
the project. Game code that refers to events by name alone can't tell these
apart. Exact duplicates and case-only duplicates (`Footstep` vs `footstep`) are
listed separately.

**Detect Loudness of Selected Assets** (v1.1.0) works on the **Assets** tab, not events. Select
assets or folders and it reports **LUFS-I**, **true peak (dBTP)** and **sample peak (dBFS)**
per asset, with a Copy-as-TSV button for spreadsheets. The loudness maths (ITU-R BS.1770-4)
is implemented in the script, so there's nothing to install. That's also why it reads
**uncompressed PCM only**: WAV and AIFF. Ogg/MP3/FLAC assets are listed as skipped. It is
CPU-bound and blocks Studio while it runs, so it asks for confirmation on large selections.

### Editing

**Copy-Paste Conditions & Automation** (v1.5.0) opens a window that stays open while you
keep working in Studio. Select an instrument in the event editor, and it's
highlighted at the top of the window. Tick what to **Include** (its **trigger
conditions**, its **automation & modulation**, or both) and press **Copy**.
Then select one or more other instruments, in any event, and press **Paste**.

- Works with every instrument type: single, multi, event (referenced),
  scatterer and the rest.
- **Conditions:** parameter conditions (with labels and the inverted flag),
  event-state conditions, and the condition mode.
- **Automation & modulation:** every curve and point, every modulator with all
  its settings, and automation on a modulator's own settings. For event
  instruments, it also covers automation of the nested event's parameters.
- **A modulator is never doubled up.** If the target already has that type on
  that property (e.g. Random on Pitch), the existing one is updated instead.
- **Only the instrument itself.** Copying from an instrument never copies or
  changes track effects or their automation. Copy from the track for that
  (next point).
- **Track automation (1.5.0):** select automation lanes under a track header
  (or the whole track, or one of its effects; the Fader means the Volume
  lane) with **Automation & Modulation** ticked, and press **Copy**. Then
  select one or more other tracks, in any event, and press **Paste**. Volume,
  pan, insert-effect and plug-in lanes go onto the same place on each track
  (the same effect type, in the same order), and a missing lane is added.
  **Effects are never added:** a lane on an effect the target track doesn't
  have is skipped and reported. Replace swaps that lane's curves, merge swaps
  only curves on the same parameters, and other lanes are never touched.
  Instruments win when both are selected.
- Pasting into another event moves timeline automation onto that event's
  timeline. Conditions and automation use the project's parameter directly, so
  the target event doesn't need a parameter sheet for them. The report notes
  each parameter that got no sheet with `=`.
- **Include** decides what Copy captures and what Paste pastes. Copy replaces
  the whole clipboard with the ticked parts, so a paste never mixes two
  sources. Untick a part to paste only the other. Loop regions and transition
  markers take conditions only.
- **Add missing parameter sheets** (off by default): tick it to also add a
  parameter sheet to the target event for each parameter it doesn't have one
  for.
- **Preview** (right of the controls): draws exactly what Paste will paste,
  the source chosen under Paste from. Each
  condition is a card with a green range bar across the parameter's full range
  (one segment per label for labelled parameters, and an *Inverted* tag);
  each automation curve is a small chart that follows its curve shapes
  (bent and S-curves, approximated); AHDSR modulators show their envelope; other modulators are tags with their settings. A part that isn't
  ticked under Include is flagged "won't paste".
- **Paste from: clipboard or profile.** Copy fills the clipboard, which lasts
  until Scripts → Reload, and chooses it under **Paste from**. Pick a saved
  profile there instead, and Paste pastes that profile. A profile keeps both
  parts. To keep a set, press **Save as…** and name it. **Manage…** renames,
  replaces (with the clipboard) or deletes profiles. Copy never
  changes a profile. Profiles are saved per user and per project, in
  `.user/Copy-Paste Conditions & Automation profiles.json` inside the project
  folder (FMOD keeps `.user` out of source control). They only apply to the
  project they were saved in.
- "Replace existing" (on by default) clears the target's conditions, or its
  automation and modulation, before pasting. Untick it to merge.
- **Instruments inside a Multi or Scatterer playlist:** select the Multi in the
  event editor, then pick the entry in the playlist below it. The window shows
  the entry and says it came from the playlist. To target the whole Multi
  again, deselect the entry or click elsewhere in the event editor.

Every paste writes a report to the window and to Show Script Output. A paste is
made of many small edits, so Undo may need several steps to reverse one.

**Open Automation Curve Library…** (v0.9.7, script 0.8.5) starts the Curve
Library app and a small **Curve Library link** window in Studio; keep the link
window open while you work. Pick or draw a curve on the left, select what to
automate in Studio, then choose the property, parameter and ranges under
*Apply curve to* and press **Apply**.

- **Targets:** effect inserts and shared effects, instruments, track and bus
  volume, an event's master track, or a selected automation curve. Several
  selected things can take the same curve at once. An existing curve on that
  property and parameter is replaced in place, and **Undo last apply**
  reverses it.
- **Libraries** live with the tool, in `Tools/AutomationCurveLibrary/Libraries/`:
  one folder per library, one file per curve, so a library is shared by
  copying its folder. *Factory* is read-only. Curves are stored 0 to 1, so a
  shape fits any range.
- **Curves already in Studio** are drawn under the thing they're on; pull one
  into the graph to edit it, or save it to a library.
- **Spatializers:** their distance attenuation can't be replaced by a curve in
  FMOD, so the app shows it and explains the way round it.
- Runs on macOS (Apple silicon) and 64-bit Windows; the script picks the
  right app. The apps aren't signed yet, so macOS or Windows may ask you to
  allow them once. Full guide: in the app (**Guide**, or F1), and
  `Tools/AutomationCurveLibrary/README.md` and `GUIDE.md`.

**Edit Events in Batch…** (v0.10.1) opens the **Batch Event Editor**, a
window that stays open beside Studio and follows the events (or event
folders) selected in the Event Browser's **Events** tab. It's built to
**standardise events on shared effects**: spot unique effects and
inconsistent setups, then fix them across many events at once.

- **Inspect:** the views show every effect chain (shared and unique effects,
  bypass, automation), spatializer settings, sheets, macros, tags and notes,
  as distributions across the selection. Events with the same master track
  effect chain are grouped A, B, C…, so patterns and outliers stand out.
  The views never change the project.
- **Differences:** a written brief of how the selected events differ, which
  ones are outliers, and which event differs the most and why.
- **Edit:** **Tags… / Notes… / Macros… / Effects… / Tracks…** act on the
  whole selection or one group (**Apply to**). Each previews its changes
  before Apply, and every write is read back. Effects… removes, adds or
  replaces shared effects (it never creates unique ones); Tracks… sets
  fader levels and clears automation.
- **Undo** reverts the tool's last batch, up to 10. Studio can't undo a
  script's batch in one step, so **commit `Metadata/` before big batches**.
- **Export TSV** copies one row per event to the clipboard for a
  spreadsheet audit.
- It doesn't create or edit shared effects themselves (use Studio's Shared
  Effects browser), or edit instruments (use Copy-Paste Conditions &
  Automation). Pure script, nothing to install. A **Guide…** button in the
  window explains the views, labels and every editor.

### Auditioning

**Open Sandbox First Person View…** (v0.3.0, script 0.3.0) opens a floating
first-person 3D view of the Sandbox scene you're in, seen from its listener,
plus a small **First Person link** window in Studio; keep the link window
open while you work. Flying in the view (WASD + mouse) moves the Sandbox
listener live, and dragging the listener or emitters in the Sandbox moves the
view.

- **Emitters** are drawn with their min/max distance spheres, sound size, and
  any extra spatializers on other tracks (preset spatializers included). The
  emitter under the crosshair gets a card with its event path, ranges, rolloff
  and what follows the built-in Distance parameter. Snapshots and 2D events
  are labelled as such.
- **Speed and Scale** (top bar) suit any project's units: speed from 0.01 to
  100 units/s, and Scale resizes the grid and markers.
- **Auto movements:** spin on the spot (CW/CCW, slow/fast) and walk a circle
  (CW/CCW, radius and speed; beside you or round the emitter under the
  crosshair). Any manual move stops them.
- **Elevation (beta), off by default:** the Sandbox has no height, so while
  you're above or below ear height the tool moves emitters away from the
  listener along the ground, in the Sandbox view. Each keeps its left/right
  offset and is heard at its true 3D distance, which is exact for every
  emitter at once. It explains this before turning on, and puts emitters
  back when you land, turn it off, switch scene or close the view. Don't
  save while elevated. It only moves emitters that react to distance (a
  distance curve, or built-in Distance automation such as snapshot
  intensity). It never moves 2D events, curve-Off spatializers or snapshots,
  and doesn't drive custom height parameters such as `CameraAltitude`.
- **Undo:** a whole first-person session (listener and emitter moves) is
  one undo step, and your own edits in between are never undone.
- Runs on macOS (Apple silicon) and 64-bit Windows;
  the script picks the right app. The apps aren't signed, so macOS or
  Windows may ask you to allow them once. Full details:
  `Tools/SandboxFirstPerson/README.md`.

**Open Profiler First Person View…** (v0.6.0, script 0.5.0) opens a floating
first-person 3D view of the profiler session you've selected, plus a small
**Profiler First Person link** window; keep it open while you use the view.
It's a 3D version of the Profiler's 3D View panel, seen from the game's
listener at the playhead.

- **It reads the game's recording:** every recording made with **API
  capture** on is saved in the session's folder, and the positions come from
  there, so sounds above or below the listener are drawn where they were.
  Sessions you haven't saved yet work too, including a recording still in
  progress (marked *(live)*).
- **Instances** get their min/max distance (and optionally sound size)
  spheres, in their event's Browser colour, drawn as lines or shells. The
  instance under the crosshair gets a card with its path, distance, state,
  child events and spatializers.
- **Follows the Profiler:** the playhead while it moves, otherwise the
  cursor, so play, scrub and click all work. Selecting profiler tracks draws
  only those events; the **Events…** window can limit it to ticked events.
- **Camera offset:** the game's camera isn't in the capture, so you can move
  the view off the listener (e.g. back and up for a third-person game). The
  listener and its probe (attenuation position) are then drawn.
- **Detached camera:** detach the view and fly a camera of your own (drag to
  look, W A S D to move) to see the listener from outside, with its field of
  view drawn as an orange frustum. **Reset to listener** couples it back.
- **Read-only:** it never changes the project, so there's nothing to undo.
- Runs on macOS (Apple silicon) and 64-bit Windows; the apps aren't signed,
  so macOS or Windows may ask you to allow them once. Full details:
  `Tools/ProfilerFirstPerson/README.md`.

### Inspecting

**Open Reference Viewer…** (v0.5.4, script 0.5.4) opens a live window beside
Studio, plus a small **Reference Viewer link** window; keep the link window
open while you use it. Select something in Studio and the viewer shows
everything connected to it, without right-click → Find References each time.

- **Events:** the events it uses and the events that use it, chained to the
  top-level events, then on through each top-level event's mixer route to the
  Master Bus and the snapshots it starts. Each link shows its trigger
  conditions (met or not at the current parameter values), automation (with
  its current value), modulation, passed parameter values, track, effects and
  any Multi or Scatterer it sits in.
- **Also:** parameters (every condition, automation, macro, modulation and
  passed value that uses it), snapshots (what they change and what starts
  them), assets, shared effects, buses, returns, sends and VCAs. Several
  selected items show together.
- **Graph or list**, sound flowing left to right. Drag to pan, pinch or
  Cmd/Ctrl+scroll to zoom, click a box to light its paths, double-click to
  open it in Studio, right-click for more. Boxes show their kind, folder,
  bank status ("bank X" / "no bank") and their colour from Studio.
- **Gain:** a stage-by-stage estimate of each route's level, from the
  instrument to the Master Bus, at default and current parameter values. The
  curve maths was measured against the FMOD 2.03 runtime. It's an estimate
  from the project's settings, not a meter.
- **Read-only:** it never changes the project.
- Runs on macOS (Apple silicon) and 64-bit Windows; the apps aren't signed, so
  macOS or Windows may ask you to allow them once. There's a **Guide** in the
  app, and full details in `Tools/ReferenceViewer/README.md`.

### Rendering

**Record Selected Events…** (Event Recorder, v0.6.5) batch-renders the events selected in the Event browser to
audio files (WAV, AIFF, FLAC or MP3), offline, from your built banks. It
handles per-event parameters (one file per selected label), multiple takes,
and optional LUFS/peak normalization. Unlike the other tools it has a Python
part, which lives in `Tools/FMOD_Event_Recorder/`.

- **It renders from the project's built banks**, read from wherever that
  project builds them: its build output directory (even if that's outside the
  project folder), its platform subfolder, and mirrored bank folders when the
  project builds with the Banks browser's folder hierarchy. Build the banks
  first. Events are recorded as they were at the last build.

- **Requires Python 3.9 or newer and the FMOD Engine libraries**, which you
  download from fmod.com yourself: FMOD doesn't allow tool makers to share them, so
  they aren't in this pack. Put them in `Tools/FMOD_Event_Recorder/libs/macos/` or
  `libs/windows/` (each has a `PUT_…_HERE.txt` naming the files), then run the
  one-time setup: `setup.command` (macOS) or `setup.bat` (Windows) in
  `Tools/FMOD_Event_Recorder/`.
- Full instructions: `Tools/FMOD_Event_Recorder/README.md`.

### Clipboard

**Copy Selected Event Names** (v1.0.0) is a deliberately simple, one-click tool rather than an audit.
Select events (or folders) in the Event browser and it puts their **names** on the
clipboard, one per line, with **no GUIDs and no `event:/` paths**. That's just the bare
names, ready to paste into a spreadsheet, a ticket or game code. Folders are recursed, the
list is sorted alphabetically, and an event selected twice (directly and via its folder) is
listed once.

## What's new in 1.7.0

- **Copy-Paste Conditions & Automation** 1.5.0: copies **track automation**.
  Select automation lanes under a track (or the whole track) and Copy, then
  select other tracks and Paste. Volume, pan, insert-effect and plug-in lanes
  are matched by place, missing lanes are added, and effects are never added
  (a lane on an effect the target lacks is reported). Saved profiles keep
  track automation too. Also fixed: the *Automation & Modulation* checkbox
  read "Automation _Modulation".

## What's new in 1.6.2

- **Open Profiler First Person View…** 0.6.0: a new **DETACHED CAMERA**
  section in the settings panel. **Detach** gives you a free camera to fly
  around (drag to look; W A S D, E / Q to move; Shift faster, Option slower)
  so you can see where the listener is and what its field of view covers,
  drawn as an orange frustum in the view and on the map. **Behind the
  listener** puts the camera back behind it, and **Reset to listener**
  couples the view back to the listener. The Studio script is unchanged.

## What's new in 1.6.1

- **Open Profiler First Person View…** 0.5.0: works with profiler sessions
  that haven't been saved yet. Studio keeps those in the project's `.unsaved`
  folder until you save, and the view only looked in the saved one, so a new
  session showed "No recordings with API capture". It now reads both, keeps
  going when you save mid-session, and follows a recording still being
  written (the status line says *(live)*).

## What's new in 1.6.0

- **New: Audit & Detect…** (1.0.0). The six Audit and Detect tools have moved
  out of the menu into one window, with a card per check that says what it's
  for, a live **Ready:** / **Not ready:** status line, and a **Run** button.
  The window stays open while you work.
- The checks themselves are unchanged apart from now running from that
  window (all 1.1.0). Their files are in `Audit & Detect/`.

## What's new in 1.5.0

- **New: Edit Events in Batch…** (0.10.1). Opens the Batch Event Editor, a
  live window that inspects and batch-edits the selected events: effect
  chains, spatializers, sheets, macros, tags and notes, grouped by master
  chain, with a Differences brief that names outliers and the event that
  differs most. Edits tags, notes, macros, shared effects and track faders
  and automation across a selection or a group, with previews, read-back
  and batch Undo. **Edits the project.** See *Editing* above.

## What's new in 1.4.0

- **New: Open Reference Viewer…** (0.5.4). A live window that follows what
  you select in Studio and shows what's connected to it: the events that use
  an event and the events it uses (on through the mixer to the Master Bus),
  with each link's conditions, automation, modulation, track and effects;
  every use of a parameter; what a snapshot changes; where an asset, shared
  effect, bus, return, send or VCA is used. Graph or list views, bank status,
  Studio colours, and a stage-by-stage gain estimate per route. Read-only.
  See *Inspecting* above.

## What's new in 1.3.6

- **New: Open Profiler First Person View…** (0.4.0). A floating first-person
  3D view of a profiler session: a 3D version of the Profiler's 3D View
  panel, seen from the game's listener at the profiler's playhead. It reads
  the positions from the session's API capture, so height is real. Every
  event instance gets its min/max distance spheres in its Event Browser
  colour, and the listener probe is shown when the game sets one. It follows
  play and scrubbing, has a camera offset for third-person games, and an
  Events window and selected profiler tracks choose what's drawn. Read-only.
  See *Auditioning* below.
- Sandbox First Person and Automation Curve Library: rebuilt Windows apps
  with no local file paths inside (same versions), and "How it works" /
  "What we had to build ourselves" sections in their READMEs.

## What's new in 1.3.5

- **Open Sandbox First Person View… (0.3.0): Elevation, rebuilt.**
  - **The old approach:** flying up or down moved the listener to a best fit,
    which can't be right for a row of emitters (e.g. street emitters).
  - **The new approach:** the new **Elevation (beta)** moves each
    distance-reactive emitter along the ground instead. Each one keeps its
    left/right offset and is heard at its true 3D distance, which is exact
    for every emitter at once.
  - **Off by default:** it explains how it works before turning on, and while
    it's off you stay at ear height. Emitters go back in place when you land,
    turn it off, switch scene or close the view.
  - **Toolbar:** now in sections (VIEW, SHOW, MOVE, SPIN, CIRCLE), with orange
    active buttons.

## What's new in 1.3.4

- **New: Open Sandbox First Person View…** (0.2.0). A floating first-person
  3D view of the open Sandbox scene, from its listener, kept in sync with the
  Sandbox both ways. Emitters are shown with their min/max distance and sound
  size, auto spin/circle movements are built in, and a beta *Height fit*
  makes flying up or down change what you hear. See *Auditioning* below.

## What's new in 1.3.3

- **Open Automation Curve Library… (script 0.8.5): selecting the Fader
  works.** The Fader's Volume knob is the volume of its strip (master track,
  track, bus or return), so a selected Fader now targets that strip with
  Volume picked. On 2.02 it offered nothing to apply to before.

## What's new in 1.3.2

- **The FMOD Engine libraries are no longer included** with Record Selected
  Events… (Event Recorder 0.6.5), on macOS as well as Windows. FMOD's position
  is that tool and script makers must not publish or share them. Download the
  FMOD Engine from [fmod.com](https://www.fmod.com/download) and copy two files
  into `Tools/FMOD_Event_Recorder/libs/macos/` or `libs/windows/`; each folder's
  `PUT_…_HERE.txt` names them. If you're updating an install that already has
  them, merge-copy as usual and they're kept.
- `setup.command` / `setup.bat` now check for the libraries first and say where
  to get them and where they go. The renderer's "libraries not found" error and
  the "renderer did not complete" alert say the same.

## What's new in 1.3.1

- **Open Automation Curve Library… now runs on Windows** (0.9.7, script
  0.8.4). `Tools/AutomationCurveLibrary/` holds both apps, `curve_library`
  for macOS and `curve_library.exe` for Windows, and the script starts the
  one for the platform Studio is running on. Both share the same
  `Libraries/`. No DLLs or other files are needed. The Windows app isn't
  signed yet, so Windows may ask you to allow it once.

## What's new in 1.3.0

- **New: Open Automation Curve Library…** (0.9.6, script 0.8.3). A companion
  app with libraries of automation curves you draw with the mouse or
  trackpad and apply to whatever is selected in Studio. It brings FMOD 2.04's
  curve presets and precision editing to 2.02 and 2.03, and reads and writes
  2.04's own presets. It ships with 99 read-only Factory curves. macOS on
  Apple silicon only for now. See *Editing* below and
  `Tools/AutomationCurveLibrary/README.md`.
- **If you had it installed on its own** (`Scripts/fmod_automation_curve_library.js`
  at the top of a Scripts folder), delete that file, or the menu item appears
  twice. Your libraries in `Tools/AutomationCurveLibrary/Libraries/` carry over.

## What's new in 1.2.1

Three menu items are renamed so each starts with a verb that says what kind of
job it does: **Audit** checks the project and reports problems, **Detect**
finds or measures something, **Copy** works with the clipboard, and **Record**
renders audio. Only the names changed.

- **Check Instrument Probability** is now **Audit Instrument Probability**
  (1.0.1), and its file is renamed to match.
- **Measure Loudness of Selected Assets** is now **Detect Loudness of Selected
  Assets** (1.0.1), and its file is renamed to match.
- **Event Recorder…** is now **Record Selected Events…** (0.6.4). The tool, its
  windows and its files keep the name Event Recorder.

The old `Check Instrument Probability.js` and `Measure Loudness of Selected
Assets.js` can be deleted from an installed copy; the loader no longer reads
them.

## What's new in 1.2.0

- **Copy-Paste Conditions & Automation 1.4.0** has a new layout:
  - **A Preview pane** to the right draws exactly what Paste will paste: a
    card per condition with a range bar, like Studio's Trigger Behavior
    drawer, a chart per automation curve, and a tag per modulator.
  - **One Copy and one Paste.** Two **Include** checkboxes, *Trigger
    Conditions* and *Automation & Modulation*, choose what they handle, so both
    can be copied and pasted in one go.
  - **Two zones.** *1 Copy*: Include, Copy, a short coloured clipboard line
    (e.g. *2 con · 1 auto · 0 mod · from engine_loop*) and **Save as
    profile…**, which saves the clipboard. *2 Paste*: **Paste from** picks
    what **Paste** (the large button) pastes, the clipboard or a saved
    profile; **Manage…** renames, replaces or deletes profiles. Copy never
    overwrites a profile, and it asks before overwriting or deleting.
  - **Profiles are saved** per user and per project, in the project's `.user`
    folder, so they're still there after Scripts → Reload or a restart.
  - **The buttons say what they'll do**, e.g. *Copy from engine_loop* or
    *Paste onto 3 selected*.
  - **No more "Use the deck selection" checkbox.** When you pick an entry in a
    Multi instrument's playlist, the tool detects it and says so.
  - **Instrument Settings** (a third Include option, off by default): delay,
    quantization, probability, volume, pitch, voices and stealing, and the
    scatterer and multi settings. Only values that differ are changed.
  - **FMOD 2.02 projects:** 2.03-only modulators (Seek, Spectral Sidechain)
    are skipped with a clear reason instead of a vague error, and a 2.03-only
    clock source setting is noted as not applied.
  - **Add missing parameter sheets** is now **off by default**. When ticked, it
    now adds the sheets reliably. In 1.2.0 it could skip them on an event that
    already referred to the parameter, for example after an earlier paste
    without sheets.

## What's new in 1.1.0

- **Copy-Paste Conditions & Automation 1.2.0** (was *Conditions & Automation
  Copy-Paste* 1.1.0). It has a new name, so it's now listed under *Scripts →
  DAT-audio Utilities* as **Copy-Paste Conditions & Automation**. Its file is
  now `Copy-Paste Conditions & Automation.js`. After updating, you can delete the
  old `Conditions & Automation Copy-Paste.js`. The loader no longer reads it,
  so it's harmless if left.
- New **Add missing parameter sheets** option in both its Trigger Conditions
  and its Automation & Modulation sections. It's ticked by default, which keeps
  the old behaviour. Untick it to paste everything without adding parameter
  sheets to the target event. See the tool's description below.
- **Event Recorder 0.6.3** checks that its one-time setup has been run before
  it opens. If the Python environment is missing or incomplete, it names the
  folder to run `setup.command` / `setup.bat` in. Before, a render failed with a
  generic "renderer did not complete" message.
