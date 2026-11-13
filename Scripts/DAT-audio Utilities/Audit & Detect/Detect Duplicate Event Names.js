// FMOD Studio Script — Detect Duplicate Event Names
// Version: 1.1.0
// ---------------------------------------------------------------------------
// Checks every event in the project and reports events that share the same
// name, regardless of where they sit in the folder hierarchy.
//
// Purpose: game code that references events by name only (not the full path)
// cannot distinguish between two events called the same thing in different
// folders. Those collisions are ambiguous at runtime and need renaming.
//
// Two sections are reported:
//   • EXACT DUPLICATES        — identical names. Always a collision.
//   • CASE-ONLY DUPLICATES    — names differing only by letter case, e.g.
//                               "Footstep" vs "footstep". A collision for any
//                               case-insensitive lookup; listed separately.
//
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// Results are shown in a scrollable modal dialog window and mirrored to:
// Scripts > Show Script Output
//
// Part of the DAT-audio Utilities pack — loaded and run by "Audit & Detect.js"
// in the folder above this one.
// Run from: Scripts > DAT-audio Utilities > Audit & Detect... (Run button)
// ---------------------------------------------------------------------------

"use strict";

// Shown in the window title / console output; see the pack's top-level README.
var DUP_VERSION = "1.1.0";

