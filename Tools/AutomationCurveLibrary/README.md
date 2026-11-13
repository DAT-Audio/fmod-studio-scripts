# Automation Curve Library

**Version 0.9.7** (Studio script 0.8.5), part of DAT-audio Utilities. For FMOD
Studio 2.02, 2.03 and 2.04 on macOS (Apple silicon) and Windows (64-bit).
By **Tobey Evans**.

A library of automation curves you can draw and edit with the mouse or
trackpad, then apply to whatever is selected in Studio. It brings FMOD 2.04's
*Automation Curve Presets* and *Precision Automation Editing* to 2.02 and
2.03, and reads and writes 2.04's own presets.

| Part | Version | File |
| --- | --- | --- |
| Open Automation Curve Library… (Studio script) | 0.8.5 | `Scripts/DAT-audio Utilities/Open Automation Curve Library.js` |
| Curve Library app, macOS | 0.9.7 | `Tools/AutomationCurveLibrary/curve_library` |
| Curve Library app, Windows | 0.9.7 | `Tools/AutomationCurveLibrary/curve_library.exe` |
| Factory curves (read-only) | 99 curves | `Tools/AutomationCurveLibrary/Libraries/Factory/` |
| User guide (also in the app: **Guide**, or F1) | | `Tools/AutomationCurveLibrary/GUIDE.md` |

**It edits the project.** When you press Apply, it writes automation onto the
thing you selected in Studio. Your curve libraries are separate from your
projects: they live with the tool, on your machine.

## Launching it

Launch it from FMOD Studio, not by opening the app:
**Scripts → DAT-audio Utilities → Open Automation Curve Library…**. The pack's script starts the app
(`Tools/AutomationCurveLibrary/curve_library` on macOS, `curve_library.exe` on Windows) and opens a small
link window in Studio that carries the connection. Keep the link window open
while you work.

