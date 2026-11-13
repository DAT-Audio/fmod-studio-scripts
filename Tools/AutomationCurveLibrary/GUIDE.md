# Curve Library guide

Keep a library of automation curves, shape them with the mouse or trackpad, and apply them to whatever is selected in FMOD Studio 2.02, 2.03 or 2.04.

## Libraries (left)

- **Factory** holds 99 base curves, most with 2 to 4 points so they're easy to edit: basics, easing (Quad to Quint, Sine, Expo, Circ, Back, Elastic, Bounce), Wwise-style Log/Exp (which double as Quad, Cubic and Quart), crossfade laws, Unreal-style rolloffs, game-audio sets (Panning, Distance, Game states, Vehicles) and dB- and Hz-ready versions. It's read-only: use **Copy to … to edit**, or drag a Factory curve into your library, to make an editable copy.
- Each curve has a **small drawing** beside its name; hover for a larger one.
- **Your libraries** hold your curves in folders. Right-click a library, folder or curve for New curve, New folder, Rename, Duplicate, Move to, Copy to, Delete and Show in Finder.
- **Drag curves** to reorder them, or onto another folder or library to move them. Right-click > **Move up** / **Move down** also reorders. **Collapse all** folds every folder.
- **Favourites:** click the star beside a curve (or by its name above the graph). Favourites are listed in the **Favourites** folder at the top, however the other folders are folded, and keep their star where they live. They're kept in the app's settings on this machine only, so they don't change shared libraries.
- **New library…** (top of the list) makes another library. The first time you save into "My Curves", you're asked to give it a proper name.
- **Sharing:** each library is a folder in the tool's `Libraries` folder (right-click a library > **Show in Finder**). Copy that folder to share it, for example into a repo. Drop someone else's library folder in, and it appears within a couple of seconds.

## Editing a curve (centre)

- **Drag a point** to move it. **Drag a diamond** to bend the segment after a point.
- **Double-click** (or double-tap) empty space to add a point, or a point to delete it.
- **Select several points:** drag a box on empty space. Shift adds to the selection, Shift-click toggles a point, and Cmd+A selects all. Drag any selected point to move them all. They squeeze together when they reach an edge.
- **Right-click** a segment for **Segment shape**: Linear, Single curve, S-curve, or compound shapes (Double S, Triple S, Steps, Wave, Overshoot, Bounce). Compound shapes add points, so they play the same in Studio.
- **Whole curve:** **Reverse** plays it backwards, **Flip** turns it upside down, and **Mirror** makes it go there and back (a rise becomes a bell). Every bend is kept.
- **S-curves** have a strength: drag their diamond (at the quarter point). Strength 0 is a straight line in Studio.
- **Exact values:** the strip under the graph shows the selected point's position, value (both 0 to 1), bend (-1 to 1) and S-curve, with Previous / Next, Add point, Delete point and the segment's shape, and the whole-curve edits. **All points…** opens a table of every point.
- **Save** (Cmd+S) writes your edits. Unsaved curves show a dot, and **Revert** discards them. Undo is Cmd+Z.
- **View:** two-finger scroll, middle-drag, Space+drag or Alt+drag pans; pinch or Cmd+scroll zooms; **Reset zoom** (top bar, or right-click the graph) goes back to the whole curve. Opening another curve always shows all of it, and with **Auto** ticked the view eases back 2 seconds after the pointer leaves the graph.
- **Keys (with the pointer over the curve):** arrows nudge the selected points, `[` and `]` step to the previous or next point, Delete removes the selected points, Cmd+A selects all. Shift while dragging is fine control; Cmd snaps to 0.05.

## Applying to Studio (right)

1. In Studio, choose **Scripts > DAT-audio Utilities > Open Automation Curve Library…**. Keep the small **Curve Library link** window open while you work.
2. **Select something in Studio** (the most specific selection counts: an effect in the deck, then instruments or tracks in the editor, then events in the browser): an effect (on a track, bus, master or return), a shared effect, an instrument, a track, an event, or a point on an automation curve. **Selected in Studio** shows each thing and where it is; a shared effect has a **Shared effect** pill (every event and bus that uses it gets the curve).
3. **Several selected:** each has a tick box. Things that share a property with the first one start ticked. The lists below offer only what every ticked one has, so Apply goes to all of them; each event's own timeline stands in for another's.
4. In **Apply curve to**, pick the **Property** and **Parameter** (a selected automation curve picks both), and check the ranges:
   - **Position range:** the part of the parameter the curve spans.
   - **Value range:** the property's values that 0 and 1 map to (pre-filled where known, for example -80 to +10 dB).
5. Select a curve in your library and press **Apply**. The note above it says whether existing curves are replaced; **Undo last apply** puts them back.

- While linked, the graph's axes show the target's real units, and the curve already in Studio is drawn dashed, with its points as rings, behind yours. **Show Studio's curve** (top bar) turns it off and on.
- **Curves already in Studio** are listed, with a small drawing, under the thing they're on in *Selected in Studio* (the Property and Parameter lists show the drawing too). **Pull into graph** opens one in the graph, in Studio's units, to edit; **Apply to Studio** (above the graph) writes it back, and opening a library curve closes it. **Save to library…** keeps a copy.
- **Spatializers:** their distance attenuation (Linear Squared, Linear, Inverse, Inverse Tapered or Off, over a min and max distance) is built in, and FMOD 2.02 to 2.04 can't replace it with a custom curve. Select one and *Selected in Studio* draws its attenuation in dB over distance. For your own curve, set Distance Attenuation to Off and add a Gain after the spatializer, automated on Distance from 0 to the max distance; **Save as a curve…** keeps the built-in one in a library, ready for that Gain. The spatializer's other properties (Min and Max Distance, extent, LFE…) can still take curves.
- **Timeline:** with no instruments or markers on the timeline, its length is a guess (10 s). Set the position range in seconds.
- **Panning curves:** apply to Direction with a value range of -180 to 180 (0 is the middle), or to Pan with -100 to 100.
- **"Studio is running an older script":** choose Scripts > Reload in Studio (or restart it), then reopen the Curve Library.

## FMOD 2.04 curve presets

2.04 keeps curve presets in the project. Right-click a curve, folder or library > **Send to Studio's curve presets**: they appear under *Curve Library / <library> / <folders>*, and one of the same name there is replaced. Save the project in Studio to keep them. **Import Studio's presets** (Studio panel) copies the project's presets into a *Studio presets* folder; importing again updates them.

## Look

- The menu beside **Guide** switches between **Classic** (the default) and **DAT Audio** (graphite and amber, like the Network Audio Grabber). The app always stays dark, whatever macOS's appearance.

## Good to know

- Studio draws volume and gain on a fader scale, so a dB curve looks lower there than here. The values are exact.
- Bends are drawn approximately here; Studio gets the exact bend values.
- Undo in Studio may take several steps to reverse one apply.
