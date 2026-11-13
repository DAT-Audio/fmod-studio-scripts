# Profiler First Person

**Version 0.6.0** (Studio script 0.5.0), part of DAT-audio Utilities. For FMOD
Studio 2.02, 2.03 and 2.04 on macOS (Apple silicon) and Windows (64-bit).
By **Tobey Evans**.

A floating first-person 3D view of a **profiler session**: a 3D version of
the Profiler's **3D View** panel, seen from the game's listener at the
profiler's playhead. It shows:
- every placed event instance, with its min/max distance and sound size
  spheres (from the event's spatializers in the open project);
- the game's listener(s);
- the **listener probe** (the attenuation position) when the game sets one
  away from the listener. Distances and "in range" are then measured from the
  probe, as FMOD does.

| Part | Version | File |
| --- | --- | --- |
| Open Profiler First Person View… (Studio script) | 0.5.0 | `Scripts/DAT-audio Utilities/Open Profiler First Person View.js` |
| First Person app, macOS | 0.6.0 | `Tools/ProfilerFirstPerson/profiler_first_person` |
| First Person app, Windows | 0.6.0 | `Tools/ProfilerFirstPerson/profiler_first_person.exe` |

**View only:** it never changes the project. There are no movement controls;
the view follows the game's listener as the profiler plays, scrubs or records.

## Launching it

Launch it from FMOD Studio, not by opening the app:
**Scripts → DAT-audio Utilities → Open Profiler First Person View…**. The pack's script starts the app
(`Tools/ProfilerFirstPerson/profiler_first_person` on macOS, `profiler_first_person.exe` on Windows) and opens a small
link window in Studio that carries the connection. Keep the link window open
while you work.

- **Opening the app directly** (double-clicking it) shows "Waiting for Studio…": the script tells it which session to show and where the playhead is.
- **No menu item?** The pack isn't installed or loaded. `DAT-audio
  Utilities.js` and the `DAT-audio Utilities` folder go in your FMOD
  `Scripts` folder, and this folder goes in `Tools/ProfilerFirstPerson/` next to it (see
  the pack's top-level `README.md`). Then use **Scripts → Reload**, or
  restart Studio. **Scripts → Show Script Output** shows
  `[DAT-audio Utilities] v… loaded from …` once it's loaded.
- **"Isn't installed" message:** the script looks for the app in
  `Tools/ProfilerFirstPerson/`, next to the `Scripts` folder it was loaded from. Check
  that the app is there, with that exact name.
- **First launch:** the apps aren't signed, so macOS or Windows may ask you
  to allow the app once (see *Notes*).
- **Without Studio:** for a quick look at a session folder, run
  `profiler_first_person --session "<project>/Metadata/ProfilerSession/{id}"`
  from a terminal. It has a time slider, but no event names, colours or ranges.

## Using it

1. Open a profiler session in Studio (**Window → Profiler**) and select it.
2. Choose **Scripts → DAT-audio Utilities → Open Profiler First Person View…**.
3. Keep the small **Profiler First Person link** window open while you use
   it. It closes by itself when you close the view.
4. Play, scrub or click the profiler timeline. The view follows the playhead
   while it moves, and otherwise the cursor.

The session needs recordings made with **API capture** on. Studio saves them
as `Metadata/ProfilerSession/{id}/apicommands-NNNN.txt`, and that's where the
3D data comes from. A session recorded since the project was last saved sits in
`.unsaved/Metadata/ProfilerSession/{id}/` until you save; the view reads both
places, so you don't need to save first.

## The window

The top bar shows the link, plus **Events…** and **Settings** buttons.
Settings open in a side panel (it scrolls, and you can drag its edge to
resize it). The defaults are in brackets.

| Section | What it does |
|---|---|
| **VIEW** | On top, Grid, Labels, Map, **Info card** (all on). FOV. **Scale** is the grid spacing and marker size in game units (**Auto** picks it from the scene; on). |
| **SHOW** | Max / Min spheres (on), Size spheres (off). **Idle** also shows instances that exist but aren't playing, drawn hollow (on). **Children** also draws the ranges of child events (played by an instance's event instruments, including inside multi instruments and scatterers) as dashed spheres at the instance (off). |
| **SPHERES** | **Lines** (wireframe; the default), **Shell** (each sphere's surface at its radius: a faint fill with a strong rim, in the event's colour) or **Both**. A shell is drawn only while its event is playing *and* the listener (or probe) is within its max distance; otherwise it's a light mesh. Min spheres stay as lines. **Opacity** applies to every sphere (90%). |
| **EVENTS** | **Events…** opens a separate window listing every event in the session, in its Event Browser folders, with tick boxes. New events are added as they appear. **Only ticked** draws just the ticked events (off; when on, nothing is drawn until you tick something). A folder's box ticks or unticks everything listed under it. Your ticks are remembered. |
| **CAMERA** | **Listener** (when the game drives several; *Auto* follows the one it updates most). The **offset** moves the view's camera off the listener: right / up / back in the listener's own frame, plus turn and tilt. The game's real camera isn't in the capture, so use this to match it (e.g. back and up for a third-person game). **At listener** resets it. |
| **DETACHED CAMERA** | **Detach** gives you a camera of your own to fly around, so you can see the listener's view from outside: where it sits and what its FOV covers, drawn as an orange frustum (in the view and on the map). It starts behind and above the listener, looking at it. Drag the view to look; W A S D fly, E / Q (or Space / C) go up and down, arrows turn; Shift is faster, Option slower, and scrolling over the view changes speed. **Speed** and **Cast** (how far out the view is drawn) are in game units; **Lens** is this camera's own FOV (VIEW → FOV is the listener's, so changing it changes the orange frustum). **Behind the listener** puts the camera back behind it; **Reset to listener** couples the view back to the listener. The camera stays where you put it while the playhead moves. |

**The info card** appears bottom-right whenever an instance's marker is near
the crosshair (the small cross in the middle of the view). It shows the event
and its path, the distance from the probe or listener and whether that's in
range, whether it's playing, the instance handle and GUID, its child events
and its spatializers. Turn it off with VIEW → Info card.

With an offset, the listener is drawn as a yellow ring with a forward arrow,
and the probe as a cyan diamond joined to it.

## How it works

- **It reads what the game recorded.** With API capture on, Studio saves every
  runtime call the game made to a text file per recording
  (`apicommands-NNNN.txt` in the session's folder). The app reads the
  listener and event positions from it (`setListenerAttributes`,
  `set3DAttributes`) and the time from the playback frames. The script tells
  the app where each recording sits on the timeline and where the playhead
  is, and looks up each event's name, colour and spatializer ranges in the
  open project.
- **Height is real.** The game's positions are full 3D (X, Y and Z), so
  sounds above or below the listener are drawn where they really were.
  Sandbox First Person is different: the Sandbox only stores X/Z and a facing,
  so its optional *Elevation (beta)* has to fake height by moving emitters
  along the ground. Nothing like that is needed or done here.
- **No undo steps.** It never changes the project: no listener, emitter or
  event is moved or edited, so there's nothing to undo. Sandbox First Person
  moves the Sandbox listener (and, with Elevation on, emitters), which is
  why it folds a session into one undo step. This tool doesn't need that.
- **Nothing is sent to the game.** It works from the session's files only,
  so it doesn't matter whether the game is still connected.
- **Recordings in progress:** while a recording's file is still growing, the
  status line says *(live)*, and a playhead past what's been captured shows the
  latest moment.

## What we had to build ourselves

Studio's Profiler has its own flat 3D View panel, but none of its 3D data is in the
scripting model. These parts are our own:

- **Reading the capture files.** The positions only exist in the API capture
  Studio saves next to each session. The app parses those text files itself,
  including while a recording grows.
- **Lining up time.** The capture has no timestamps, only "advance N
  samples" frames. The app adds them up at the capture's sample rate, and the
  script sends where each recording starts on the session timeline (its
  recording marker) and where the playhead and cursor are.
- **Naming events.** The capture only has handles and GUIDs. The app follows
  each handle back to its event's GUID (handles are reused after release),
  and the script looks the event up in the open project for its name, path
  and spatializer ranges.
- **Colours are approximated.** Studio stores an event's colour as a name
  (e.g. "Blue Light 2"), not a value, so the app maps each name to an RGB
  close to Studio's.
- **Child events are found by walking the event.** The capture only has the
  events the game created, so the script follows event instruments, multi
  instruments and scatterers to list the events they play.
- **Guesses the capture forces:** which listener to follow when the game
  drives several (the one it updates most), the 2 s shown for a
  fire-and-forget one-shot (the capture doesn't say when it ends), and the
  game's handedness (from the capture header).

## Notes

- **Selecting tracks in the Profiler** (one or several event tracks) makes
  the view draw only those events, whatever is ticked in Events…. Clear the
  selection to go back.
- **Colours** follow each event's colour in the Event Browser (the RGB is
  approximate). Events with no colour, or not in the project, are white.
- **Child events** aren't in the capture: the game only creates the top
  event. They're drawn at the parent instance whenever it's shown, so they
  may not actually be playing. Scatterer children really spawn *around* the
  parent.
- **One-shots:** an instance released while playing (fire and forget) is
  shown for 2 s after it starts; the capture doesn't say when it ends.
- **Event names and ranges** come from the open project, by GUID. Events the
  project doesn't have show as "not in this project".
- **Right-handed games** (FMOD_INIT_3D_RIGHTHANDED) are detected from the
  capture header.
- **Unsigned apps:** macOS or Windows may ask you to allow the app once.
- **Libraries:** it needs no FMOD libraries; it talks to Studio through the script.

## What's new

- **0.6.0:** DETACHED CAMERA: fly a camera of your own to see the listener's
  view (its position and FOV, drawn as an orange frustum) from outside, and
  **Reset to listener** to couple back. The Studio script is unchanged (0.5.0).
- **0.5.0:** works with sessions that haven't been saved yet (it read only the
  saved folder before, so a new session showed "No recordings with API
  capture"). Follows a recording still being written, marked *(live)*.

## Build from source

Needs the Rust toolchain (edition 2024: Rust 1.85 or newer; tested with 1.99) from [rustup.rs](https://rustup.rs). Build on the machine you run it on: macOS (Apple silicon) or Windows (64-bit).

```sh
cd Tools/ProfilerFirstPerson
cargo build --release   # the app is target/release/profiler_first_person
cargo test              # 35 tests
```

The built app goes where the DAT-audio Utilities pack expects it (see the pack's README); the Studio script starts it from there. `target/` is build output and is git-ignored.
