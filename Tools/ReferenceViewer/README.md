# Reference Viewer

**Version 0.5.4** (Studio script 0.5.4), part of DAT-audio Utilities. Targets FMOD
Studio **2.03** first, then 2.04 and 2.02, on macOS (Apple silicon) and
Windows (64-bit). By **Tobey Evans**.

A live window beside FMOD Studio that follows what you select and shows
everything connected to it, without right-click → Find References each time.
Everything is drawn as signal flow: sources on the left, where the sound
goes on the right.

- **An event:** the events it uses on the left (chained down), and the events
  that use it on the right, chained up to the top-level events. Each link
  shows what happens to the sound on the way: trigger conditions, automation,
  modulation, track and effect settings, and parameter values passed in.
- **An asset** (Assets browser): every event that plays it, and the events
  that use those.
- **A mixer bus, return or VCA** (Mixer window): what feeds it (buses, events,
  sends) and where it goes (its output chain to the Master Bus, its sends),
  with its fader, automation, effects, VCAs and snapshots.
- **A send** (selected in an effect chain): its return's picture, with this
  send highlighted.
- **A shared effect** (Effects browser, or selected in an effect chain):
  every track, event and bus it is on.
- **A parameter:** every instrument, track, effect, macro and event that uses
  it, as a trigger condition, automation, modulation, or a value passed into a
  referenced event.
- **A snapshot:** which properties it changes and by how much, and what drives
  its intensity.
- **Signal path / effective gain** (for an event): an estimate of how loud the
  event comes out at Studio's current parameter values, step by step from the
  instrument to the Master Bus. A *Default* column shows the same chain at
  every parameter's initial value.

**It never changes your project.** The one action it takes is to open things
in Studio when you double-click them.

| Part | Version | File |
| --- | --- | --- |
| Open Reference Viewer… (Studio script) | 0.5.4 | `Scripts/DAT-audio Utilities/Open Reference Viewer.js` |
| Reference Viewer app, macOS | 0.5.4 | `Tools/ReferenceViewer/reference_viewer` |
| Reference Viewer app, Windows | 0.5.4 | `Tools/ReferenceViewer/reference_viewer.exe` |

## Launching it

Launch it from FMOD Studio:
**Scripts → DAT-audio Utilities → Open Reference Viewer…**. The script starts
the app from `Tools/ReferenceViewer/`, next to the `Scripts` folder it was
loaded from, and opens a small **Reference Viewer link** window in Studio that
carries the connection. Keep the link window open while you work. Closing it
closes the viewer.

- **Opening the app directly** shows "Waiting for Studio…" and nothing else.
- **No menu item?** The pack isn't installed or loaded (see the pack's
  `README.md`), then **Scripts → Reload**.
- **First launch:** the apps aren't signed, so macOS or Windows may ask you to
  allow the app once.

## How to use it

The **Guide** button (top right) shows this section inside the viewer.

### Selecting

- Select something in Studio and the viewer follows it:
  - **browser:** events, parameters, snapshots, assets, shared effects; and in
    the Mixer window, buses, returns, VCAs and events;
  - **event editor:** an event instrument shows the event it plays;
  - **effect deck:** a send shows its return; a shared effect (or the effect
    inside it) shows its uses.