- **Opening the app directly** (double-clicking it) opens it unlinked: your libraries work, but it can't see Studio's selection or apply anything. It says "Not linked to Studio".
- **No menu item?** The pack isn't installed or loaded. `DAT-audio
  Utilities.js` and the `DAT-audio Utilities` folder go in your FMOD
  `Scripts` folder, and this folder goes in `Tools/AutomationCurveLibrary/` next to it (see
  the pack's top-level `README.md`). Then use **Scripts → Reload**, or
  restart Studio. **Scripts → Show Script Output** shows
  `[DAT-audio Utilities] v… loaded from …` once it's loaded.
- **"Isn't installed" message:** the script looks for the app in
  `Tools/AutomationCurveLibrary/`, next to the `Scripts` folder it was loaded from. Check
  that the app is there, with that exact name.
- **First launch:** the apps aren't signed, so macOS or Windows may ask you
  to allow the app once (see *Notes* or *Install notes*).

## Using it

1. In FMOD Studio, choose **Scripts → DAT-audio Utilities → Open Automation
   Curve Library…**. Keep the small **Curve Library link** window open while
   you work: it carries the connection between Studio and the app.
2. Pick a curve on the left (copy a Factory curve to edit it), or make one
   with **New curve**.
3. Select the thing (or things) to automate in Studio, and check *Selected in
   Studio* on the right.
4. In *Apply curve to*, choose the property and parameter, check the ranges,
   and press **Apply**.

## What it does

- **Libraries.** *Factory* holds 99 base curves (Basics, Ease, Easing, Fades,
  Crossfades, Distance, Panning, Game states, Vehicles, Shapes, For dB and Hz
  and more) and is read-only: copy a curve into one of your libraries to
  change it. Right-click anything for New curve, New folder, Rename,
  Duplicate, Move to, Copy to and Delete. Star curves to add them to
  **Favourites**.
- **One folder per library, one file per curve.** To share a library, copy its
  folder (for example, into a repo). To use someone else's, drop their folder
  into `Libraries/`; it appears within a couple of seconds.
- **Range-free curves.** Curves are stored 0 to 1 on both axes, like FMOD
  2.04's presets, so the same shape fits distance 0-3 or camera height 0-100.
- **Drawing.** Drag points and bend handles, double-click to add or remove
  points, drag a box to select several, right-click for segment shapes. The
  strip under the graph has exact values, and Reverse, Flip and Mirror. Cmd
  shortcuts are Ctrl on Windows.
- **Applying.** Effects (inserts and shared), instruments, track and bus
  volume, an event's master track, or a selected automation curve. Several
  selected things can take the same curve at once. An existing curve on that
  property and parameter is replaced in place; **Undo last apply** reverses it.
- **Curves already in Studio** are drawn under the thing they're on. **Pull
  into graph** edits one in place; **Save to library…** keeps a copy.
- **FMOD 2.04 presets:** copy a library to the project's curve presets, or
  bring them into a library.
- **Spatializers:** their distance attenuation is built in and can't be
  replaced by a curve, so the app shows it (mode, range and a drawing) with
  the way round it: attenuation Off plus a Gain automated on Distance.

## How it works

- **Two parts, one link.** The Studio script starts the app and talks to it
  through the app's standard input and output, one line of JSON at a time.
  The small *Curve Library link* window owns the timer that carries that
  link. The app never touches the project or FMOD's libraries itself.
- **What's selected:** the script reads Studio's selection (the deck, the
  editor and the browser) and sends the app the things a curve can go on,
  the parameters each one allows and any curves already on them.
- **Applying** uses Studio's scripting API. The script adds an automator for
  the property (`addAutomator`) and a curve on the parameter
  (`addAutomationCurve`), then writes the points with their exact values and
  bends (`addAutomationPoint`). If the event doesn't have that parameter yet,
  it adds it (`addGameParameter`). A curve already on that property and
  parameter is reused, and its points are replaced in place.
- **Undo:** the app's **Undo last apply** puts back the curve that was there
  before (or removes the curve if the apply created it). Studio records an
  apply as several small edits, so Studio's own Undo may need a few steps.
- **Curves are files.** Each curve is one `.curve.json` file in its library's
  folder, stored 0 to 1 on both axes and scaled to the property and
  parameter ranges only when applied. The app notices library folders being
  added or changed within a couple of seconds.

## What we had to build ourselves

FMOD Studio's scripting API can read and write automation, but much of what
this tool shows isn't something Studio hands over. These parts are our own:

- **The editor is a separate app.** Studio's script dialogs have no drawing
  canvas and no mouse events, so the curve editor can't live inside Studio.
  The script only carries data between Studio and the app.
- **We draw Studio's curves ourselves.** Studio can't give a script a picture
  of a curve, only its points (position, value and bend or S shape). The app
  redraws curves already in Studio from those numbers: the faint *Studio's
  curve* ghost in the graph and the small previews under each selected thing.
  Our drawing of bends and S shapes is an approximation of Studio's; the
  values sent to Studio are exact.
- **Real-unit ranges are built in.** The API doesn't report a property's
  range or unit, so the script carries a table taken from FMOD's Effect and
  Instrument Reference. That's what turns a 0 to 1 curve into dB, Hz or
  percent when you apply it.
- **Spatializer attenuation is calculated, not read.** Its curve can't be
  automated or read as points, so the app works it out from the rolloff type
  and min/max distance with FMOD's documented formulas, and draws that.
- **Presets for 2.02 and 2.03.** Curve presets only exist in 2.04, so the
  libraries are plain files kept with the tool, and the app reads and writes
  2.04's presets when they're there.
- **Undo last apply.** A script can't group its edits into one Studio undo
  step, so the app remembers the curve that was there and puts it back
  itself.
- **Volume looks different in Studio.** Studio draws volume and gain on a
  fader scale and the app draws them straight, so a dB curve looks lower in
  Studio. The values are the same.

## Install notes

Install with the rest of the pack (see the `README.md` at the top of the
scripts-and-tools folder). FMOD 2.04 on macOS uses the same folder as 2.03,
`~/Library/Preferences/FMOD Studio/`; on Windows every version uses
`%LOCALAPPDATA%\FMOD Studio\` (by default
`C:\Users\<YourUsername>\AppData\Local\FMOD Studio\`).

**Both apps ship in the same folder.** The script starts `curve_library` on
macOS and `curve_library.exe` on Windows, depending on where Studio is
running, and both use the same `Libraries/` folder. Neither needs any other
files: no DLLs, and nothing per FMOD version (the app talks to Studio
through the script, not through FMOD's libraries).

- **If macOS blocks the app** ("can't be opened" or "unidentified developer";
  it isn't signed yet): open `curve_library` once from Finder with
  right-click → Open, or allow it under System Settings → Privacy & Security.
  In Terminal: `xattr -d com.apple.quarantine "<path to>/curve_library"`.
- **If Windows blocks the app** ("Windows protected your PC"; it isn't
  signed yet): choose **More info → Run anyway** the first time. If Studio
  reports it couldn't start the app, double-click `curve_library.exe` once in
  File Explorer and allow it there.
- **Check it's installed:** after you open it, **Scripts → Show Script
  Output** shows `[Automation Curve Library] v0.8.5 (macOS): started …`, or
  `(Windows)`.
- **If you had the standalone version** (`Scripts/fmod_automation_curve_library.js`
  at the top of a Scripts folder), delete that file, or the menu item appears
  twice. Its `Tools/AutomationCurveLibrary/` folder is the same one this pack
  uses, so your libraries carry over.

## Where things are kept

| What | Where | Shared? |
| --- | --- | --- |
| Factory and your libraries | `<FMOD per-user folder>/Tools/AutomationCurveLibrary/Libraries/` | Copy a library's folder to share it |
| Favourites, last selected curve, window layout | macOS: `~/Library/Application Support/DAT-Curve-Library/`; Windows: `%APPDATA%\DAT Curve Library\data\` | No: personal, per machine |
| Automation you apply | Your FMOD project | Yes, with the project |

**Updating:** copy the new files over the old ones. Your own libraries are
separate folders next to *Factory*, so they're untouched.

## Known limits

- macOS on Apple silicon and 64-bit Windows; no Intel Mac build yet. (FMOD
  2.02 on a Mac runs under Rosetta and starts the app normally.)
- **The Windows app is new in 0.9.7** and built on a Mac: tested there, but
  not yet run on Windows.
- In 2.02 the link and apply were checked against its documented scripting
  API, not yet in a 2.02 session.
- Studio draws volume and gain on a fader scale, so a dB curve looks lower in
  Studio than in the app. The values are exact.
- The app draws bends approximately; Studio receives the exact bend values.
- An apply is made of several small edits, so Undo in Studio may need a few
  steps to reverse one.

## Build from source

Needs the Rust toolchain (edition 2024: Rust 1.85 or newer; tested with 1.99) from [rustup.rs](https://rustup.rs). Build on the machine you run it on: macOS (Apple silicon) or Windows (64-bit).

```sh
cd Tools/AutomationCurveLibrary
cargo build --release   # the app is target/release/curve_library.exe on Windows
cargo test              # 53 tests
```

The built app goes where the DAT-audio Utilities pack expects it (see the pack's README); the Studio script starts it from there. `target/` is build output and is git-ignored.
