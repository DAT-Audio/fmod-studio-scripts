// ============================================================
//  FMOD Event Recorder
//  Version: 0.6.5
//  Select events in the Event Browser, then run this script.
//
//  Part of the DAT-audio Utilities pack — loaded by "DAT-audio Utilities.js"
//  in the Scripts directory above this folder.
//  Menu: Scripts > DAT-audio Utilities > Record Selected Events...
//
//  LAYOUT — the renderer lives in a Tools/ folder NEXT TO the Scripts/ folder
//  this script is installed in, so the same files work installed per machine
//  or per project:
//
//    <root>/                         per machine, macOS 2.02.x:
//      Scripts/                        ~/Library/Application Support/FMOD Studio/
//        DAT-audio Utilities.js      per machine, macOS 2.03.x:
//        DAT-audio Utilities/          ~/Library/Preferences/FMOD Studio/
//          Event Recorder.js  ← this  per machine, Windows: %LOCALAPPDATA%\FMOD Studio\
//      Tools/                        per project: the folder with the .fspro
//        FMOD_Event_Recorder/
//          fmod_recorder.py   ← Python renderer
//          setup.command / setup.bat, requirements.txt, libs/, venv/
//
//  SETUP (once per machine, or once per project install): run
//  Tools/FMOD_Event_Recorder/setup.command (macOS) or setup.bat (Windows).
//  Requires Python 3.9 or newer. See Tools/FMOD_Event_Recorder/README.md.
//
//  PARAMETERS:
//    Events that expose game parameters (labeled or continuous, local or
//    global) get a "Configure…" button. Labeled parameters can be multi-
//    selected — one file is rendered per selected label (cross product across
//    labeled params, multiplied by the take count). Continuous parameters take
//    a single value applied to every take. Effect-chain knobs are not exposed
//    (they are not game parameters).
// ============================================================

// The folder this file is being loaded from (documented global; only valid
// while the file loads, so it's captured here, not inside execute).
var ER_SCRIPT_DIR = (typeof __dirname === "string") ? __dirname : "";

// Tools/FMOD_Event_Recorder/ next to the nearest enclosing "Scripts" folder —
// works for Scripts/Event Recorder.js and Scripts/DAT-audio Utilities/…, per
// machine or per project. Falls back to <project>/Tools/ (the original
// per-project layout) if the script's location is unknown. String ops only:
// the scripting engine rejects some regex literals.
function erToolsDir(projectDir) {
    var p = ER_SCRIPT_DIR.split("\\").join("/");
    while (p.length) {
        var cut = p.lastIndexOf("/");
        if (p.substring(cut + 1).toLowerCase() === "scripts") {
            return p.substring(0, cut + 1) + "Tools/FMOD_Event_Recorder/";
        }
        if (cut <= 0) break;
        p = p.substring(0, cut);
    }
    return projectDir + "Tools/FMOD_Event_Recorder/";
}

// Plugins folder of the FMOD Studio app that is running this script, from the
// documented studio.application.filePath (the executable). macOS: the exe is in
// <bundle>/Contents/MacOS/, plugins in <bundle>/Contents/Plugins/. Windows: a
// "plugins" folder next to the exe. Lets the renderer load the exact plugins of
// whichever Studio version/install the user has, wherever it's installed.
function erStudioPluginDir() {
    var exe = "";
    try { exe = "" + studio.application.filePath; } catch (e) { exe = ""; }
    exe = exe.split("\\").join("/");
    var cut = exe.lastIndexOf("/");
    if (cut <= 0) return null;
    var dir = exe.substring(0, cut);
    if (dir.length > 6 && dir.substring(dir.length - 6) === "/MacOS") {
        return dir.substring(0, dir.length - 6) + "/Plugins";
    }
    return dir + "/plugins";
}

// ── Built-bank paths ──────────────────────────────────────────
// FMOD stores the built-banks output directory relative to the project folder
// (e.g. "../../game/sound"), or as an absolute path. On Windows that may be
// "C:\\…" or "\\\\server\\…". String ops only (the engine rejects some regex
// literals).

// True for "/x", "//server/x", "\\server\x" and "C:/x" / "C:\\x".
function erIsAbsolute(p) {
    p = ("" + p).split("\\").join("/");
    return p.charAt(0) === "/" || (p.length > 1 && p.charAt(1) === ":");
}

// "/" separators, "." and ".." resolved, no trailing "/". Keeps a leading "/",
// a "//" UNC prefix or a "C:" drive.
function erNormalizePath(p) {
    p = ("" + p).split("\\").join("/");
    var prefix = "";
    if (p.substring(0, 2) === "//") { prefix = "//"; p = p.substring(2); }
    else if (p.length > 1 && p.charAt(1) === ":") { prefix = p.substring(0, 2) + "/"; p = p.substring(2); }
    else if (p.charAt(0) === "/") { prefix = "/"; }
    var parts = p.split("/"), out = [];
    for (var i = 0; i < parts.length; i++) {
        var seg = parts[i];
        if (seg === "" || seg === ".") continue;
        if (seg === "..") {
            if (out.length && out[out.length - 1] !== "..") out.pop();
            else if (!prefix) out.push("..");
            continue;
        }
        out.push(seg);
    }
    return prefix + out.join("/");
}

// A bank's folder path in the Banks browser ("sound/banks"), used when the
// project builds with "mirror browser folder hierarchy" switched on. Stops at
// the master bank folder, which is not part of the built path.
function erBankFolderPath(bank) {
    var parts = [], f = null, guard = 0;
    try { f = bank.folder; } catch (e) { f = null; }
    while (f && guard++ < 32) {
        var isFolder = false;
        try { isFolder = f.isOfExactType("BankFolder"); } catch (e) { isFolder = false; }
        if (!isFolder) break;
        parts.unshift("" + f.name);
        try { f = f.folder; } catch (e) { f = null; }
    }
    return parts.join("/");
}

function repeatStr(str, n) {
    var out = "";
    for (var i = 0; i < n; i++) out += str;
    return out;
}

