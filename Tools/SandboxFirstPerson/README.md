# Sandbox First Person

**Version 0.3.0** (Studio script 0.3.0), part of DAT-audio Utilities. For FMOD
Studio 2.02, 2.03 and 2.04 on macOS (Apple silicon) and Windows (64-bit).
By **Tobey Evans**.

A floating first-person 3D view of a **Sandbox** scene, seen from its
listener and **kept in sync with it, live**:
- Flying in the view moves the real Sandbox listener.
- Dragging the listener or emitters in the Sandbox moves the view.

Every emitter is shown with its spatial size: min/max distance spheres, sound
size, and extra spatializers on other tracks.

| Part | Version | File |
| --- | --- | --- |
| Open Sandbox First Person View… (Studio script) | 0.3.0 | `Scripts/DAT-audio Utilities/Open Sandbox First Person View.js` |
| First Person app, macOS | 0.3.0 | `Tools/SandboxFirstPerson/sandbox_first_person` |
| First Person app, Windows | 0.3.0 | `Tools/SandboxFirstPerson/sandbox_first_person.exe` |

**It moves the Sandbox listener**, and with Elevation on (off by default) it
temporarily moves emitters too. It never touches events. The whole first-person session leaves a single undo step:
one Undo returns the listener to where it was before you started. If you
edit something else in Studio meanwhile, a new step starts after your edit,
so your own work is never undone.

## Launching it

Launch it from FMOD Studio, not by opening the app:
**Scripts → DAT-audio Utilities → Open Sandbox First Person View…**. The pack's script starts the app
(`Tools/SandboxFirstPerson/sandbox_first_person` on macOS, `sandbox_first_person.exe` on Windows) and opens a small
link window in Studio that carries the connection. Keep the link window open
while you work.