- Whichever selection you changed last wins. Clicking something the viewer
  can't show (a track, an audio instrument) leaves the view as it is, and
  **Scripts → Show Script Output** names it ("Not shown (deck selection):
  …"), which helps when a click seems to do nothing.
- **Several items** of the same kind are shown together, each outlined as
  selected. The Gain tab gives each selected event its own section. Other
  kinds selected at the same time are counted in the top bar. Up to 25 are
  shown.

### References and Gain

The toolbar has two separate tabs:
- **References** shows how things connect. Its own options sit beside it:
  - **Direction:** *Parent of* / *Inputs* (what feeds the selection), *Both*,
    or *Referenced in* / *Outputs* (where it goes).
  - **Layout:** *Graph* or *List*, and **Further chain**: past the
    top-level events, their route through the mixer to the Master Bus and the
    snapshots they start (dimmer boxes under a "FURTHER CHAIN" heading).
- **Gain** (events only) estimates the output level on each route. Its
  only option is **Sort**: loudest first or longest first.

It opens on References, Both, Graph each time.

### Graph

- Sound flows **left to right**: sources on the left, the selection in the
  middle, where it goes on the right. Column headings ("REFERENCED IN · 1",
  "SELECTED") say how far each column is from the selection.
- Each box shows:
  - its kind (EVENT, SNAPSHOT, BUS, RETURN, VCA, ASSET, SHARED EFFECT, EVENT
    TRACK) and name;
  - its folder;
  - how it links toward the selection (track, instrument, Multi/Scatterer,
    send level);
  - facts: how many use it, how many it uses, and the banks it is assigned
    to directly ("no bank" when it's only played through a parent).
- **Drag** the background to pan; **scroll** to pan; **pinch**, or hold
  **Cmd** (Ctrl on Windows) and scroll, to zoom. Panning stops a little past
  the edge of the graph. **Fit** (bottom left) brings the selection back. The
  view re-centres when the selection changes.
- **Box border = the item's colour in Studio** (events, buses, returns,
  VCAs, assets, shared effects), and nothing else. No colour set in Studio:
  a plain grey border. The RGB values approximate Studio's palette, including
  its "Light" variants. In the list and the inspector the colour is a dot.
- **Selection = an accent bar** down the box's left edge: **orange** for
  what's selected in Studio (also tagged "· SELECTED"), **white** for the box
  you clicked to inspect.
- **Bank status = a pill** at each event's top right: **"bank X"** in green,
  or **"no bank"** in amber. Amber isn't an error: building-block events used
  only inside other events don't need a bank of their own. The same pill
  shows in the list, the Selected card and the inspector.
- The **further chain** is grouped in one shaded panel under a single
  "FURTHER CHAIN" heading.
- **Click** a box, or the dot on a link, to see its details in the inspector.
  Its paths light up (what feeds it and where it goes) and everything else
  dims, which untangles busy multi-selections. Click the background to clear.
  **Double-click** a box to open it in Studio. **Right-click** a box or a
  link to open it, show its details, or copy its name or path.
- A column with more than 10 boxes shows **+N more**; click it to show them
  all, and **Show fewer** to fold it again. Boxes on the path of the box
  you've clicked are always shown, even in a folded column, so a lit path is
  always drawn end to end. Links to the hidden boxes are drawn as one thicker
  line from the "+N more" box to each box they connect to, labelled "×N".
  Hover it for their names; click it to unfold the column.
- Link colours: grey event instruments, teal assets, purple mixer routing,
  green sends, red shared effects, yellow VCAs, pink snapshots started.

### List

- Three columns: what feeds the selection, the **selection** itself, and
  where it goes. Only the first layer starts open; each chain folds open and
  closed with its arrow.
- With several items selected, each side column has a section per selected
  item ("FOR …"). Several instruments playing the same event show as one row
  with a "×N instruments" count.
- Each row shows how it links (*in*, *uses*, *sends to*…), its kind, name and
  folder, and pills for the track and instrument, conditions, automation,
  passed parameters, top-level, and bank.
- Click a row for the link's details; double-click to open that item in
  Studio; right-click for more.

### Inspector

- It shows the selected box or link:
  - instrument settings;
  - trigger conditions (green: met now, red: not met);
  - automation with its current and default values;
  - modulation, tracks, effects, sends, VCAs and snapshots;
  - the route through the mixer.
- **Cards / Plain** at its top switches the look. Plain is the original
  layout.
- The open button says where Studio will show it:
  - **Open in Event Editor** for events and event tracks;
  - **Show in Mixer** for buses, returns and VCAs (and snapshots);
  - **Show in Assets** for assets;
  - **Show in Effects** for shared effects.

  Studio picks its last-used window of that kind, or opens one; it can't
  be told which window to use.

### What a link shows

For each place an event is used (an event instrument in a parent event):

- **The instrument:** trigger conditions and their mode, volume, pitch,
  probability, delay, its automation and modulation, and the parameter values
  it passes into the child event.
- **Where it sits:** the track (name, volume and pitch automation, effects with
  their automation, modulation and bypass), the parameter sheet or the
  timeline, and any Multi Instrument or Scatterer it is inside (playlist
  entry, probability).
- **The parent event:** its macros and their automation and modulation, and
  its master track volume and effects.

Snapshot instruments are listed separately from event instruments.

## The gain estimate: what it is and isn't

The Gain tab shows each route as a strip of **stages**, left to right, from the
source to the output:
1. the instrument and its track;
2. for each parent event the sound passes through: the instrument, its
   track, the parent's master track;
3. the event's input in the mixer;
4. each bus on the way, its VCAs and Gain effects, then the Master Bus.

Each stage box lists what it changes and the **running level after it**, with
every parameter at its initial value. A "now" line appears where Studio's
current values give a different level. Snapshots and things not counted show
as pills on the stage they belong to. The step table under each card lists
every change.

**Now** uses the values the parameters are set to in Studio right now, so it
updates as you move them. **Default** uses each parameter's initial value.
When an event is used by several parents, each route is listed, loudest
first. You can also sort the routes by length.

What goes into the number, and how sure it is:

- **Exact:** faders, instrument volume, Gain effects, send levels, VCAs,
  macros, and their automation curves at the current parameter values (see
  *How automation curves are evaluated*).
- **A range, not one number:** modulators (LFO, random, envelope).
- **Listed but not counted:** effects whose level depends on the sound itself
  (compressors, limiters, EQ, filters, multiband, third-party plugins). Their
  settings are shown so you can judge them.
- **Approximate:** distance attenuation, from the spatializer's min/max and
  rolloff at the built-in Distance parameter's value.
- **Snapshots** are shown as "would apply −x dB at intensity y" and are never
  added to the total. FMOD Studio's scripting can't tell whether a snapshot
  is playing.

It is **a model, not a meter**: Studio's scripting can't read live meter levels.

## How automation curves are evaluated

Studio's scripting has no way to ask for a curve's value at a position, so the
viewer computes it from the curve's points. The formulas were **measured
against the FMOD 2.03.14 runtime** (23 test curves played through the engine
and metered). They reproduce its output to within 0.18 dB, and about 0.1 dB
on most. Things worth knowing, for this tool and when drawing curves in Studio:

- **Volume curves don't interpolate in dB.** Between two points the engine
  moves along a *fader scale*. That scale is plain dB from +10 down to −20 dB,
  and compressed below −20 dB. So halfway along a −40 → 0 dB curve is
  **−16 dB, not −20 dB**. A −20 → 0 or −10 → +10 curve is straight in dB.
  On this scale, −40 dB sits at about −32, −60 dB at about −39, and −80 dB is
  silence.
- **A curve's bend is stored on the segment's first point** (`curveShape`,
  −1 to 1). A positive bend starts slow, a negative one starts fast, and a
  negative bend is the mirror image of the positive one.
- **Ordinary bends are exponential curves,** not power curves:
  `u = (e^(6.96·b·t) − 1) / (e^(6.96·b) − 1)`.
- **S-curves:** a bend of 0 is a straight line. Positive bends ease both ends
  (exponent `1 + 2·b`). Negative bends steepen both ends with a flat middle (a
  blend of a straight line and an inverse S, weighted by the bend).
- The bend applies on the fader scale, then the result converts back to dB.

**Limit:** only the master-track volume was measured. The tool applies the
same rules to instrument volume, Gain, send levels and other dB properties,
and to pitch and other properties in their own units, and marks those as
estimated. Only the 2.03 runtime was measured. (The measurement data is in
`Tests/calibration/data/`.)

## How it works

- The **Studio script** watches Studio's selection every 40 ms. When the
  selection changes it rebuilds the picture, and every second it re-reads the
  picture to catch edits, sending only what changed. It reads the project
  through Studio's object model, using the links FMOD keeps between objects
  (an event's references, a parameter's conditions and automation curves, a
  snapshot's properties). So **nothing scans the whole project**, and large
  projects stay responsive. Chains stop at 8 hops, at a cycle, or at a node
  limit.
- The **app** draws the panes, graph and side panel. It only receives
  descriptions from the script and asks it to open events. It never touches
  the project files.
- The two talk over the app's standard input and output, one JSON message per
  line.

## Notes

- **Unsigned apps:** macOS or Windows may ask you to allow the app once.
- **Libraries:** it needs no FMOD libraries; it talks to Studio through the script.
- **Read-only:** it never changes the project. Double-clicking only asks
  Studio to show the item.

## Known limits

- Snapshot instruments and event instruments are listed separately. A snapshot
  has no output of its own, so it doesn't appear in signal paths.
- Events in banks that aren't loaded, and lazily loaded events, are untested.
- The live **Now** values depend on Studio updating a parameter's current
  value as you drag its cursor. To be confirmed in the first GUI test.
- Some Studio settings are numeric codes whose names aren't documented. These
  are shown as codes: trigger condition mode, event-state conditions, snapshot
  behaviour and quantization. For "won't play now", the gain estimate takes
  condition mode 0 as "all conditions" and 1 as "any".
- When one property is automated by several parameters, the gain estimate
  adds the curves together. This is an assumption.
- Timeline automation is read at the timeline's start.

## Build from source

Needs the Rust toolchain (edition 2024: Rust 1.85 or newer; tested with 1.99) from [rustup.rs](https://rustup.rs). Build on the machine you run it on: macOS (Apple silicon) or Windows (64-bit).

```sh
cd Tools/ReferenceViewer
cargo build --release   # the app is target/release/reference_viewer
cargo test              # 27 tests
```

The built app goes where the DAT-audio Utilities pack expects it (see the pack's README); the Studio script starts it from there. `target/` is build output and is git-ignored.