// Exported to "Audit & Detect.js", which shows the title and purpose and
// a status line from `selection`, and calls execute() from the Run button.
module.exports = {
    title:     "Detect Duplicate Event Names",
    version:   DUP_VERSION,
    selection: "project",   // "project", "events" or "assets"
    purpose:   "Finds events that share a name anywhere in the project, which game code that looks events up by name can't tell apart.",
    execute: function () {

        // ── Collect all events ───────────────────────────────────────────────

        function collectAllEvents(folder, out) {
            folder.items.forEach(function (item) {
                if (item.isOfExactType("Event")) {
                    out.push(item);
                } else if (item.isOfExactType("EventFolder")) {
                    collectAllEvents(item, out);
                }
            });
        }

        var allEvents = [];
        collectAllEvents(studio.project.workspace.masterEventFolder, allEvents);

        // ── Group by name ────────────────────────────────────────────────────
        // exactGroups : name          -> [ {name, path}, ... ]
        // caseGroups  : lowercased    -> [ {name, path}, ... ]

        var exactGroups = {};
        var caseGroups  = {};
        var order       = [];   // preserves first-seen order of exact names

        allEvents.forEach(function (event) {
            var name = "";
            var path = "";
            try { name = "" + event.name;      } catch (e) {}
            try { path = "" + event.getPath(); } catch (e) {}
            if (name === "") return;

            var entry = { name: name, path: path };

            // Prefix keys with "#" so names like "constructor" or "toString"
            // can't collide with Object.prototype members.
            var exactKey = "#" + name;
            var caseKey  = "#" + name.toLowerCase();

            if (!exactGroups[exactKey]) {
                exactGroups[exactKey] = [];
                order.push(exactKey);
            }
            exactGroups[exactKey].push(entry);

            if (!caseGroups[caseKey]) caseGroups[caseKey] = [];
            caseGroups[caseKey].push(entry);
        });

        // ── Build duplicate lists ────────────────────────────────────────────

        var exactDupes = [];   // { name, entries[] }
        order.forEach(function (key) {
            var entries = exactGroups[key];
            if (entries.length > 1) {
                exactDupes.push({ name: entries[0].name, entries: entries });
            }
        });

        // Case-only duplicates: a lowercase group holding more than one
        // *distinct* spelling. Groups that are a single spelling repeated are
        // already covered by the exact section above.
        var caseOnlyDupes = [];
        var seenCaseKeys  = {};
        allEvents.forEach(function (event) {
            var name = "";
            try { name = "" + event.name; } catch (e) {}
            if (name === "") return;

            var caseKey = "#" + name.toLowerCase();
            if (seenCaseKeys[caseKey]) return;
            seenCaseKeys[caseKey] = true;

            var entries   = caseGroups[caseKey];
            var spellings = {};
            var count     = 0;
            entries.forEach(function (e) {
                if (!spellings["#" + e.name]) {
                    spellings["#" + e.name] = true;
                    count++;
                }
            });
            if (count > 1) {
                caseOnlyDupes.push({ name: entries[0].name.toLowerCase(), entries: entries });
            }
        });

        // Sort each section alphabetically by name for a stable, scannable report.
        function byName(a, b) {
            var an = a.name.toLowerCase();
            var bn = b.name.toLowerCase();
            return an < bn ? -1 : (an > bn ? 1 : 0);
        }
        exactDupes.sort(byName);
        caseOnlyDupes.sort(byName);

        var exactAffected = 0;
        exactDupes.forEach(function (g) { exactAffected += g.entries.length; });

        // ── Format output ────────────────────────────────────────────────────

        function repeatStr(s, n) {
            var o = "";
            for (var i = 0; i < n; i++) o += s;
            return o;
        }

        function writeGroups(list, lines) {
            list.forEach(function (g, i) {
                lines.push("");
                lines.push("    " + (i + 1) + ". \"" + g.name + "\"  —  " +
                           g.entries.length + " events");
                g.entries.forEach(function (e) {
                    lines.push("         • " + e.path);
                    if (e.name !== g.name) {
                        lines.push("           (spelled: " + e.name + ")");
                    }
                });
            });
        }

        var lines = [];
        lines.push("=== Duplicate Event Names ===");
        lines.push("Events checked        : " + allEvents.length);
        lines.push("Unique names          : " + order.length);
        lines.push("Duplicated names      : " + exactDupes.length);
        lines.push("Events affected       : " + exactAffected);
        lines.push("Case-only collisions  : " + caseOnlyDupes.length);
        lines.push("");

        if (exactDupes.length === 0 && caseOnlyDupes.length === 0) {
            lines.push("PASS: All event names are unique across the project.");
        } else {

            // -- Exact duplicates ---------------------------------------------
            lines.push("┌─────────────────────────────────────────────────────────────────────┐");
            lines.push("│  EXACT DUPLICATES  (" + exactDupes.length + ")" +
                       repeatStr(" ", Math.max(0, 47 - ("" + exactDupes.length).length)) + "│");
            lines.push("│  Identical names. Ambiguous for any name-only lookup.               │");
            lines.push("└─────────────────────────────────────────────────────────────────────┘");

            if (exactDupes.length === 0) {
                lines.push("");
                lines.push("    None.");
            } else {
                writeGroups(exactDupes, lines);
            }

            // -- Case-only duplicates -----------------------------------------
            lines.push("");
            lines.push("");
            lines.push("┌─────────────────────────────────────────────────────────────────────┐");
            lines.push("│  CASE-ONLY DUPLICATES  (" + caseOnlyDupes.length + ")" +
                       repeatStr(" ", Math.max(0, 43 - ("" + caseOnlyDupes.length).length)) + "│");
            lines.push("│  Differ only by letter case — a collision if lookup ignores case.   │");
            lines.push("│  Full group listed, so exact duplicates may repeat from above.      │");
            lines.push("└─────────────────────────────────────────────────────────────────────┘");

            if (caseOnlyDupes.length === 0) {
                lines.push("");
                lines.push("    None.");
            } else {
                writeGroups(caseOnlyDupes, lines);
            }
        }

        lines.push("");
        lines.push("=== Check Complete ===");
        lines.push("");

        var out = lines.join("\n");

        // ── Mirror to Script Output console ───────────────────────────────────
        studio.system.print("=== Duplicate Event Names v" + DUP_VERSION + " ===");
        studio.system.print(out);

        // ── Show results in scrollable modal dialog ───────────────────────────
        studio.ui.showModalDialog({
            windowTitle:   "Duplicate Event Names  v" + DUP_VERSION,
            widgetType:    studio.ui.widgetType.TextEdit,
            readOnly:      true,
            text:          out,
            minimumWidth:  1280,
            minimumHeight: 960
        });
    }
};
