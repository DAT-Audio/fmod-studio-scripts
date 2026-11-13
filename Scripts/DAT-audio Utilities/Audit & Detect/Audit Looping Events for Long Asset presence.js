// FMOD Studio Script — Looping Events with Long / Streamed Assets
// Version: 1.1.0
// ---------------------------------------------------------------------------
// Checks every event independently for looping behaviour, then inspects all
// audio assets inside it. Flags any event where a looping event contains an
// asset longer than 10 seconds.
//
// Looping detection:
//   • Timeline events  → LoopRegion marker on any markerTrack
//   • No-timeline events → presence of a continuous (non-discrete) parameter
//     (heuristic: event can sustain indefinitely, labelled "possibly looping")
//
// Asset coverage:
//   • SingleSound    → module.audioFile
//   • MultiSound     → module.sounds[]
//   • SoundScatterer → module.sound
//
// Priority section flags events with no loop keyword in name/path.
// Music/VO events are shown in a separate sub-section within priority.
//
// Part of the DAT-audio Utilities pack — loaded and run by "Audit & Detect.js"
// in the folder above this one.
// Run from: Scripts > DAT-audio Utilities > Audit & Detect... (Run button)
// ---------------------------------------------------------------------------

"use strict";

// Shown in the window title / console output; see the pack's top-level README.
var LPA_VERSION = "1.1.0";