- **Opening the app directly** (double-clicking it) shows "Waiting for Studio…" and nothing else: everything it draws comes from the open Sandbox scene through the script.
- **No menu item?** The pack isn't installed or loaded. `DAT-audio
  Utilities.js` and the `DAT-audio Utilities` folder go in your FMOD
  `Scripts` folder, and this folder goes in `Tools/SandboxFirstPerson/` next to it (see
  the pack's top-level `README.md`). Then use **Scripts → Reload**, or
  restart Studio. **Scripts → Show Script Output** shows
  `[DAT-audio Utilities] v… loaded from …` once it's loaded.
- **"Isn't installed" message:** the script looks for the app in
  `Tools/SandboxFirstPerson/`, next to the `Scripts` folder it was loaded from. Check
  that the app is there, with that exact name.
- **First launch:** the apps aren't signed, so macOS or Windows may ask you
  to allow the app once (see *Notes*).

## Using it

1. Open a Sandbox scene and click into it (selecting anything in it also works).
2. Choose **Scripts → DAT-audio Utilities → Open Sandbox First Person View…**.
3. Keep the small **First Person link** window open while you work. It
   carries the connection between Studio and the view.

The First Person window floats above Studio (untick **On top** to stop that).
Add and edit emitters in the normal Sandbox; this view is for navigating.

## Controls

Click the view first. **Controls** (top bar), or **H**, shows the full list.

| Input | Does |
| --- | --- |
| Drag (any mouse button) | look around |
| W / S or Up / Down | fly forward / back (where you look) |
| A / D | strafe |
| Left / Right | turn |
| E or Space / Q or C | up / down (Elevation on) |
| Shift / Option (Alt) | fast / slow |
| Scroll over the view, or - / = | flying speed: fine, halve / double |
| R | back to ear height, level |
| [ / ] | minimap zoom |

- **Speed** (top bar) is the flying speed, in Sandbox units per second (0.01
  to 100). Set it to suit your project's units.
- **Scale** resizes the grid, markers and floor for projects whose units
  aren't metres.
- **Toolbar:** grouped into VIEW, SHOW, MOVE, SPIN and CIRCLE; active buttons are orange.
- **Auto movements:**
  - **Spin** (CW/CCW, slow/fast).
  - **Circle** (CW/CCW, with radius and speed). The centre is either *beside
    me* or *round the emitter under the crosshair*. While walking you can
    face the way you're going, face the centre, or keep your facing.
  - Any manual move stops both.

## Elevation (beta)

**The Sandbox has no height.** Its listener and emitters have only X/Z and a
facing, so FMOD can't hear elevation directly. **Elevation is off by
default**, and while it's off you stay at ear height: E/Q do nothing, and
flying is level.

Turn it on with **Elevation (beta)** (MOVE section). An explanation comes up
first. While you're above or below ear height, the tool **fakes elevated
distance by moving emitters away from the listener along the ground, in the
Sandbox view**:
- **Left/right offset:** each emitter keeps its true offset to your left or right.
- **Distance:** it's placed at your true 3D distance from it.

The listener stays under you. This is exact for every emitter at once, rows
of street emitters included. A sound below you sits near the centre; it
flips from in front to behind only once you've clearly passed over it.
- **You'll see emitters move in the Sandbox window while you're up.** They go
  back to their real positions when you return to ear height (R), turn
  Elevation off, switch scene or close the view. The First Person view always
  draws them where they really are.
- **Moved:** emitters that react to distance, meaning a spatializer with a
  distance **curve**, or anything automated on built-in **Distance** /
  **Distance (Normalized)** (e.g. a snapshot instrument's intensity).
- **Never moved:** 2D events, spatializers whose curve is **Off**, and
  snapshots placed in the Sandbox.
- **Custom height game parameters** (e.g. `CameraAltitude`, `GlobalHeight`)
  are **not** driven by the view's elevation. Set them by hand if you need them.
- **Elevation** and the other built-in parameters only see the flat Sandbox.
- **Don't save the project while you're elevated:** the moved positions would
  be saved. Undo puts them back.
- **Dragging while elevated:** dragging an emitter in the Sandbox while
  you're up makes that spot its real position. Dragging the listener (or
  pressing Undo) brings you back to ear height.
- **Undo:** the listener and emitter moves of a session are one undo step.

## How it works

- **Two parts, one link.** The Studio script starts the app and talks to it
  through the app's standard input and output, one line of JSON at a time.
  The small *First Person link* window owns a timer (about 47 times a second)
  that carries that link.
- **Following the Sandbox:** the Sandbox scene, its listener and emitters are
  in Studio's scripting model (X/Z position and a facing). The script reads
  them every tick and sends the app only what changed, so dragging in the
  Sandbox moves the view live. Event names and spatializer ranges come from
  the emitters' events.
- **Moving the listener:** when you fly in the view, the app sends the new
  position and the script writes it to the Sandbox listener, so FMOD hears
  from there.
- **Undo:** Studio records every script write as its own undo step, so a
  short flight could leave hundreds. When a movement stops (half a second
  with no moves), the script undoes its own steps with Studio's Undo action,
  one per write, back to where the movement started. Then it writes the end
  position once. The whole session is left as one undo step. It only folds
  when the history grew by exactly its own writes, so an edit you make
  meanwhile is never undone.
- **Elevation:** the Sandbox stores no height, so FMOD can't hear it. With
  *Elevation (beta)* on, the script moves each distance-reactive emitter
  along the ground until its distance from the listener equals the true 3D
  distance, keeping its left/right offset. It remembers the real positions
  and puts them back when you return to ear height, turn Elevation off,
  switch scene or close the view.

## What we had to build ourselves

Studio's Sandbox has positions and a facing in the scripting model, but no
3D view, no height and no way for a script to see what the Sandbox draws.
These parts are our own:

- **The view is a separate app.** Studio's script dialogs can't draw 3D, so
  the script only carries data between the Sandbox and the app.
- **We draw the spheres ourselves.** The script reads each emitter's event
  and its spatializers: min/max distance (from the spatializer's own range or
  the event's), sound size, extent mode and rolloff. Preset spatializers are
  looked through to the real effect. The app draws its own spheres from those
  numbers, with spatializers on other tracks drawn dashed.
- **Syncing both ways without echoes.** The script remembers what it wrote,
  so its own moves aren't sent back to the app as if you'd dragged in the
  Sandbox. The facing convention (degrees, clockwise from +Z) was measured
  from real drags.
- **One undo step per session.** Studio's undocumented undo-grouping calls
  fail inside a script's timer, so every write would be its own undo step.
  The script undoes its own steps with Studio's Undo action when you stop
  moving and writes the end position once, only when the history holds
  nothing but its own writes.
- **Elevation is faked.** With no height in the Sandbox, *Elevation (beta)*
  moves distance-reactive emitters along the ground to their true 3D
  distance, and puts them back afterwards. Finding which emitters react to
  distance means scanning each event for a distance curve or automation on
  the built-in Distance parameters.
- **Not possible:** a per-emitter volume or a real height. Sandbox emitters
  only have position, facing, loop and parameter values, and their playing
  instances aren't reachable from scripts.

## Notes

- **Unsigned apps:** macOS or Windows may ask you to allow the app once.
- **Libraries:** it needs no FMOD libraries; it talks to Studio through the script.
- **Sound Size** is drawn as a diameter, and only when Extent Mode is User.
- **Preset (shared) spatializers** are looked through.

## Build from source

Needs the Rust toolchain (edition 2024: Rust 1.85 or newer; tested with 1.99) from [rustup.rs](https://rustup.rs). Build on the machine you run it on: macOS (Apple silicon) or Windows (64-bit).

```sh
cd Tools/SandboxFirstPerson
cargo build --release   # the app is target/release/sandbox_first_person
cargo test              # 29 tests
```

The built app goes where the DAT-audio Utilities pack expects it (see the pack's README); the Studio script starts it from there. `target/` is build output and is git-ignored.