// event:/Folder/Name  →  Folder/Name  (drops the "event:/" prefix for display)
function shortName(path) {
    return path.replace("event:/", "");
}

// True if a file exists (readable). Used to detect the OS from the venv layout.
function fileExists(path) {
    try {
        var f = studio.system.getFile(path);
        f.open(studio.system.openMode.ReadOnly);
        f.close();
        return true;
    } catch (e) { return false; }
}

// True if a file or folder exists. Uses File.exists(), which (unlike
// fileExists' getFile/open) does report a missing path on macOS. A broken
// symlink, such as a venv python whose base install was removed, counts as missing.
function erExists(path) {
    try { return !!studio.system.getFile(path).exists(); } catch (e) { return false; }
}

// Packages fmod_recorder.py can't run without, as they appear in site-packages.
// pyloudnorm is optional (normalization only), so it isn't checked.
var ER_REQUIRED_PACKAGES = ["pyfmodex", "numpy", "soundfile.py"];

// Checks that setup.command / setup.bat has been run in toolsUrl: the venv's
// python exists, and its site-packages holds the required packages. Returns
// null when ready, or the text to show. The FMOD libraries aren't checked:
// they have several valid locations, and the renderer reports a missing one.
function erSetupProblem(toolsUrl, isWindows) {
    var setupName = isWindows ? "setup.bat" : "setup.command";
    var where = isWindows ? toolsUrl.replace(/\//g, "\\") : toolsUrl;
    var howTo = "Run " + setupName + " in this folder, then try again:\n\n" + where + "\n\n" +
                "It needs Python 3.9 or newer. See README.md in the same folder.";

    var python = toolsUrl + (isWindows ? "venv/Scripts/python.exe" : "venv/bin/python");
    if (!erExists(python)) {
        if (erExists(toolsUrl + "venv/pyvenv.cfg")) {
            return "Event Recorder's Python environment is broken: its python is missing. " +
                   "This usually means the Python it was made with was removed or upgraded.\n\n" +
                   howTo + " Setup rebuilds the environment.";
        }
        return "Event Recorder's one-time setup hasn't been run for this install " +
               "(no Python environment was found).\n\n" + howTo + " This is needed once per install.";
    }

    // site-packages: venv/Lib/site-packages on Windows, venv/lib/pythonX.Y/
    // site-packages on macOS. If the folder can't be listed, don't block.
    var sitePackages = [];
    if (isWindows) {
        sitePackages.push(toolsUrl + "venv/Lib/site-packages/");
    } else {
        var libDirs = [];
        try {
            var f = studio.system.readDirFilters;
            libDirs = studio.system.readDir(toolsUrl + "venv/lib", f.Dirs | f.NoDotAndDotDot) || [];
        } catch (e) { libDirs = []; }
        for (var i = 0; i < libDirs.length; i++) {
            var d = "" + libDirs[i];
            if (d.indexOf("python") === 0) sitePackages.push(toolsUrl + "venv/lib/" + d + "/site-packages/");
        }
    }
    if (!sitePackages.length) return null;

    var missing = [];
    for (var k = 0; k < ER_REQUIRED_PACKAGES.length; k++) {
        var found = false;
        for (var j = 0; j < sitePackages.length && !found; j++) {
            found = erExists(sitePackages[j] + ER_REQUIRED_PACKAGES[k]);
        }
        if (!found) missing.push(ER_REQUIRED_PACKAGES[k].replace(".py", ""));
    }
    if (missing.length) {
        return "Event Recorder's setup didn't finish: its Python environment is missing " +
               missing.join(", ") + ".\n\n" + howTo + " Check that it finishes without errors.";
    }
    return null;
}

// Detect an event's game parameters, classified into continuous vs labeled.
// Returns { continuous:[...], labeled:[...], total:N, skipped:N }.
// Each continuous: { name, isGlobal, min, max, value, automated }
// Each labeled:    { name, isGlobal, labels:[...], selected:[...], automated }
function detectParams(ev) {
    var result = { continuous: [], labeled: [], total: 0, skipped: 0 };
    var params;
    try { params = ev.getParameterPresets(); } catch (e) { return result; }
    if (!params) return result;

    for (var i = 0; i < params.length; i++) {
        var pr = params[i];

        var name = null;
        try { if (pr.presetOwner && pr.presetOwner.name) name = pr.presetOwner.name; } catch (e) {}
        if (!name) { result.skipped++; continue; }

        var readOnly = false;
        try { readOnly = !!pr.isReadOnly; } catch (e) {}
        if (readOnly) { result.skipped++; continue; }   // automatic/built-in params can't be set meaningfully

        var isGlobal = false;
        try { isGlobal = !!pr.isGlobal; } catch (e) {}

        var automated = false;
        try { automated = !!(pr.automationCurves && pr.automationCurves.length > 0); } catch (e) {}

        var labels = null;
        try { labels = pr.enumerationLabels; } catch (e) {}

        if (labels && labels.length) {
            var labArr = [];
            for (var j = 0; j < labels.length; j++) labArr.push(labels[j]);
            result.labeled.push({
                name: name, isGlobal: isGlobal, labels: labArr, selected: [], automated: automated
            });
        } else {
            var mn = 0, mx = 1, iv = 0;
            try { mn = pr.minimum; }      catch (e) {}
            try { mx = pr.maximum; }      catch (e) {}
            try { iv = pr.initialValue; } catch (e) {}
            result.continuous.push({
                name: name, isGlobal: isGlobal, min: mn, max: mx, value: iv, automated: automated
            });
        }
        result.total++;
    }
    return result;
}

// Count sustain points on an event's timeline. Each one holds playback until a
// (game-side) key-off; the recorder issues the key-off itself and splits the
// render into one segment per key-off. Returns the number of sustain points.
function detectSustainPoints(ev) {
    try {
        var tl = ev.timeline;
        if (!tl || !tl.markers) return 0;
        var n = 0;
        for (var i = 0; i < tl.markers.length; i++) {
            if (tl.markers[i].entity === "SustainPoint") n++;
        }
        return n;
    } catch (e) { return 0; }
}

// Shown in the window title / console output; see the pack's top-level README.
var ER_VERSION = "0.6.5";

studio.menu.addMenuItem({
    name:      "DAT-audio Utilities\\Record Selected Events...",
    execute:   function () {

        // ── Default output path ──────────────────────────────
        // Place a "Recordings" folder next to the .fspro project file
        var projectPath = studio.project.filePath;
        var projectDir  = projectPath.replace(/[^\/\\]+$/, "");   // strip filename (handles / and \)

        var toolsUrl  = erToolsDir(projectDir);
        // Detect the OS from the project path: a Windows drive-letter root
        // (second char is ":", e.g. "D:/…") vs a Unix "/" root. This is reliable
        // — probing for a file with fileExists() is NOT, because
        // studio.system.getFile/open does not fault on a missing path on macOS
        // (so it would wrongly report Windows). Plain string checks only, so the
        // Studio scripting engine parses it (it rejects some regex literals).
        var _drive0    = projectDir.charAt(0);
        var _isLetter  = (_drive0 >= "A" && _drive0 <= "Z") || (_drive0 >= "a" && _drive0 <= "z");
        var isWindows  = _isLetter && projectDir.charAt(1) === ":";
        var defaultOutputPath = projectDir + "Recordings";

        // ── Check setup before anything else ───────────────────
        // Without the venv the renderer can't start, and all that would show
        // after configuring a render is a generic "did not complete" alert.
        var setupProblem = erSetupProblem(toolsUrl, isWindows);
        if (setupProblem) {
            alert(setupProblem);
            return;
        }

        // ── Read selection ───────────────────────────────────
        var sel = studio.window.browserSelection();

        var events = [];
        for (var i = 0; i < sel.length; i++) {
            if (sel[i].entity === "Event") events.push(sel[i]);
        }

        if (events.length === 0) {
            alert("No events selected.\n\nPlease select one or more events in the Event Browser, then run this script.");
            return;
        }

        // ── Detect parameters for every selected event ────────
        // paramState[eventPath] = { continuous:[...], labeled:[...], total, skipped }
        // This object is the single source of truth for the GUI; the Configure
        // dialogs mutate it in place, and Record reads it to build the manifest.
        var paramState = {};
        var eventGuids = {};    // eventPath -> GUID string, for robust lookup (no strings bank needed)
        var sustainCounts = {}; // eventPath -> number of sustain points on the timeline
        var eventPaths = [];
        for (var e = 0; e < events.length; e++) {
            var ep = events[e].getPath();
            eventPaths.push(ep);
            paramState[ep] = detectParams(events[e]);
            try { eventGuids[ep] = events[e].id; } catch (gerr) { eventGuids[ep] = null; }
            sustainCounts[ep] = detectSustainPoints(events[e]);
        }

        // ── Build event list text ────────────────────────────
        var listLines = "";
        for (var e2 = 0; e2 < events.length; e2++) {
            var p = eventPaths[e2];
            var det = paramState[p];
            var tag = det.total > 0 ? "   [" + det.total + " param" + (det.total > 1 ? "s" : "") + "]" : "";
            var sp  = sustainCounts[p] || 0;
            if (sp > 0) tag += "   [" + sp + " sustain pt" + (sp > 1 ? "s" : "") + " → " + (sp + 1) + " parts]";
            listLines += (e2 + 1) + ".  " + p + tag + "\n";
        }

        // ── Per-event parameter configuration dialog ──────────
        // Opened by the "Configure…" button for one event. Builds compact rows
        // (no ScrollArea/GroupBox exist, so we keep each dialog bounded and use
        // multi-column HBox rows). Writes chosen values back into paramState.
        function openParamDialog(eventPath) {
            var det = paramState[eventPath];
            var items = [];

            items.push({ widgetType: studio.ui.widgetType.Label, text: "<b>Parameters — " + shortName(eventPath) + "</b>" });
            items.push({ widgetType: studio.ui.widgetType.Label, text: "<hr/>" });

            // ── Labeled parameters (multi-select checkboxes) ──
            if (det.labeled.length > 0) {
                items.push({ widgetType: studio.ui.widgetType.Label, text: "<b>Labeled</b>  (tick labels — one file rendered per ticked label)" });
                for (var li = 0; li < det.labeled.length; li++) {
                    var lp = det.labeled[li];
                    var rowItems = [{
                        widgetType: studio.ui.widgetType.Label,
                        text: (lp.automated ? "⚙ " : "") + lp.name + (lp.isGlobal ? " (global)" : "") + ":",
                        minimumWidth: 200
                    }];
                    for (var lj = 0; lj < lp.labels.length; lj++) {
                        rowItems.push({
                            widgetType: studio.ui.widgetType.CheckBox,
                            widgetId:   "lab_" + li + "_" + lj,
                            text:       lp.labels[lj],
                            checked:    lp.selected.indexOf(lp.labels[lj]) !== -1
                        });
                    }
                    items.push({
                        widgetType: studio.ui.widgetType.Layout,
                        layout:     studio.ui.layoutType.HBoxLayout,
                        spacing:    10,
                        items:      rowItems
                    });
                }
                items.push({ widgetType: studio.ui.widgetType.Label, text: "<hr/>" });
            }

            // ── Continuous parameters (numeric value, 2 per row) ──
            if (det.continuous.length > 0) {
                items.push({ widgetType: studio.ui.widgetType.Label, text: "<b>Continuous</b>  (value applied to every take)" });
                var PER_ROW = 2;
                for (var ci = 0; ci < det.continuous.length; ci += PER_ROW) {
                    var cells = [];
                    for (var k = 0; k < PER_ROW && (ci + k) < det.continuous.length; k++) {
                        var idx = ci + k;
                        var cp  = det.continuous[idx];
                        cells.push({
                            widgetType: studio.ui.widgetType.Layout,
                            layout:     studio.ui.layoutType.HBoxLayout,
                            spacing:    6,
                            items: [
                                {
                                    widgetType: studio.ui.widgetType.Label,
                                    text: (cp.automated ? "⚙ " : "") + cp.name + (cp.isGlobal ? " *" : ""),
                                    minimumWidth: 190
                                },
                                {
                                    widgetType:   studio.ui.widgetType.LineEdit,
                                    widgetId:     "cont_" + idx,
                                    text:         String(cp.value),
                                    minimumWidth: 70,
                                    sizePolicy: {
                                        horizontalPolicy: studio.ui.sizePolicy.Fixed,
                                        verticalPolicy:   studio.ui.sizePolicy.Fixed
                                    }
                                },
                                {
                                    widgetType: studio.ui.widgetType.Label,
                                    text: "[" + cp.min + "–" + cp.max + "]"
                                }
                            ]
                        });
                    }
                    items.push({
                        widgetType: studio.ui.widgetType.Layout,
                        layout:     studio.ui.layoutType.HBoxLayout,
                        spacing:    18,
                        items:      cells
                    });
                }
                items.push({ widgetType: studio.ui.widgetType.Label, text: "<i>⚙ = drives automation&nbsp;&nbsp;·&nbsp;&nbsp;* = global parameter</i>" });
            }

            // ── OK / Cancel ──
            items.push({ widgetType: studio.ui.widgetType.Label, text: "<hr/>" });
            items.push({
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    {
                        widgetType: studio.ui.widgetType.PushButton,
                        text:       "OK",
                        onClicked:  function () {
                            // Read labeled checkboxes
                            for (var a = 0; a < det.labeled.length; a++) {
                                var lp2 = det.labeled[a];
                                var chosen = [];
                                for (var b = 0; b < lp2.labels.length; b++) {
                                    var w = this.findWidget("lab_" + a + "_" + b);
                                    if (w && w.isChecked()) chosen.push(lp2.labels[b]);
                                }
                                lp2.selected = chosen;
                            }
                            // Read continuous values
                            for (var c = 0; c < det.continuous.length; c++) {
                                var cw = this.findWidget("cont_" + c);
                                if (cw) {
                                    var v = parseFloat(cw.text());
                                    if (!isNaN(v)) det.continuous[c].value = v;
                                }
                            }
                            this.closeDialog();
                        }
                    },
                    {
                        widgetType: studio.ui.widgetType.PushButton,
                        text:       "Cancel",
                        onClicked:  function () { this.closeDialog(); }
                    }
                ]
            });

            var dlgHeight = Math.min(760, 200 + det.labeled.length * 34 + Math.ceil(det.continuous.length / 2) * 34);
            try {
                studio.ui.showModalDialog({
                    windowTitle: "Configure Parameters — " + shortName(eventPath),
                    windowWidth:  620,
                    windowHeight: dlgHeight,
                    widgetType: studio.ui.widgetType.Layout,
                    layout:     studio.ui.layoutType.VBoxLayout,
                    spacing:    6,
                    contentsMargins: { left: 14, right: 14, top: 14, bottom: 14 },
                    items: items
                });
            } catch (err) {
                alert("Could not open the parameter dialog:\n" + err +
                      "\n\n(Please report this — the nested-dialog approach may need a fallback.)");
            }
        }

        // Opened by the single "Configure Parameters…" button. Lists every event
        // that exposes parameters in a ComboBox (fixed height, so any number of
        // events fits) and hands the chosen one to openParamDialog. This replaces
        // one Configure row per event, which pushed the Record button off-screen
        // for large selections (FMOD dialogs have no scroll area).
        function openParamPicker() {
            var comboItems = [];
            for (var ci = 0; ci < paramEvents.length; ci++) {
                var dd = paramState[paramEvents[ci]];
                comboItems.push({ text: shortName(paramEvents[ci]) +
                    "  (" + dd.continuous.length + " cont, " + dd.labeled.length + " lab)" });
            }
            studio.ui.showModalDialog({
                windowTitle:  "Configure Parameters",
                windowWidth:  640,
                windowHeight: 190,
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.VBoxLayout,
                spacing:    10,
                contentsMargins: { left: 16, right: 16, top: 16, bottom: 16 },
                items: [
                    { widgetType: studio.ui.widgetType.Label,
                      text: "<b>" + paramEvents.length + " event" + (paramEvents.length > 1 ? "s" : "") +
                            " expose parameters.</b><br/>Pick one and click Configure. Settings are kept while this window stays open." },
                    { widgetId: "paramPick", widgetType: studio.ui.widgetType.ComboBox,
                      items: comboItems, currentIndex: 0, minimumWidth: 600 },
                    {
                        widgetType: studio.ui.widgetType.Layout,
                        layout:     studio.ui.layoutType.HBoxLayout,
                        spacing:    8,
                        items: [
                            { widgetType: studio.ui.widgetType.PushButton, text: "Configure Selected Event…",
                              onClicked: function () {
                                  var pick = this.findWidget("paramPick");
                                  var idx  = pick ? pick.currentIndex() : -1;   // currentIndex() is a method
                                  if (idx >= 0 && idx < paramEvents.length) openParamDialog(paramEvents[idx]);
                              } },
                            { widgetType: studio.ui.widgetType.PushButton, text: "Close",
                              onClicked: function () { this.closeDialog(); } }
                        ]
                    }
                ]
            });
        }

        // ── Build the Parameters section of the main dialog ───
        // Collect the events that expose parameters. They're reached through a
        // single "Configure Parameters…" button + picker (openParamPicker) rather
        // than one row each, so the dialog height stays bounded no matter how many
        // events are selected.
        var paramEvents = [];
        for (var pe = 0; pe < eventPaths.length; pe++) {
            if (paramState[eventPaths[pe]].total > 0) paramEvents.push(eventPaths[pe]);
        }
        var nWithParams = paramEvents.length;
        var nWithout    = eventPaths.length - nWithParams;

        var paramHeader;
        if (nWithParams === 0) {
            paramHeader = "<hr/><b>Event Parameters</b><br/><i>None of the selected events expose game parameters.</i>";
        } else {
            paramHeader = "<hr/><b>Event Parameters</b>  <i>(" + nWithParams + " event" + (nWithParams > 1 ? "s" : "") +
                          " with parameters" + (nWithout > 0 ? ", " + nWithout + " without" : "") + ")</i>";
        }

        // ── Assemble main dialog items ────────────────────────
        var mainItems = [
            { widgetType: studio.ui.widgetType.Label, text: "<b>FMOD Event Recorder</b>" },
            { widgetType: studio.ui.widgetType.Label, text: "<hr/>" },
            { widgetType: studio.ui.widgetType.Label, text: "<b>Selected Events (" + events.length + ")</b>" },
            {
                widgetType:   studio.ui.widgetType.TextEdit,
                widgetId:     "eventList",
                text:         listLines,
                minimumWidth: 460,
                minimumHeight: 110,
                sizePolicy: {
                    horizontalPolicy: studio.ui.sizePolicy.Expanding,
                    verticalPolicy:   studio.ui.sizePolicy.Fixed
                }
            },

            { widgetType: studio.ui.widgetType.Label, text: "<hr/><b>Output Configuration</b>" },
            { widgetType: studio.ui.widgetType.Label, text: "Output Folder:" },
            {
                widgetId:     "outputPath",
                widgetType:   studio.ui.widgetType.LineEdit,
                text:         defaultOutputPath,
                minimumWidth: 460,
                sizePolicy: {
                    horizontalPolicy: studio.ui.sizePolicy.MinimumExpanding,
                    verticalPolicy:   studio.ui.sizePolicy.Fixed
                }
            },
            {
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    { widgetType: studio.ui.widgetType.Label, text: "Format:" },
                    { widgetId: "format", widgetType: studio.ui.widgetType.ComboBox, items: [ { text: "WAV" }, { text: "AIFF" }, { text: "FLAC" }, { text: "MP3" } ], currentIndex: 0, minimumWidth: 110 },
                    { widgetType: studio.ui.widgetType.Label, text: "   Sample Rate:" },
                    { widgetId: "sampleRate", widgetType: studio.ui.widgetType.ComboBox, items: [ { text: "44100 Hz" }, { text: "48000 Hz" } ], currentIndex: 1, minimumWidth: 110 }
                ]
            },
            {
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    { widgetType: studio.ui.widgetType.Label, text: "Channels:" },
                    { widgetId: "channels", widgetType: studio.ui.widgetType.ComboBox, items: [ { text: "Stereo" }, { text: "Mono" } ], currentIndex: 0, minimumWidth: 110 },
                    { widgetType: studio.ui.widgetType.Label, text: "   Takes per event:" },
                    { widgetId: "numTakes", widgetType: studio.ui.widgetType.ComboBox, items: [ { text: "1" }, { text: "2" }, { text: "3" }, { text: "4" }, { text: "5" }, { text: "6" }, { text: "7" }, { text: "8" } ], currentIndex: 0, minimumWidth: 110 }
                ]
            },
            {
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    { widgetType: studio.ui.widgetType.Label, text: "Max duration (s):" },
                    {
                        widgetId:     "maxDuration",
                        widgetType:   studio.ui.widgetType.LineEdit,
                        text:         "30",
                        minimumWidth: 70,
                        sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed }
                    },
                    { widgetType: studio.ui.widgetType.Label, text: "  <i>Looping events are captured for this length; finite events stop at their natural end.</i>" }
                ]
            },
            {
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    { widgetType: studio.ui.widgetType.Label, text: "Sustain hold (s):" },
                    {
                        widgetId:     "sustainHold",
                        widgetType:   studio.ui.widgetType.LineEdit,
                        text:         "1",
                        minimumWidth: 70,
                        sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed }
                    },
                    { widgetType: studio.ui.widgetType.Label, text: "  <i>Events with sustain points are held this long at each point, then keyed off &amp; split into parts.</i>" }
                ]
            },

            // ── Normalization (off by default) ────────────────
            { widgetType: studio.ui.widgetType.Label, text: "<hr/><b>Normalization</b>  <i>(optional)</i>" },
            {
                widgetType: studio.ui.widgetType.Layout,
                layout:     studio.ui.layoutType.HBoxLayout,
                spacing:    8,
                items: [
                    { widgetId: "normEnabled", widgetType: studio.ui.widgetType.CheckBox, text: "Normalize output", checked: false },
                    { widgetType: studio.ui.widgetType.Label, text: "   LUFS-I target:" },
                    { widgetId: "normLufs", widgetType: studio.ui.widgetType.LineEdit, text: "-16",
                      minimumWidth: 60, sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed } },
                    { widgetType: studio.ui.widgetType.Label, text: "   Peak (dBFS):" },
                    { widgetId: "normPeak", widgetType: studio.ui.widgetType.LineEdit, text: "-1",
                      minimumWidth: 60, sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed } }
                ]
            },
            { widgetType: studio.ui.widgetType.Label,
              text: "<i>Applied only when ticked. LUFS-I = loudness target; Peak = dBFS ceiling.<br/>" +
                    "With both: match to the LUFS target, then reduce so peaks stay under the ceiling.<br/>" +
                    "Leave a field blank to disable just that one. Output is 16-bit.</i>" },

            { widgetType: studio.ui.widgetType.Label, text: paramHeader }
        ];

        // Append a single "Configure Parameters…" button (opens the picker) when
        // any selected event exposes parameters — one bounded row instead of N,
        // so the Record button below always stays on-screen.
        if (nWithParams > 0) {
            mainItems.push({
                widgetType: studio.ui.widgetType.PushButton,
                text:       "Configure Parameters…",
                minimumWidth: 200,
                onClicked:  function () { openParamPicker(); }
            });
        }

        // ── Record button ─────────────────────────────────────
        mainItems.push({ widgetType: studio.ui.widgetType.Label, text: "<hr/>" });
        mainItems.push({
            widgetId:   "btnRecord",
            widgetType: studio.ui.widgetType.PushButton,
            text:       "▶  Record " + events.length + " Event" + (events.length > 1 ? "s" : ""),
            onClicked:  function () {
                var formatList = ["WAV", "AIFF", "FLAC", "MP3"];

                var opWidget  = this.findWidget("outputPath");
                var fmtWidget = this.findWidget("format");
                var srWidget  = this.findWidget("sampleRate");
                var chWidget  = this.findWidget("channels");
                var tkWidget  = this.findWidget("numTakes");
                var mdWidget  = this.findWidget("maxDuration");

                var outputPath = opWidget ? opWidget.text() : defaultOutputPath;
                var format     = fmtWidget ? formatList[fmtWidget.currentIndex()] : "WAV";
                var sampleRate = srWidget  ? (srWidget.currentIndex() === 0 ? 44100 : 48000) : 48000;
                var channels   = chWidget  ? (chWidget.currentIndex() === 0 ? 2 : 1) : 2;
                var numTakes   = tkWidget  ? (tkWidget.currentIndex() + 1) : 1;
                var maxDuration = 30.0;
                if (mdWidget) {
                    var mdParsed = parseFloat(mdWidget.text());
                    if (!isNaN(mdParsed) && mdParsed > 0) maxDuration = mdParsed;
                }

                var shWidget = this.findWidget("sustainHold");
                var sustainHold = 1.0;
                if (shWidget) {
                    var shParsed = parseFloat(shWidget.text());
                    if (!isNaN(shParsed) && shParsed >= 0) sustainHold = shParsed;
                }

                // Normalization (off unless ticked; blank field = that control off)
                var normEnabledW = this.findWidget("normEnabled");
                var normLufsW    = this.findWidget("normLufs");
                var normPeakW    = this.findWidget("normPeak");
                var normEnabled  = normEnabledW ? normEnabledW.isChecked() : false;
                var normLufs = null, normPeak = null;
                if (normEnabled) {
                    if (normLufsW) { var lv = parseFloat(normLufsW.text()); if (!isNaN(lv)) normLufs = lv; }
                    if (normPeakW) { var pv = parseFloat(normPeakW.text()); if (!isNaN(pv)) normPeak = pv; }
                    if (normLufs === null && normPeak === null) normEnabled = false;  // nothing to do
                }

                if (!outputPath || outputPath === "") {
                    alert("No Output Folder selected.\n\nPlease select an output folder before recording.");
                    return;
                }

                // ── Resolve the built-banks output directory ──────
                // From the project's build settings, the way FMOD itself builds:
                //   <output dir> / <platform subdirectory> / [<bank folder path>/] <bank>.bank
                // The output dir is relative to the project folder unless absolute,
                // so banks built outside the project (e.g. into a game repo) are
                // found. The bank folder path applies when the project mirrors the
                // Banks browser's folder hierarchy.
                var buildBaseDir = projectDir + "Build/Desktop/";   // fallback
                var mirrorFolders = false;
                try {
                    var ws     = studio.project.workspace;
                    var outDir = "" + (ws.builtBanksOutputDirectory || "Build");
                    var absBase = erNormalizePath(erIsAbsolute(outDir) ? outDir : (projectDir + outDir)) + "/";
                    var subDir = "Desktop";
                    var plats  = ws.platforms;
                    if (plats && plats.length) {
                        var desktopSub = null, firstSub = null;
                        for (var pi = 0; pi < plats.length; pi++) {
                            // An empty subdirectory means "no platform subfolder".
                            var sd = plats[pi].subDirectory;
                            if (typeof sd !== "string") sd = "" + plats[pi].name;
                            if (pi === 0) firstSub = sd;
                            if (("" + plats[pi].name).toLowerCase() === "desktop") desktopSub = sd;
                        }
                        subDir = (desktopSub !== null) ? desktopSub : firstSub;   // prefer Desktop for offline render
                    }
                    var subNorm = erNormalizePath(subDir);
                    buildBaseDir = absBase + (subNorm ? subNorm + "/" : "");
                    mirrorFolders = !!ws.builtBanksMirrorBrowserFolderHierarchy;
                } catch (e) {
                    console.log("  (build path: falling back to Build/Desktop/ — " + e + ")");
                }
                console.log("  Build dir     : " + buildBaseDir);
                console.log("  Bank folders  : " + (mirrorFolders ? "mirrored from the Banks browser" : "flat"));

                // ── Collect bank paths ────────────────────────────
                var banks = studio.project.model.Bank.findInstances();
                var bankPaths = [];
                for (var b = 0; b < banks.length; b++) {
                    var rel  = mirrorFolders ? erBankFolderPath(banks[b]) : "";
                    var base = buildBaseDir + (rel ? rel + "/" : "") + banks[b].name;
                    bankPaths.push(base + ".bank");
                    bankPaths.push(base + ".strings.bank");
                }

                // ── Build per-event configs (with parameters) ─────
                var eventConfigs = [];
                for (var ec = 0; ec < eventPaths.length; ec++) {
                    var path = eventPaths[ec];
                    var det  = paramState[path];
                    var pms  = [];

                    for (var cc = 0; cc < det.continuous.length; cc++) {
                        var cp = det.continuous[cc];
                        pms.push({ name: cp.name, global: cp.isGlobal, labeled: false, value: cp.value });
                    }
                    for (var ll = 0; ll < det.labeled.length; ll++) {
                        var lp = det.labeled[ll];
                        if (lp.selected.length > 0) {
                            pms.push({ name: lp.name, global: lp.isGlobal, labeled: true, labels: lp.labels, selected: lp.selected });
                        }
                    }

                    eventConfigs.push({ path: path, guid: eventGuids[path] || null, takes: numTakes, params: pms, sustainPoints: (sustainCounts[path] || 0) });
                }

                // ── Build manifest ────────────────────────────────
                var manifest = {
                    outputPath:  outputPath,
                    format:      format,
                    sampleRate:  sampleRate,
                    channels:    channels,
                    maxDuration: maxDuration,
                    sustainHold: sustainHold,
                    numTakes:    numTakes,
                    // Project-local plugins, then the running Studio app's own plugins. The
                    // renderer adds standard install locations as a last-resort fallback.
                    pluginDirs:  [ projectDir + "Plugins" ].concat(erStudioPluginDir() ? [ erStudioPluginDir() ] : []),
                    bankPaths:   bankPaths,
                    normalize:   { enabled: normEnabled, lufs: normLufs, peakDb: normPeak },
                    events:      eventConfigs
                };

                var manifestPath = toolsUrl + "recorder_manifest.json";
                var file = studio.system.getFile(manifestPath);
                file.open(studio.system.openMode.WriteOnly);
                file.writeText(JSON.stringify(manifest, null, 2));
                file.close();

                // ── Log ───────────────────────────────────────────
                console.log("");
                console.log("┌─────────────────────────────────────────┐");
                console.log("│        FMOD Event Recorder — Launch      │");
                console.log("  Version       : " + ER_VERSION);
                console.log("└─────────────────────────────────────────┘");
                console.log("  Output folder : " + outputPath);
                console.log("  Format        : " + format);
                console.log("  Sample rate   : " + sampleRate);
                console.log("  Channels      : " + channels);
                console.log("  Takes         : " + numTakes);
                console.log("  Banks         : " + bankPaths.length);
                console.log("  Events        : " + eventConfigs.length);
                console.log("  Tools folder  : " + toolsUrl);
                console.log("  Manifest      : " + manifestPath);

                // ── Launch Python renderer (cross-platform) ───────────
                var scriptPath = toolsUrl + "fmod_recorder.py";

                // Blank any previous results BEFORE launching, so a renderer that
                // crashes or never runs can't leave stale JSON that reports false
                // success. The renderer overwrites this with fresh results on success.
                var resultsPath = toolsUrl + "recorder_results.json";
                var clearFile = studio.system.getFile(resultsPath);
                clearFile.open(studio.system.openMode.WriteOnly);
                clearFile.writeText("");
                clearFile.close();

                // Launch the Python renderer. The two platforms need different
                // mechanisms (each is what actually works there):
                //   • Windows: run python.exe directly with a single un-quoted
                //     argument. A cmd/.bat wrapper fails — studio.system.start
                //     mangles its quoting — and start is non-blocking here (the
                //     "Rendering…" dialog below lets the process run).
                //   • macOS: studio.system.start won't run the venv python binary
                //     directly, but it does run a /bin/sh script — and blocks until
                //     it finishes, so results are ready immediately after.
                if (isWindows) {
                    var pyExe = (toolsUrl + "venv/Scripts/python.exe").replace(/\//g, "\\");
                    studio.system.start(pyExe, {args: scriptPath.replace(/\//g, "\\"), timeout: 600000});
                } else {
                    var pythonPath   = toolsUrl + "venv/bin/python";
                    var launchScript = toolsUrl + "launch.sh";
                    var launchFile = studio.system.getFile(launchScript);
                    launchFile.open(studio.system.openMode.WriteOnly);
                    launchFile.writeText("#!/bin/sh\nexec \"" + pythonPath + "\" \"" + scriptPath + "\" \"" + manifestPath + "\"\n");
                    launchFile.close();
                    // Array form (as FMOD's shipped SVN.sc.js uses): a per-machine
                    // install path always contains spaces ("Application Support",
                    // "FMOD Studio"), and each array element stays one argument.
                    studio.system.start("/bin/sh", {args: [launchScript], timeout: 600000});
                }

                this.closeDialog();

                // ── Read results & show summary ────────────────────────────
                // studio.system.start does NOT block on Windows (it blocks on
                // macOS), and the renderer process only actually launches once
                // FMOD's event loop runs again — so we must NOT busy-wait here (a
                // wait-loop freezes the event loop and the renderer never starts).
                // A modal dialog runs its own event loop, so on Windows we keep a
                // "Rendering…" dialog open (letting the renderer run) and read the
                // results when the user clicks "Show Results". On macOS the render
                // has already finished by the time start() returns.

                // Reads recorder_results.json once; returns "" if empty/missing.
                function readResultsRaw() {
                    var raw = "";
                    try {
                        var rf = studio.system.getFile(resultsPath);
                        rf.open(studio.system.openMode.ReadOnly);
                        raw = rf.readText(1048576);
                        rf.close();
                    } catch (e) { raw = ""; }
                    return raw;
                }

                // Builds and shows the results summary dialog for a parsed payload.
                function showSummary(results) {
                    var recs = results.recordings || [];
                    var lines = "";
                    for (var r = 0; r < recs.length; r++) {
                        var rec = recs[r];
                        if (rec.success) {
                            var dur = rec.durationSec !== null ? rec.durationSec.toFixed(2) + "s" : "?s";
                            var bd  = rec.bitDepth || "?-bit";
                            var sr2 = rec.sampleRate ? (rec.sampleRate / 1000).toFixed(1) + " kHz" : "";
                            var ch  = rec.channels === 2 ? "Stereo" : "Mono";
                            var kb  = rec.sizeKB !== null ? rec.sizeKB + " KB" : "";
                            lines += "✔  " + rec.filename + "\n";
                            lines += "     " + bd + "  " + sr2 + "  " + ch + "  " + dur + "  " + kb + "\n\n";
                        } else {
                            var ep2 = rec.eventPath + (rec.suffix || "");
                            lines += "✘  " + ep2 + "\n";
                            lines += "     Error: " + (rec.error || "unknown") + "\n\n";
                        }
                    }

                    // Bank problems first: they explain every failure below.
                    if (results.bankError) {
                        lines = "⚠  " + results.bankError + "\n\n" + lines;
                    } else if (typeof results.banksFound === "number" &&
                               results.banksFound < results.banksExpected) {
                        lines = "Note: " + (results.banksExpected - results.banksFound) + " of " +
                                results.banksExpected + " banks were not found on disk (not built yet?). " +
                                "Events in those banks can't be recorded.\n\n" + lines;
                    }

                    var summaryTitle = results.success + " recorded";
                    if (results.failed > 0) summaryTitle += ",  " + results.failed + " failed";

                    studio.ui.showModalDialog({
                        windowTitle: "Event Recorder v" + ER_VERSION + " — Results",
                        windowWidth:  580,
                        windowHeight: 480,
                        widgetType: studio.ui.widgetType.Layout,
                        layout:     studio.ui.layoutType.VBoxLayout,
                        spacing:    8,
                        contentsMargins: { left: 14, right: 14, top: 14, bottom: 14 },
                        items: [
                            { widgetType: studio.ui.widgetType.Label, text: "<b>Recording Complete — " + summaryTitle + "</b>" },
                            { widgetType: studio.ui.widgetType.Label, text: "Output: " + results.outputPath },
                            { widgetType: studio.ui.widgetType.Label, text: "<hr/>" },
                            {
                                widgetType:    studio.ui.widgetType.TextEdit,
                                text:          lines,
                                minimumWidth:  540,
                                minimumHeight: 340,
                                sizePolicy: {
                                    horizontalPolicy: studio.ui.sizePolicy.Expanding,
                                    verticalPolicy:   studio.ui.sizePolicy.Expanding
                                }
                            },
                            {
                                widgetType: studio.ui.widgetType.Layout,
                                layout:     studio.ui.layoutType.HBoxLayout,
                                spacing:    8,
                                items: [
                                    {
                                        widgetType: studio.ui.widgetType.PushButton,
                                        text:       "Open Output Folder",
                                        onClicked:  function () {
                                            // Reveal the output folder — Explorer on Windows, Finder on macOS.
                                            try {
                                                if (isWindows) {
                                                    // Pass the path as a single un-quoted argument — studio.system.start
                                                    // quotes `args` itself, so adding our own quotes breaks it and Explorer
                                                    // falls back to the default (Documents) folder.
                                                    studio.system.start("explorer.exe", {args: results.outputPath.replace(/\//g, "\\")});
                                                } else {
                                                    var openScript = toolsUrl + "_open_output.sh";
                                                    var of = studio.system.getFile(openScript);
                                                    of.open(studio.system.openMode.WriteOnly);
                                                    of.writeText("#!/bin/sh\nopen \"" + results.outputPath + "\"\n");
                                                    of.close();
                                                    studio.system.start("/bin/sh", {args: [openScript]});
                                                }
                                            } catch (e) {
                                                alert("Couldn't open the folder:\n" + e + "\n\n" + results.outputPath);
                                            }
                                        }
                                    },
                                    { widgetType: studio.ui.widgetType.PushButton, text: "Close", onClicked: function () { this.closeDialog(); } }
                                ]
                            }
                        ]
                    });
                }

                // Parses raw JSON and shows the summary (shared parse handling).
                function presentResults(raw) {
                    var results = null;
                    try {
                        results = JSON.parse(raw);
                    } catch (e) {
                        alert("Recordings finished but the results file could not be parsed:\n" + e);
                        return;
                    }
                    showSummary(results);
                }

                if (isWindows) {
                    // The renderer runs in the background while this dialog is
                    // open (its event loop is what lets the process run at all).
                    studio.ui.showModalDialog({
                        windowTitle: "Event Recorder v" + ER_VERSION + " — Rendering",
                        windowWidth:  540,
                        windowHeight: 200,
                        widgetType: studio.ui.widgetType.Layout,
                        layout:     studio.ui.layoutType.VBoxLayout,
                        spacing:    10,
                        contentsMargins: { left: 16, right: 16, top: 16, bottom: 16 },
                        items: [
                            { widgetType: studio.ui.widgetType.Label, text: "<b>Rendering " + eventConfigs.length + " event" + (eventConfigs.length > 1 ? "s" : "") + " in the background…</b>" },
                            { widgetType: studio.ui.widgetType.Label, text: "When your audio files appear in the folder below, click <b>Show Results</b>.<br/>Large batches can take a minute or two." },
                            { widgetType: studio.ui.widgetType.Label, text: outputPath },
                            {
                                widgetType: studio.ui.widgetType.Layout,
                                layout:     studio.ui.layoutType.HBoxLayout,
                                spacing:    8,
                                items: [
                                    {
                                        widgetType: studio.ui.widgetType.PushButton,
                                        text:       "Show Results",
                                        onClicked:  function () {
                                            var raw = readResultsRaw();
                                            if (!raw || raw.replace(/\s/g, "") === "") {
                                                alert("Still rendering — give it a few seconds, then click Show Results again.");
                                                return;
                                            }
                                            this.closeDialog();
                                            presentResults(raw);
                                        }
                                    },
                                    { widgetType: studio.ui.widgetType.PushButton, text: "Cancel", onClicked: function () { this.closeDialog(); } }
                                ]
                            }
                        ]
                    });
                } else {
                    // macOS: start() blocked until the render finished, so the
                    // results file is already populated.
                    var raw = readResultsRaw();
                    if (!raw || raw.replace(/\s/g, "") === "") {
                        alert("The renderer did not complete — no results were produced.\n\n" +
                              "The render may have crashed or failed to launch. Check that setup has been run and that you've added " +
                              "the FMOD Engine libraries to Tools/FMOD_Event_Recorder/libs/macos/ (they aren't included with the tool; " +
                              "see PUT_MACOS_DYLIBS_HERE.txt there), " +
                              "and that the banks have been built.");
                        return;
                    }
                    presentResults(raw);
                }
            }
        });

        // ── Show main dialog ──────────────────────────────────
        studio.ui.showModalDialog({
            windowTitle: "Event Recorder v" + ER_VERSION + "  —  " + events.length + " event" + (events.length > 1 ? "s" : "") + " selected",
            windowWidth:  540,
            windowHeight: 620,
            widgetType: studio.ui.widgetType.Layout,
            layout:     studio.ui.layoutType.VBoxLayout,
            spacing:    8,
            contentsMargins: { left: 14, right: 14, top: 14, bottom: 14 },
            items: mainItems
        });
    },

    isEnabled: function () { return studio.project !== null; }
});
