// FMOD Studio Script — Copy Selected Event Names
// Version: 1.0.0
// ---------------------------------------------------------------------------
// Copies the NAMES of the events selected in the Event browser to the system
// clipboard, one per line — names only, never paths and never GUIDs.
//
//   • Selected events            — the event's own name.
//   • Selected folders           — every event inside, recursively.
//   • Snapshots / snapshot groups— handled the same way (a Snapshot is an Event).
//   • Anything else selected     — ignored, and counted in the summary.
//
// The list is de-duplicated by object (selecting a folder AND an event inside it
// yields that event once) and sorted alphabetically, case-insensitively, so the
// result matches the browser's own ordering and is the same every run.
//
// Read-only: this script never modifies the project.
//
// Nothing outside the FMOD project is read or written — no paths are hardcoded,
// so the script works in any project it is dropped into.
//
// Clipboard: shells out to `pbcopy` (macOS) / `clip` (Windows) and pipes the
// text in on standard input. If that fails for any reason the names are shown
// in a selectable text box so they can still be copied by hand.
//
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// A summary is mirrored to: Scripts > Show Script Output
//
// Part of the DAT-audio Utilities pack — loaded by "DAT-audio Utilities.js"
// in the Scripts directory above this folder.
// Menu: Scripts > DAT-audio Utilities > Copy Selected Event Names
// ---------------------------------------------------------------------------

"use strict";

