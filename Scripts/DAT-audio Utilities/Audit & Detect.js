// FMOD Studio Script — Audit & Detect
// Version: 1.0.0
// ---------------------------------------------------------------------------
// Opens a NON-BLOCKING (modeless) window that gathers the pack's project
// checks in one place. Each check gets a card that says what it's for and
// what to select first, and a Run button. The window stays open while you
// work, so you can change the selection in Studio and run another check.
//
//   Audit  — checks the project and reports problems.
//   Detect — finds or measures something and reports it.
//
// The checks themselves live in the "Audit & Detect" folder next to this
// file. Each one exports { title, version, selection, purpose, execute } instead of registering its own menu item; Run calls execute(),
// which shows the check's own options and results windows as before.
// Every card has a status line that follows the browser selection live:
// green "Ready:" with what Run will scan, or orange "Not ready:" with what
// to select first. Text has fixed sizes, so resizing the window can't
// reflow or clip it.
//
// Read-only: neither this window nor any of the checks changes the project.
//
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// Part of the DAT-audio Utilities pack — loaded by "DAT-audio Utilities.js"
// in the Scripts directory above this folder.
// Menu: Scripts > DAT-audio Utilities > Audit & Detect...
//
// ADDING A CHECK
//   1. Put the .js in the "Audit & Detect" folder, named after its title,
//      which starts with "Audit" or "Detect".
//   2. Instead of studio.menu.addMenuItem, set module.exports to
//      { title, version, selection, purpose, execute }, where selection is
//      "project", "events" or "assets" (optionally isEnabled). The status
//      line is written from selection; keep purpose to two lines.
//   3. Add the filename to ADT_CHECKS below (alphabetical; Audit before Detect).
// ---------------------------------------------------------------------------

"use strict";

// Captured while the file loads (__dirname is only valid then).
var ADT_DIR = (typeof __dirname === "string") ? __dirname : "";

var ADT_FOLDER = "Audit & Detect/";

var ADT_CHECKS = [
    "Audit Events Routed Directly to Master.js",
    "Audit Events vs Bank Assignments.js",
    "Audit Instrument Probability.js",
    "Audit Looping Events for Long Asset presence.js",
    "Detect Duplicate Event Names.js",
    "Detect Loudness of Selected Assets.js"
];

// Load every check now, while __dirname is valid. A check that fails to load
// still gets a card, with the error in place of its Run button.
var adtTools = ADT_CHECKS.map(function (file) {
    var tool = null, error = "";
    try {
        tool = studio.system.require(ADT_DIR + "/" + ADT_FOLDER + file);
        if (!tool || typeof tool.execute !== "function") {
            tool = null;
            error = "it has no execute() export";
        }
    } catch (e) {
        error = "" + e;
    }
    if (error) {
        studio.system.error("[DAT-audio Utilities] Audit & Detect couldn't load \"" +
                            ADT_FOLDER + file + "\": " + error);
    }
    return {
        file:  file,
        tool:  tool,
        error: error,
        title: tool ? "" + tool.title : file.replace(/\.js$/, "")
    };
});

