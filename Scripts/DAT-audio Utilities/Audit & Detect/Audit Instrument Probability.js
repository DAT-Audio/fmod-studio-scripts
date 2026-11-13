// FMOD Studio Script — Audit Instrument Probability
// Version: 1.1.0
// ---------------------------------------------------------------------------
// Reports events whose instruments have Trigger Probability switched ON but set
// to a value that is almost certainly not what was intended:
//
//   • REDUNDANT      — probability is on but at/above the "redundant" threshold
//                      (default 100%). The instrument always plays, so the
//                      probability gate does nothing. Turn it off, or lower it.
//   • SUSPICIOUSLY   — probability is on but below the "low" threshold
//     LOW              (default 10%). Rarely deliberate; usually a typo (5
//                      instead of 50) or a leftover from testing. 0% never
//                      plays at all.
//
// Scope (choose one in the dialog):
//   • Whole project        — every event under the master event folder.
//   • Selection            — only the events / folders selected in the Event
//                            browser. Folders are recursed.
//       └ Extended search  — additionally follow event-reference instruments
//                            out to events outside the selection (recursively).
//
// Read-only: this script never modifies the project.
//
// All paths are relative — nothing outside the FMOD project is read or written,
// so the script works in any project it is dropped into.
//
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// Results are shown in a scrollable modal dialog window and mirrored to:
// Scripts > Show Script Output
//
// Part of the DAT-audio Utilities pack — loaded and run by "Audit & Detect.js"
// in the folder above this one.
// Run from: Scripts > DAT-audio Utilities > Audit & Detect... (Run button)
//
// File-scope helpers are prefixed "pc" because required scripts share a global
// namespace with the rest of the pack.
// ---------------------------------------------------------------------------

"use strict";

// ── Small string helpers ────────────────────────────────────────────────────
// (The Studio JS engine is ES5-ish: no String.prototype.repeat, no arrow
// functions, no template literals.)

function pcRepeat(str, n) {
    var out = "";
    for (var i = 0; i < n; i++) out += str;
    return out;
}

function pcPadRight(str, n) {
    if (str.length >= n) return str;
    return str + pcRepeat(" ", n - str.length);
}

function pcPadLeft(str, n) {
    if (str.length >= n) return str;
    return pcRepeat(" ", n - str.length) + str;
}

// "SomeDir/file.wav" or "SomeDir\file.wav" -> "file.wav"
function pcBaseName(path) {
    var a = path.lastIndexOf("/");
    var b = path.lastIndexOf("\\");
    var i = a > b ? a : b;
    return i >= 0 ? path.substring(i + 1) : path;
}

// Trims pointless decimals: 100 -> "100%", 12.5 -> "12.5%"
function pcFormatPct(v) {
    if (typeof v !== "number" || isNaN(v)) return "?";
    return (Math.round(v * 1000) / 1000) + "%";
}

// A framed block of text, used for the section headers in the report.
function pcBox(textLines, width) {
    var lines = [];
    lines.push("┌" + pcRepeat("─", width + 4) + "┐");
    for (var i = 0; i < textLines.length; i++) {
        var t = textLines[i];
        if (t.length > width) t = t.substring(0, width);
        lines.push("│  " + pcPadRight(t, width) + "  │");
    }
    lines.push("└" + pcRepeat("─", width + 4) + "┘");
    return lines;
}

// ── Project traversal ───────────────────────────────────────────────────────

// Every event under a folder, recursively.
// NOTE: Event derives from Folder in the object model, so isOfType("Folder") is
// true for events too. Recurse on isOfExactType("EventFolder") only, otherwise
// we descend into an event's own items and double-count.
function pcCollectEvents(folder, out) {
    var items = [];
    try { items = folder.items || []; } catch (e) { return; }
    for (var i = 0; i < items.length; i++) {
        var item = items[i];
        try {
            if (item.isOfExactType("Event")) out.push(item);
            else if (item.isOfExactType("EventFolder")) pcCollectEvents(item, out);
        } catch (e) {}
    }
}

// The audio tracks of an event: master + group + return tracks. Instruments
// hang off AudioTrack.modules, across every parameter sheet of the event.
function pcTrackList(ev) {
    var tracks = [];
    try { if (ev.masterTrack) tracks.push(ev.masterTrack); } catch (e) {}
    try {
        var g = ev.groupTracks || [];
        for (var i = 0; i < g.length; i++) tracks.push(g[i]);
    } catch (e) {}
    try {
        var r = ev.returnTracks || [];
        for (var j = 0; j < r.length; j++) tracks.push(r[j]);
    } catch (e) {}
    return tracks;
}