// Shown in the window title / console output; see the pack's top-level README.
var CSN_VERSION = "1.0.0";

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Copy Selected Event Names",
    execute: function () {

        // ── Type helpers ─────────────────────────────────────────────────────
        // Every managed-object call is wrapped: a stale or partially loaded
        // object throws rather than returning false.

        function isExactly(obj, typeName) {
            try { return obj.isOfExactType(typeName); } catch (e) { return false; }
        }

        function derivesFrom(obj, typeName) {
            try { return obj.isOfType(typeName); } catch (e) { return false; }
        }

        // Containers worth recursing into. NOT "Folder" — Event derives from
        // Folder, so a generic folder test would descend into an event's own
        // items and pick up its internals.
        var CONTAINER_TYPES = ["EventFolder", "SnapshotGroup", "SnapshotList"];

        function isContainer(obj) {
            for (var i = 0; i < CONTAINER_TYPES.length; i++) {
                if (isExactly(obj, CONTAINER_TYPES[i])) return true;
            }
            return false;
        }

        // ── Collect ──────────────────────────────────────────────────────────

        var names   = [];   // event names, in traversal order (sorted later)
        var seen    = {};   // object key -> true, so an event is counted once
        var ignored = 0;    // selected items that were neither event nor folder
        var unnamed = 0;    // events with an empty name

        function objectKey(obj) {
            var key = "";
            try { key = "" + obj.id; } catch (e) {}
            if (key === "") {
                try { key = "" + obj.getPath(); } catch (e) {}
            }
            return key;
        }

        function addEvent(event) {
            var key = objectKey(event);
            if (key !== "") {
                // Prefix with "#" so a key can't collide with an
                // Object.prototype member such as "constructor".
                if (seen["#" + key]) return;
                seen["#" + key] = true;
            }

            var name = "";
            try { name = "" + event.name; } catch (e) {}
            if (name === "") {
                unnamed++;
                return;
            }
            names.push(name);
        }

        // depth is a belt-and-braces guard; the folder tree cannot be circular.
        function walk(item, depth) {
            if (!item || depth > 64) return;

            // Event-ness is tested FIRST: Event derives from Folder, and
            // Snapshot derives from Event, so this catches both.
            if (derivesFrom(item, "Event")) {
                addEvent(item);
                return;
            }

            if (isContainer(item)) {
                var children = [];
                try { children = item.items || []; } catch (e) {}
                for (var i = 0; i < children.length; i++) {
                    walk(children[i], depth + 1);
                }
                return;
            }

            ignored++;
        }

        var selection = [];
        try { selection = studio.window.browserSelection() || []; } catch (e) {}

        if (selection.length === 0) {
            alert("Copy Selected Event Names\n\n" +
                  "Nothing is selected in the Event browser.\n\n" +
                  "Select one or more events (or folders) and run the tool again.");
            return;
        }

        for (var s = 0; s < selection.length; s++) {
            walk(selection[s], 0);
        }

        if (names.length === 0) {
            alert("Copy Selected Event Names\n\n" +
                  "No events found in the selection.\n\n" +
                  "Selected items: " + selection.length +
                  (ignored  ? "\nNot events or event folders: " + ignored : "") +
                  (unnamed  ? "\nEvents with no name: " + unnamed : ""));
            return;
        }

        // Case-insensitive alphabetical, with a case-sensitive tie-break so the
        // order is total and therefore identical on every run.
        names.sort(function (a, b) {
            var al = a.toLowerCase();
            var bl = b.toLowerCase();
            if (al !== bl) return al < bl ? -1 : 1;
            return a < b ? -1 : (a > b ? 1 : 0);
        });

        var uniqueNames = 0;
        var seenName    = {};
        for (var n = 0; n < names.length; n++) {
            if (!seenName["#" + names[n]]) {
                seenName["#" + names[n]] = true;
                uniqueNames++;
            }
        }

        // ── Copy to the clipboard ────────────────────────────────────────────

        var platform = "";
        try { platform = "" + studio.os.platform; } catch (e) {}
        var isWindows = (platform === "win");

        // CRLF on Windows so the list pastes correctly into Notepad and friends.
        var payload = names.join(isWindows ? "\r\n" : "\n");
        var display = names.join("\n");

        var copied      = false;
        var copyFailure = "";

        try {
            // `standardInput` is not in the scripting manual but is used by
            // FMOD's own shipped SVN.sc.js, which is where it came from. The
            // timeout guards against a helper that never reads its input.
            var result = studio.system.start(isWindows ? "clip" : "/usr/bin/pbcopy", {
                standardInput: payload,
                timeout:       10000
            });

            if (result && typeof result.exitCode === "number" && result.exitCode !== 0) {
                copyFailure = "the clipboard helper exited with code " + result.exitCode;
                if (result.standardError) {
                    copyFailure += " (" + result.standardError + ")";
                }
            } else {
                copied = true;
            }
        } catch (e) {
            copyFailure = "" + e;
        }

        // ── Report ───────────────────────────────────────────────────────────

        var summary = names.length + " event name" + (names.length === 1 ? "" : "s");
        if (uniqueNames !== names.length) {
            summary += " (" + uniqueNames + " unique)";
        }

        var notes = [];
        if (ignored) {
            notes.push(ignored + " selected item" + (ignored === 1 ? " was" : "s were") +
                       " not an event or event folder and " +
                       (ignored === 1 ? "was" : "were") + " ignored.");
        }
        if (unnamed) {
            notes.push(unnamed + " event" + (unnamed === 1 ? "" : "s") + " had no name.");
        }

        var status = copied
            ? "Copied " + summary + " to the clipboard."
            : "Could not reach the clipboard — " + copyFailure +
              ".\nSelect the text below and copy it manually.";

        studio.system.print("=== Copy Selected Event Names v" + CSN_VERSION + " ===\n" +
                            status + "\n" +
                            (notes.length ? notes.join("\n") + "\n" : "") +
                            display + "\n");

        studio.ui.showModalDialog({
            windowTitle:   "Copy Selected Event Names  v" + CSN_VERSION,
            widgetType:    studio.ui.widgetType.Layout,
            layout:        studio.ui.layoutType.VBoxLayout,
            windowWidth:   460,
            windowHeight:  520,
            items: [
                { widgetType: studio.ui.widgetType.Label,
                  text: copied ? "<b>" + status + "</b>"
                               : "<b style='color:#c0392b'>" + status + "</b>" },

                (notes.length
                    ? { widgetType: studio.ui.widgetType.Label, text: notes.join("<br/>") }
                    : { widgetType: studio.ui.widgetType.Spacer, minimumHeight: 0 }),

                { widgetType:    studio.ui.widgetType.TextEdit,
                  isReadOnly:    false,          // editable so the text can be selected and copied
                  text:          display,
                  minimumWidth:  420,
                  minimumHeight: 380 },

                {
                    widgetType: studio.ui.widgetType.Layout,
                    layout:     studio.ui.layoutType.HBoxLayout,
                    items: [
                        { widgetType: studio.ui.widgetType.Spacer },
                        { widgetType: studio.ui.widgetType.PushButton,
                          text:       "Close",
                          onClicked:  function () { this.closeDialog(); } }
                    ]
                }
            ]
        });
    }
});