// Exported to "Audit & Detect.js", which shows the title and purpose and
// a status line from `selection`, and calls execute() from the Run button.
module.exports = {
    title:     "Audit Looping Events for Long Asset presence",
    version:   LPA_VERSION,
    selection: "project",   // "project", "events" or "assets"
    purpose:   "Flags looping events that contain an asset longer than 10 seconds. Events with no loop keyword in their name are listed first.",
    execute: function () {

        var THRESHOLD_SEC   = 10;
        var LOOP_KEYWORDS   = ["loop", "lp", "looping", "looped", "lping", "lp_", "_lp"];
        var EXEMPT_KEYWORDS = ["mx", "music", "mus", "vo", "voice", "dialogue", "dialog", "dlg"];

        // ── String helpers ───────────────────────────────────────────────────

        function repeatStr(s, n) {
            var o = "";
            for (var i = 0; i < n; i++) o += s;
            return o;
        }

        // Simple substring match — used for loop keywords.
        // "lp" anywhere in the string counts, e.g. LOOPLAND, LP_Idle, amb_lp.
        function containsSubstring(str, keywords) {
            var lower = ("" + str).toLowerCase();
            for (var i = 0; i < keywords.length; i++) {
                if (lower.indexOf(keywords[i]) !== -1) return true;
            }
            return false;
        }

        // Word-boundary match — used for exempt keywords.
        // Keyword must be surrounded by non-alphanumeric characters so that
        // "vo" doesn't match inside "AstralVoid" or "SourceEvents".
        function containsWord(str, keywords) {
            var lower = ("" + str).toLowerCase();
            for (var i = 0; i < keywords.length; i++) {
                var kw  = keywords[i];
                var idx = lower.indexOf(kw);
                while (idx !== -1) {
                    var before = (idx === 0)                          || !/[a-z0-9]/.test(lower[idx - 1]);
                    var after  = (idx + kw.length >= lower.length)   || !/[a-z0-9]/.test(lower[idx + kw.length]);
                    if (before && after) return true;
                    idx = lower.indexOf(kw, idx + 1);
                }
            }
            return false;
        }

        function hasLoopKeyword(r) {
            return containsSubstring(r.name, LOOP_KEYWORDS) ||
                   containsSubstring(r.path, LOOP_KEYWORDS);
        }

        function isExempt(r) {
            return containsWord(r.name, EXEMPT_KEYWORDS) ||
                   containsWord(r.path, EXEMPT_KEYWORDS);
        }

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

        // ── Loop detection ───────────────────────────────────────────────────

        function hasLoopRegion(event) {
            var found = false;
            try {
                event.markerTracks.forEach(function (mt) {
                    mt.markers.forEach(function (m) {
                        if (m.isOfExactType("LoopRegion")) found = true;
                    });
                });
            } catch (e) {}
            return found;
        }

        function hasContinuousParameter(event) {
            var found = false;
            try {
                event.parameters.forEach(function (p) {
                    var pt = p.parameterType;
                    if (pt === 0 || pt === 3 || pt === 4 || pt === 5 || pt === 6) found = true;
                });
            } catch (e) {}
            return found;
        }

        function getLoopStatus(event) {
            if (hasLoopRegion(event)) return "loop_region";
            try {
                if (!event.timeline && hasContinuousParameter(event)) return "no_timeline_continuous";
            } catch (e) {}
            return null;
        }

        // ── Asset extraction ─────────────────────────────────────────────────

        function assetFilename(assetPath) {
            var s     = "" + assetPath;
            var slash = Math.max(s.lastIndexOf("/"), s.lastIndexOf("\\"));
            return slash >= 0 ? s.substring(slash + 1) : s;
        }

        function makeAssetEntry(af, trackIdx, modIdx, slotIdx) {
            if (!af) return null;
            var len, streaming, path;
            try { len       = af.length;      } catch (e) { len       = 0;     }
            try { streaming = af.isStreaming;  } catch (e) { streaming = false; }
            try { path      = "" + af.assetPath; } catch (e) { path   = "";    }
            if (len <= THRESHOLD_SEC) return null;
            var loc = "Track " + (trackIdx + 1) + ", Module " + (modIdx + 1);
            if (slotIdx !== undefined) loc += ", Slot " + (slotIdx + 1);
            return { filename: assetFilename(path), assetPath: path,
                     lengthSec: len, streaming: streaming, location: loc };
        }

        function collectLongAssets(event) {
            var results = [];
            try {
                event.groupTracks.forEach(function (gt, ti) {
                    try {
                        gt.modules.forEach(function (mod, mi) {
                            var ent = "" + mod.entity;
                            if (ent === "SingleSound") {
                                var e = makeAssetEntry(mod.audioFile, ti, mi, undefined);
                                if (e) results.push(e);
                            } else if (ent === "MultiSound") {
                                try {
                                    mod.sounds.forEach(function (af, si) {
                                        var e = makeAssetEntry(af, ti, mi, si);
                                        if (e) results.push(e);
                                    });
                                } catch (e) {}
                            } else if (ent === "SoundScatterer") {
                                var e = makeAssetEntry(mod.sound, ti, mi, undefined);
                                if (e) results.push(e);
                            }
                        });
                    } catch (e) {}
                });
            } catch (e) {}
            return results;
        }

        // ── Main pass ────────────────────────────────────────────────────────

        var allEvents = [];
        collectAllEvents(studio.project.workspace.masterEventFolder, allEvents);

        var loopLabel = {
            "loop_region":            "Loop Region",
            "no_timeline_continuous": "No timeline — continuous parameter (possibly looping)"
        };

        var flagged = [];
        allEvents.forEach(function (event) {
            var loopStatus = getLoopStatus(event);
            if (!loopStatus) return;
            var longAssets = collectLongAssets(event);
            if (longAssets.length === 0) return;
            var path = "";
            try { path = event.getPath(); } catch (e) {}
            flagged.push({ name: event.name, path: path,
                           loopStatus: loopStatus, assets: longAssets });
        });

        var priority     = flagged.filter(function (r) { return !hasLoopKeyword(r) && !isExempt(r); });
        var priorityMxVO = flagged.filter(function (r) { return !hasLoopKeyword(r) &&  isExempt(r); });

        // ── Format output ────────────────────────────────────────────────────

        function writeEntries(list, offset, lines) {
            list.forEach(function (r, i) {
                lines.push("");
                lines.push("    " + (i + 1 + offset) + ". " + r.name);
                lines.push("       Path      : " + r.path);
                lines.push("       Loop type : " + loopLabel[r.loopStatus]);
                lines.push("       Assets (" + r.assets.length + ") :");
                r.assets.forEach(function (a) {
                    var tag = a.streaming ? " [STREAMED]" : " [not streamed]";
                    lines.push("         • " + a.filename + tag);
                    lines.push("           Length   : " + a.lengthSec.toFixed(2) + "s");
                    lines.push("           Location : " + a.location);
                    lines.push("           Path     : " + a.assetPath);
                });
            });
        }

        var lines = [];
        lines.push("=== Looping Events with Long Assets (>" + THRESHOLD_SEC + "s) ===");
        lines.push("Events checked   : " + allEvents.length);
        lines.push("Events flagged   : " + flagged.length);
        lines.push("Priority         : " + priority.length);
        lines.push("Priority (Mx/VO) : " + priorityMxVO.length);
        lines.push("");

        if (flagged.length === 0) {
            lines.push("PASS: No looping events found with assets longer than " + THRESHOLD_SEC + " seconds.");
        } else {

            // -- Priority -----------------------------------------------------
            lines.push("┌─────────────────────────────────────────────────────────────────────┐");
            lines.push("│  PRIORITY  —  No loop keyword in name or path                       │");
            lines.push("│  Most likely to be unintentional. Check these first.                │");
            lines.push("└─────────────────────────────────────────────────────────────────────┘");

            if (priority.length === 0) {
                lines.push("");
                lines.push("    None.");
            } else {
                writeEntries(priority, 0, lines);
            }

            // -- Priority: Mx/VO sub-section ----------------------------------
            if (priorityMxVO.length > 0) {
                lines.push("");
                lines.push("    ┌─────────────────────────────────────────────────────────────┐");
                lines.push("    │  Possible Music / VO  (" + priorityMxVO.length + ")" +
                           repeatStr(" ", Math.max(0, 37 - ("" + priorityMxVO.length).length)) + "│");
                lines.push("    │  Streaming may be intentional — verify before changing.     │");
                lines.push("    └─────────────────────────────────────────────────────────────┘");
                writeEntries(priorityMxVO, priority.length, lines);
            }

            // -- Full report --------------------------------------------------
            lines.push("");
            lines.push("");
            lines.push("┌─────────────────────────────────────────────────────────────────────┐");
            lines.push("│  FULL REPORT  —  All flagged events (" + flagged.length + ")" +
                       repeatStr(" ", Math.max(0, 32 - ("" + flagged.length).length)) + "│");
            lines.push("│  All looping events with assets >" + THRESHOLD_SEC + "s.                           │");
            lines.push("└─────────────────────────────────────────────────────────────────────┘");
            writeEntries(flagged, 0, lines);
        }

        lines.push("");
        lines.push("=== Check Complete ===");
        lines.push("");

        var out = lines.join("\n");
        studio.system.print("=== Looping Events — Long Asset Check v" + LPA_VERSION + " ===");
        studio.system.print(out);

        studio.ui.showModalDialog({
            windowTitle:   "Looping Events — Long Asset Check  v" + LPA_VERSION,
            widgetType:    studio.ui.widgetType.TextEdit,
            readOnly:      true,
            text:          out,
            minimumWidth:  1280,
            minimumHeight: 960
        });
    }
};