// Track display name. A track's name lives on its mixer group, not the track.
function pcTrackLabel(tr, index) {
    try { if (tr.isOfExactType("MasterTrack")) return "Master"; } catch (e) {}
    try {
        if (tr.mixerGroup && tr.mixerGroup.name) {
            var n = "" + tr.mixerGroup.name;
            if (n !== "") return n;
        }
    } catch (e) {}
    try {
        if (tr.name) {
            var n2 = "" + tr.name;
            if (n2 !== "") return n2;
        }
    } catch (e) {}
    return "track " + (index + 1);
}

// Friendly instrument-type name, from the entity type.
function pcInstrumentKind(m) {
    var entity = "";
    try { entity = "" + m.entity; } catch (e) {}
    if (entity === "SingleSound")     return "Single";
    if (entity === "MultiSound")      return "Multi";
    if (entity === "EventSound")      return "Event ref";
    if (entity === "ProgrammerSound") return "Programmer";
    if (entity === "SilenceSound")    return "Silence";
    if (entity === "PluginSound")     return "Plugin";
    if (entity === "SoundScatterer")  return "Scatterer";
    if (entity === "CommandSound")    return "Command";
    return entity === "" ? "Instrument" : entity;
}

// 'Single "footstep_01.wav"' — falls back through name, asset, referenced event.
function pcInstrumentLabel(m) {
    var kind = pcInstrumentKind(m);
    var nm = "";

    try { if (m.name) nm = "" + m.name; } catch (e) {}

    if (nm === "") {
        try {
            if (m.isOfExactType("SingleSound") && m.audioFile && m.audioFile.assetPath) {
                nm = pcBaseName("" + m.audioFile.assetPath);
            }
        } catch (e) {}
    }
    if (nm === "") {
        try {
            if (m.isOfExactType("EventSound") && m.event) nm = "" + m.event.getPath();
        } catch (e) {}
    }

    return nm === "" ? kind : (kind + " \"" + nm + "\"");
}

// Which parameter sheet the instrument sits on. Empty string for the main
// timeline (the common case), so the report stays uncluttered.
function pcSheetLabel(m) {
    var p = null;
    try { p = m.parameter; } catch (e) { return ""; }
    if (!p) return "";
    try { if (p.isOfExactType("TransitionTimeline")) return "transition timeline"; } catch (e) {}
    try { if (p.isOfType("Timeline")) return ""; } catch (e) {}
    try {
        if (p.name) {
            var n = "" + p.name;
            if (n !== "") return n;
        }
    } catch (e) {}
    try {
        if (p.preset && p.preset.name) {
            var n2 = "" + p.preset.name;
            if (n2 !== "") return n2;
        }
    } catch (e) {}
    return "parameter sheet";
}

// Reads the trigger-probability pair off an instrument.
// supported=false means the property is absent on this Studio version, which is
// reported separately rather than silently counted as "off".
function pcReadProbability(m) {
    var enabled = false;
    var prob    = null;

    try { enabled = !!m.triggerProbabilityEnabled; } catch (e) {}

    try { prob = m.triggerProbability; } catch (e) { prob = null; }
    if (typeof prob !== "number") {
        try { prob = m.properties.triggerProbability.value; } catch (e) { prob = null; }
    }

    return {
        enabled:   enabled,
        prob:      (typeof prob === "number" && !isNaN(prob)) ? prob : null,
        supported: (typeof prob === "number" && !isNaN(prob))
    };
}

