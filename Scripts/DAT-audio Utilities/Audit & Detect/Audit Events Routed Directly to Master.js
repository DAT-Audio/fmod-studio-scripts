// FMOD Studio Script
// Version: 1.1.0
// Checks all Events in the project and reports any that are routed
// directly to the Master Bus output (i.e. not via an intermediate bus).
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// Results are shown in a scrollable modal dialog window and mirrored to:
// Scripts > Show Script Output
//
// Part of the DAT-audio Utilities pack — loaded and run by "Audit & Detect.js"
// in the folder above this one.
// Run from: Scripts > DAT-audio Utilities > Audit & Detect... (Run button)

"use strict";

// Shown in the window title / console output; see the pack's top-level README.
var RTM_VERSION = "1.1.0";

// Exported to "Audit & Detect.js", which shows the title and purpose and
// a status line from `selection`, and calls execute() from the Run button.
module.exports = {
    title:     "Audit Events Routed Directly to Master Bus",
    version:   RTM_VERSION,
    selection: "project",   // "project", "events" or "assets"
    purpose:   "Lists events whose output goes straight to the Master Bus instead of through a bus.",
    execute: function () {
        var offendingEvents = [];

        // Recursively collect all events in the project
        function collectEvents(folder, results) {
            folder.items.forEach(function (item) {
                if (item.isOfExactType("Event")) {
                    results.push(item);
                } else if (item.isOfExactType("EventFolder")) {
                    collectEvents(item, results);
                }
            });
        }

        var allEvents = [];
        collectEvents(studio.project.workspace.masterEventFolder, allEvents);

        allEvents.forEach(function (event) {
            var mixerInput = event.mixerInput;
            if (!mixerInput) return;

            var output = mixerInput.output;
            if (!output || output.isOfExactType("MixerMaster")) {
                offendingEvents.push({
                    name: event.name,
                    path: event.getPath()
                });
            }
        });

        // ── Build result text ─────────────────────────────────────────────────
        var lines = [];

        lines.push("=== Bus Routing Check ===");
        lines.push("");

        if (offendingEvents.length === 0) {
            lines.push("PASS: All events are routed through an intermediate bus.");
        } else {
            lines.push("FAIL: " + offendingEvents.length + " event(s) routed directly to Master Bus.");
            lines.push("─────────────────────────────────────────────");
            offendingEvents.forEach(function (e, i) {
                lines.push((i + 1) + ".  " + e.name);
                lines.push("    " + e.path);
                lines.push("");
            });
        }

        lines.push("");
        lines.push("=== Check Complete ===");
        lines.push("");

        var resultText = lines.join("\n");

        // ── Mirror to Script Output console ───────────────────────────────────
        studio.system.print("=== Bus Routing Check v" + RTM_VERSION + " ===");
        studio.system.print(resultText);
        studio.system.print("=== Check Complete ===");

        // ── Show results in scrollable modal dialog ───────────────────────────
        studio.ui.showModalDialog({
            windowTitle:   "Bus Routing Check  v" + RTM_VERSION,
            widgetType:    studio.ui.widgetType.TextEdit,
            readOnly:      true,
            text:          resultText,
            minimumWidth:  1280,
            minimumHeight: 960
        });
    }
};