function adtEscape(s) {
    return ("" + s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function adtPlural(n, word) {
    return n + " " + word + (n === 1 ? "" : "s");
}

// What's selected in the browser, counted by kind.
function adtSelection() {
    var c = { events: 0, eventFolders: 0, assets: 0, other: 0 };
    var sel = [];
    try { sel = studio.window.browserSelection() || []; } catch (e) {}
    for (var i = 0; i < sel.length; i++) {
        var item = sel[i];
        if (adtIs(item, "isOfExactType", "Event"))            c.events++;
        else if (adtIs(item, "isOfExactType", "EventFolder")) c.eventFolders++;
        else if (adtIs(item, "isOfType", "Asset"))            c.assets++;
        else                                                  c.other++;
    }
    return c;
}

function adtIs(item, fn, type) {
    try { return item[fn](type); } catch (e) { return false; }
}

// The status line under every card: whether the check can run now, and what
// it will look at. Green "Ready:" or orange "Not ready:", kept to one line.
// A choice the user could make goes after it in blue as "Optional:", so it
// stands out from a column of green.
function adtLiveText(kind, c) {
    var OK = "<span style=\"color:#9ccc65;\">Ready: ", NO = "<span style=\"color:#ffb74d;\">Not ready: ";
    var OPT = "&nbsp;&nbsp;&nbsp;<span style=\"color:#64b5f6;\">Optional: ";
    if (kind === "events") {
        var parts = [];
        if (c.events)       parts.push(adtPlural(c.events, "event"));
        if (c.eventFolders) parts.push(adtPlural(c.eventFolders, "folder"));
        return parts.length
            ? OK + parts.join(", ") + " selected.</span>" + OPT + "its options can scan just those.</span>"
            : OK + "scans the whole project.</span>" + OPT + "select events or folders to scan only those.</span>";
    }
    if (kind === "assets") {
        if (c.assets) return OK + adtPlural(c.assets, "asset") + " selected.</span>";
        if (c.other)  return OK + adtPlural(c.other, "item") + " selected. Folders are searched for assets.</span>";
        return NO + "select assets or folders in the Assets browser first.</span>";
    }
    return OK + "scans the whole project. No selection needed.</span>";
}

// Shown in the window title; see the pack's top-level README.
var ADT_VERSION = "1.0.0";

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Audit && Detect...",

    isEnabled: function () { return studio.project !== null; },

    execute: function () {
        var W = studio.ui.widgetType;
        var G = studio.ui.layoutType;
        var GREY = "<span style=\"color:#9a9a9a;\">";
        var state = { cache: {}, timerId: null };

        function setOnce(w, id, what, value) {
            var key = id + "|" + what;
            if (state.cache[key] === value) return;
            state.cache[key] = value;
            var x = null;
            try { x = w.findWidget(id); } catch (e) {}
            if (!x) return;
            try {
                if (what === "text") x.setText(value);
                else x.setEnabled(value);
            } catch (e) {}
        }

        function refresh(w) {
            var c = adtSelection();
            adtTools.forEach(function (t, i) {
                if (!t.tool) return;
                setOnce(w, "adt_live" + i, "text", adtLiveText(t.tool.selection, c));
                if (typeof t.tool.isEnabled === "function") {
                    var on = true;
                    try { on = !!t.tool.isEnabled(); } catch (e) {}
                    setOnce(w, "adt_run" + i, "enabled", on);
                }
            });
        }

        function run(w, i) {
            var t = adtTools[i];
            var status = null;
            try { status = w.findWidget("adt_status"); } catch (e) {}
            function say(html) { try { if (status) status.setText(html); } catch (e) {} }
            say(GREY + "Running " + adtEscape(t.title) + "…</span>");
            try {
                t.tool.execute();
                say(GREY + "Finished: " + adtEscape(t.title) + "</span>");
            } catch (e) {
                studio.system.error("[Audit & Detect] " + t.title + " failed: " + e);
                say("<span style=\"color:#ffb74d;\">" + adtEscape(t.title) + " failed: " +
                    adtEscape(e) + "</span>");
            }
            refresh(w);
        }

        // Everything sits in one fixed-width column, anchored top-left, so FMOD
        // can't stretch, clip or spread it out when the window is resized:
        // - a word-wrapped label gets about one line of height unless its
        //   height is fixed, and spare height is spread as gaps between rows;
        // - maximumWidth/maximumHeight only hold with a Fixed size policy, so
        //   every label and row bar here gets one.
        // - nested layouts get a default inner margin, which pushes a card's
        //   Run button past the bars' right edge, so theirs are set to zero;
        // - a Spacer only soaks up width, so an empty expanding Label at the
        //   bottom takes the spare height.
        // Extra window space collects as blank space right and below. The title
        // and status are one line each, the purpose exactly two (keep purposes
        // to two lines).
        var TEXT_W = 620, RUN_W = 90, GAP = 12, LINE_H = 18;
        var ROW_W = TEXT_W + GAP + RUN_W;
        var fixed = { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed };
        var NO_MARGIN = { left: 0, right: 0, top: 0, bottom: 0 };
        var initial = adtSelection();

        // Fixed width and height for a widget description.
        function sized(desc, w, h) {
            desc.minimumWidth = desc.maximumWidth = w;
            desc.minimumHeight = desc.maximumHeight = h;
            desc.sizePolicy = fixed;
            return desc;
        }

        // A section heading, styled like Copy-Paste's zones.
        function zone(title, sub) {
            return sized({ widgetType: W.Label, wordWrap: false,
                           text: "<table width=\"100%\" cellpadding=\"3\" bgcolor=\"#2a2a2a\"><tr><td>" +
                                 "<span style=\"color:#ffd84d;\"><b>" + title + "</b></span>&nbsp;&nbsp;" +
                                 GREY + sub + "</span></td></tr></table>" },
                         ROW_W, LINE_H + 6);
        }

        // One check: title, version and purpose, the status line under them, and
        // Run on the right.
        function card(i) {
            var t = adtTools[i], tool = t.tool;
            var title = "<b>" + adtEscape(t.title) + "</b>" +
                        (tool ? "&nbsp;&nbsp;" + GREY + "v" + adtEscape(tool.version) + "</span>" : "");
            var purpose = tool ? adtEscape(tool.purpose)
                               : "<span style=\"color:#ffb74d;\">Couldn't load " + adtEscape(t.file) +
                                 ": " + adtEscape(t.error) + "</span>";
            var left = [
                sized({ widgetType: W.Label, text: title, wordWrap: false }, TEXT_W, LINE_H),
                sized({ widgetType: W.Label, text: purpose, wordWrap: true }, TEXT_W, 2 * LINE_H)
            ];
            if (tool) {
                left.push(sized({ widgetType: W.Label, widgetId: "adt_live" + i, wordWrap: false,
                                  text: adtLiveText(tool.selection, initial) }, TEXT_W, LINE_H));
            }
            return { widgetType: W.Layout, layout: G.HBoxLayout, spacing: GAP, contentsMargins: NO_MARGIN, items: [
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 2, contentsMargins: NO_MARGIN, items: left },
                { widgetType: W.PushButton, widgetId: "adt_run" + i, text: "Run",
                  isEnabled: !!tool, minimumWidth: RUN_W, maximumWidth: RUN_W, sizePolicy: fixed,
                  onClicked: function () { run(this, i); } }
            ] };
        }

        var column = [
            sized({ widgetType: W.Label, wordWrap: true,
                    text: "Checks that read the project and report what they find. None of them " +
                          "change the project. Each opens its results in its own window and also " +
                          "writes them to <b>Scripts → Show Script Output</b>. This window stays " +
                          "open, so you can select something in Studio and run another." },
                  ROW_W, 3 * LINE_H)
        ];
        [["Audit", "checks the project and reports problems"],
         ["Detect", "finds or measures something and reports it"]].forEach(function (s) {
            var cards = [];
            adtTools.forEach(function (t, i) {
                if (t.title.indexOf(s[0] + " ") === 0) cards.push(card(i));
            });
            if (!cards.length) return;
            column.push(zone(s[0], s[1]));
            column = column.concat(cards);
        });
        column.push({ widgetType: W.Layout, layout: G.HBoxLayout, spacing: GAP, contentsMargins: NO_MARGIN, items: [
            sized({ widgetType: W.Label, widgetId: "adt_status", text: "", wordWrap: false }, TEXT_W, LINE_H),
            { widgetType: W.PushButton, text: "Close",
              minimumWidth: RUN_W, maximumWidth: RUN_W, sizePolicy: fixed,
              onClicked: function () { this.closeDialog(); } }
        ] });
        // Spare height goes below everything.
        column.push({ widgetType: W.Label, text: "",
                      sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed,
                                    verticalPolicy:   studio.ui.sizePolicy.MinimumExpanding } });

        studio.ui.showModelessDialog({
            windowTitle:  "Audit & Detect  v" + ADT_VERSION,
            windowWidth:  ROW_W + 28,
            windowHeight: 720,
            widgetType:   W.Layout,
            layout:       G.HBoxLayout,
            spacing:      0,
            contentsMargins: { left: 14, right: 14, top: 14, bottom: 14 },
            items: [
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 12, contentsMargins: NO_MARGIN, items: column },
                { widgetType: W.Spacer }   // spare width goes to the right
            ],
            onConstructed: function () {
                try { refresh(this); } catch (e) {}
                try { state.timerId = this.startTimer(500); } catch (e) {}
            },
            onTimerEvent: function () {
                try { refresh(this); } catch (e) {}
            },
            onClose: function () {
                try { if (state.timerId !== null) this.stopTimer(state.timerId); } catch (e) {}
            }
        });
    }
});