// One instrument (and, recursively, the entries nested inside a multi
// instrument or a scatterer playlist).
function pcVisitModule(m, trackName, prefix, out, seen) {
    if (!m) return;

    var key = null;
    try { key = "#" + m.id; } catch (e) { key = null; }
    if (key !== null) {
        if (seen[key]) return;
        seen[key] = true;
    }

    // Only Sound-derived modules are instruments. TransitionSourceSound /
    // TransitionDestinationSound derive from Module but are not instruments.
    var isSound = false;
    try { isSound = m.isOfType("Sound"); } catch (e) {}
    if (!isSound) return;

    var label = prefix + pcInstrumentLabel(m);
    var pr    = pcReadProbability(m);

    out.list.push({
        label:     label,
        track:     trackName,
        sheet:     pcSheetLabel(m),
        enabled:   pr.enabled,
        prob:      pr.prob,
        supported: pr.supported
    });

    // Event references — the extended search follows these.
    try {
        if (m.isOfExactType("EventSound") && m.event) out.refs.push(m.event);
    } catch (e) {}

    // Nested playlist entries.
    var children = [];
    try {
        if (m.isOfExactType("MultiSound") && m.sounds) children = m.sounds;
    } catch (e) {}
    if (children.length === 0) {
        try {
            if (m.isOfExactType("SoundScatterer") && m.sound && m.sound.sounds) {
                children = m.sound.sounds;
            }
        } catch (e) {}
    }

    var nestedPrefix = label + " ▸ ";
    for (var c = 0; c < children.length; c++) {
        pcVisitModule(children[c], trackName, nestedPrefix, out, seen);
    }
}

// Every instrument in one event, plus the events it references.
function pcGatherInstruments(ev) {
    var out    = { list: [], refs: [] };
    var seen   = {};
    var tracks = pcTrackList(ev);

    for (var t = 0; t < tracks.length; t++) {
        var tr    = tracks[t];
        var tname = pcTrackLabel(tr, t);
        var mods  = [];
        try { mods = tr.modules || []; } catch (e) { mods = []; }
        for (var m = 0; m < mods.length; m++) {
            pcVisitModule(mods[m], tname, "", out, seen);
        }
    }
    return out;
}

// ── The tool ────────────────────────────────────────────────────────────────

// Shown in the window title / console output; see the pack's top-level README.
var PC_VERSION = "1.1.0";

// Exported to "Audit & Detect.js", which shows the title and purpose and
// a status line from `selection`, and calls execute() from the Run button.
module.exports = {
    title:     "Audit Instrument Probability",
    version:   PC_VERSION,
    selection: "events",   // "project", "events" or "assets"
    purpose:   "Finds instruments with Trigger Probability on but at 100% or more (the gate does nothing) or under 10% (usually a typo). An options window sets the scope and thresholds.",

    isEnabled: function () { return studio.project !== null; },

    execute: function () {

        var DEFAULT_HIGH = 100;   // report probability >= this as redundant
        var DEFAULT_LOW  = 10;    // report probability <  this as suspicious
        var BOX_WIDTH    = 68;

        // ── Read the browser selection up front ──────────────────────────────
        // It is a snapshot: the options dialog is modal, so it cannot change
        // underneath us.
        var selEvents  = [];
        var selFolders = [];
        var selIgnored = 0;
        try {
            var sel = studio.window.browserSelection() || [];
            for (var s = 0; s < sel.length; s++) {
                var selItem = sel[s];
                var isEvent = false, isFolder = false;
                try { isEvent  = selItem.isOfExactType("Event"); }       catch (e) {}
                try { isFolder = selItem.isOfExactType("EventFolder"); } catch (e) {}
                if (isEvent)       selEvents.push(selItem);
                else if (isFolder) selFolders.push(selItem);
                else               selIgnored++;
            }
        } catch (e) {}

        var hasSelection  = (selEvents.length + selFolders.length) > 0;
        var selectionText = hasSelection
            ? (selEvents.length + " event" + (selEvents.length === 1 ? "" : "s") +
               ", " + selFolders.length + " folder" + (selFolders.length === 1 ? "" : "s") +
               (selIgnored > 0 ? ", " + selIgnored + " other item(s) ignored" : ""))
            : "nothing selected in the Event browser";

        // ── Options dialog ───────────────────────────────────────────────────
        // FMOD's UI API has no RadioButton widget, so the two scope options are
        // mutually exclusive CheckBoxes: ticking one unticks the other, and one
        // is always ticked. `syncing` stops setChecked() re-entering onToggled.
        var opts = {
            run:      false,
            whole:    true,
            extended: false,
            high:     DEFAULT_HIGH,
            low:      DEFAULT_LOW
        };
        var syncing = false;

        function pcSyncScope(widget, otherId) {
            if (syncing) return;
            syncing = true;
            try {
                var other = widget.findWidget(otherId);
                if (widget.isChecked()) {
                    if (other) other.setChecked(false);
                } else if (other && !other.isChecked()) {
                    widget.setChecked(true);          // never leave both unticked
                }
                var wholeW = widget.findWidget("scopeWhole");
                var extW   = widget.findWidget("extended");
                if (extW) extW.setEnabled(!!(wholeW && !wholeW.isChecked()) && hasSelection);
            } catch (e) {}
            syncing = false;
        }

        studio.ui.showModalDialog({
            windowTitle:  "Instrument Probability Audit  v" + PC_VERSION,
            windowWidth:  620,
            windowHeight: 470,
            widgetType:   studio.ui.widgetType.Layout,
            layout:       studio.ui.layoutType.VBoxLayout,
            spacing:      8,
            contentsMargins: { left: 14, right: 14, top: 14, bottom: 14 },
            items: [
                { widgetType: studio.ui.widgetType.Label,
                  text: "<b>Instrument Probability Checker</b>" },
                { widgetType: studio.ui.widgetType.Label,
                  text: "Finds instruments whose Trigger Probability is switched on but set to a " +
                        "value that does nothing (too high) or almost nothing (too low). " +
                        "Read-only — nothing in the project is changed.",
                  wordWrap: true },
                { widgetType: studio.ui.widgetType.Label, text: "<hr/><b>Thresholds</b>" },

                {
                    widgetType: studio.ui.widgetType.Layout,
                    layout:     studio.ui.layoutType.GridLayout,
                    spacing:    6,
                    items: [
                        { row: 0, column: 0, widgetType: studio.ui.widgetType.Label,
                          text: "Report as redundant when probability is ≥" },
                        { row: 0, column: 1, widgetId: "high",
                          widgetType: studio.ui.widgetType.LineEdit,
                          text: "" + DEFAULT_HIGH, minimumWidth: 60,
                          sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed,
                                        verticalPolicy:   studio.ui.sizePolicy.Fixed } },
                        { row: 0, column: 2, widgetType: studio.ui.widgetType.Label, text: "%" },

                        { row: 1, column: 0, widgetType: studio.ui.widgetType.Label,
                          text: "Report as suspiciously low when probability is &lt;" },
                        { row: 1, column: 1, widgetId: "low",
                          widgetType: studio.ui.widgetType.LineEdit,
                          text: "" + DEFAULT_LOW, minimumWidth: 60,
                          sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed,
                                        verticalPolicy:   studio.ui.sizePolicy.Fixed } },
                        { row: 1, column: 2, widgetType: studio.ui.widgetType.Label, text: "%" }
                    ]
                },
                { widgetType: studio.ui.widgetType.Label,
                  text: "<i>Both values must be between 0 and 100, and the low value must be " +
                        "below the high one.</i>" },

                { widgetType: studio.ui.widgetType.Label, text: "<hr/><b>Scope</b>" },
                { widgetId: "scopeWhole", widgetType: studio.ui.widgetType.CheckBox,
                  text: "Whole project — every event under the master event folder",
                  isChecked: true,
                  onToggled: function () { pcSyncScope(this, "scopeSelection"); } },
                { widgetId: "scopeSelection", widgetType: studio.ui.widgetType.CheckBox,
                  text: "Selected event(s) / folder(s) in the Event browser",
                  isChecked: false,
                  isEnabled: hasSelection,
                  onToggled: function () { pcSyncScope(this, "scopeWhole"); } },
                { widgetType: studio.ui.widgetType.Label,
                  text: "<i>Current selection: " + selectionText + "</i>" },

                { widgetId: "extended", widgetType: studio.ui.widgetType.CheckBox,
                  text: "        ↳  Extended search: also follow referenced events outside the selection",
                  isChecked: false,
                  isEnabled: false },
                { widgetType: studio.ui.widgetType.Label,
                  text: "<i>Only applies to the selection scope — a whole-project scan already " +
                        "covers every referenced event.</i>" },

                { widgetType: studio.ui.widgetType.Label, text: "<hr/>" },
                {
                    widgetType: studio.ui.widgetType.Layout,
                    layout:     studio.ui.layoutType.HBoxLayout,
                    spacing:    8,
                    items: [
                        {
                            widgetType: studio.ui.widgetType.PushButton,
                            text:       "Run Check",
                            onClicked:  function () {
                                var highW = this.findWidget("high");
                                var lowW  = this.findWidget("low");
                                var high  = highW ? parseFloat(highW.text()) : NaN;
                                var low   = lowW  ? parseFloat(lowW.text())  : NaN;

                                if (isNaN(high) || high < 0 || high > 100) {
                                    alert("The redundant threshold must be a number between 0 and 100.");
                                    return;
                                }
                                if (isNaN(low) || low < 0 || low > 100) {
                                    alert("The suspiciously-low threshold must be a number between 0 and 100.");
                                    return;
                                }
                                if (low >= high) {
                                    alert("The low threshold (" + low + "%) must be below the high " +
                                          "threshold (" + high + "%), otherwise the two reports overlap.");
                                    return;
                                }

                                var wholeW = this.findWidget("scopeWhole");
                                var extW   = this.findWidget("extended");
                                opts.whole    = wholeW ? wholeW.isChecked() : true;
                                opts.extended = (!opts.whole && extW) ? extW.isChecked() : false;
                                opts.high     = high;
                                opts.low      = low;
                                opts.run      = true;
                                this.closeDialog();
                            }
                        },
                        { widgetType: studio.ui.widgetType.PushButton, text: "Cancel",
                          onClicked: function () { this.closeDialog(); } }
                    ]
                }
            ]
        });

        if (!opts.run) return;

        // ── Build the root event set ─────────────────────────────────────────
        var roots = [];
        if (opts.whole) {
            pcCollectEvents(studio.project.workspace.masterEventFolder, roots);
        } else {
            for (var e1 = 0; e1 < selEvents.length; e1++) roots.push(selEvents[e1]);
            for (var f1 = 0; f1 < selFolders.length; f1++) pcCollectEvents(selFolders[f1], roots);
        }

        if (roots.length === 0) {
            alert(opts.whole
                ? "This project contains no events."
                : "The selection contains no events.\n\nSelect one or more events or folders in " +
                  "the Event browser, then run the script again.");
            return;
        }

        // ── Scan ─────────────────────────────────────────────────────────────
        // Breadth-first over the roots, then (in extended mode) out through
        // event-reference instruments. `visited` keys on the event GUID, so an
        // event reached twice — or selected and also referenced — is scanned once.
        var queue   = [];
        var visited = {};
        var scanned = [];   // { ev, path, referenced }

        for (var r = 0; r < roots.length; r++) queue.push({ ev: roots[r], referenced: false });

        var instrumentsScanned = 0;
        var probabilityOn      = 0;
        var unsupported        = 0;
        var highHitsTotal      = 0;
        var lowHitsTotal       = 0;
        var highGroups         = [];   // { path, referenced, hits[] }
        var lowGroups          = [];

        while (queue.length > 0) {
            var item = queue.shift();
            var ev   = item.ev;

            var key = null;
            try { key = "#" + ev.id; } catch (e) { key = null; }
            if (key !== null) {
                if (visited[key]) continue;
                visited[key] = true;
            }

            var path = "";
            try { path = "" + ev.getPath(); } catch (e) { path = "(unnamed event)"; }
            scanned.push({ ev: ev, path: path, referenced: item.referenced });

            var gathered = pcGatherInstruments(ev);
            var highHits = [];
            var lowHits  = [];

            for (var i = 0; i < gathered.list.length; i++) {
                var inst = gathered.list[i];
                instrumentsScanned++;

                if (!inst.supported) { unsupported++; continue; }
                if (!inst.enabled)   continue;

                probabilityOn++;
                if (inst.prob >= opts.high)     { highHits.push(inst); highHitsTotal++; }
                else if (inst.prob < opts.low)  { lowHits.push(inst);  lowHitsTotal++;  }
            }

            if (highHits.length > 0) {
                highGroups.push({ path: path, referenced: item.referenced, hits: highHits });
            }
            if (lowHits.length > 0) {
                lowGroups.push({ path: path, referenced: item.referenced, hits: lowHits });
            }

            if (opts.extended) {
                for (var g = 0; g < gathered.refs.length; g++) {
                    queue.push({ ev: gathered.refs[g], referenced: true });
                }
            }
        }

        var referencedCount = 0;
        for (var sc = 0; sc < scanned.length; sc++) {
            if (scanned[sc].referenced) referencedCount++;
        }

        // Stable, scannable output: alphabetical by event path.
        function pcByPath(a, b) {
            var ap = a.path.toLowerCase();
            var bp = b.path.toLowerCase();
            return ap < bp ? -1 : (ap > bp ? 1 : 0);
        }
        highGroups.sort(pcByPath);
        lowGroups.sort(pcByPath);

        // ── Format the report ────────────────────────────────────────────────

        function pcWriteGroups(groups, lines, isLow) {
            for (var i = 0; i < groups.length; i++) {
                var grp = groups[i];
                lines.push("");
                lines.push("    " + (i + 1) + ". " + grp.path +
                           (grp.referenced ? "   [referenced event]" : ""));
                for (var h = 0; h < grp.hits.length; h++) {
                    var hit  = grp.hits[h];
                    var note = "";
                    if (isLow && hit.prob === 0) note = "   ← never plays";
                    var where = "[" + hit.track + (hit.sheet !== "" ? " · " + hit.sheet : "") + "]";
                    lines.push("         • " + pcPadLeft(pcFormatPct(hit.prob), 8) + "   " +
                               pcPadRight(hit.label, 52) + " " + where + note);
                }
            }
        }

        var scopeLine = opts.whole
            ? "Whole project"
            : ("Selection — " + selEvents.length + " event(s), " + selFolders.length + " folder(s)");
        var extLine = opts.whole
            ? "n/a (whole-project scan)"
            : (opts.extended
                ? ("On — " + referencedCount + " referenced event(s) pulled in")
                : "Off");

        var lines = [];
        lines.push("=== Instrument Probability Check ===");
        lines.push("Scope                      : " + scopeLine);
        lines.push("Extended search            : " + extLine);
        lines.push("Redundant threshold        : >= " + opts.high + "%");
        lines.push("Suspiciously-low threshold : <  " + opts.low + "%");
        lines.push("");
        lines.push("Events scanned             : " + scanned.length);
        lines.push("Instruments scanned        : " + instrumentsScanned);
        lines.push("  with probability on      : " + probabilityOn);
        lines.push("Redundant instruments      : " + highHitsTotal +
                   "  (in " + highGroups.length + " event(s))");
        lines.push("Suspiciously low           : " + lowHitsTotal +
                   "  (in " + lowGroups.length + " event(s))");
        if (unsupported > 0) {
            lines.push("No probability property    : " + unsupported +
                       "  (instrument type or Studio version does not expose it)");
        }
        lines.push("");

        if (highHitsTotal === 0 && lowHitsTotal === 0) {
            lines.push("PASS: No instrument has probability enabled at >= " + opts.high +
                       "% or < " + opts.low + "%.");
        } else {
            var hb = pcBox([
                "REDUNDANT PROBABILITY  (" + highHitsTotal + ")",
                "Probability is switched ON but set at or above " + opts.high + "%, so it",
                "never gates anything. Turn it off, or lower the value."
            ], BOX_WIDTH);
            for (var a = 0; a < hb.length; a++) lines.push(hb[a]);

            if (highGroups.length === 0) {
                lines.push("");
                lines.push("    None.");
            } else {
                pcWriteGroups(highGroups, lines, false);
            }

            lines.push("");
            lines.push("");

            var lb = pcBox([
                "SUSPICIOUSLY LOW PROBABILITY  (" + lowHitsTotal + ")",
                "Probability is switched ON and below " + opts.low + "%. Rarely deliberate —",
                "usually a typo or a leftover from testing. 0% never plays at all."
            ], BOX_WIDTH);
            for (var b2 = 0; b2 < lb.length; b2++) lines.push(lb[b2]);

            if (lowGroups.length === 0) {
                lines.push("");
                lines.push("    None.");
            } else {
                pcWriteGroups(lowGroups, lines, true);
            }
        }

        lines.push("");
        lines.push("=== Check Complete ===");
        lines.push("");

        var out = lines.join("\n");

        // ── Mirror to the Script Output console ──────────────────────────────
        studio.system.print("=== Instrument Probability Audit v" + PC_VERSION + " ===");
        studio.system.print(out);

        // ── Show results in a scrollable modal dialog ───────────────────────
        studio.ui.showModalDialog({
            windowTitle:   "Instrument Probability Audit  v" + PC_VERSION,
            widgetType:    studio.ui.widgetType.TextEdit,
            isReadOnly:    true,
            text:          out,
            minimumWidth:  1280,
            minimumHeight: 960
        });
    }
};
