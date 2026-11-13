// Profiler First Person
// Version: 0.5.0
//
// For FMOD Studio 2.02, 2.03 and 2.04 (built against 2.03.14, macOS). Menu:
// Scripts > DAT-audio Utilities > Open Profiler First Person View...
//
// A floating first-person 3D view of a profiler session: the game's event
// instances, their distance spheres, its listener(s) and the listener's probe
// (attenuation position), seen from the listener at the profiler's playhead.
// It is a 3D version of the Profiler's own 3D View panel. View only: it never
// changes the project.
//
// Where the data comes from:
// - Every profiler recording's API capture is saved as plain text in the
//   session's folder: <project>/Metadata/ProfilerSession/{id}/apicommands-NNNN.txt.
//   The app reads those files itself (they can be tens of MB), including
//   set3DAttributes and setListenerAttributes with the attenuation position.
// - Recording N starts on the session timeline at the Nth
//   ProfilerRecordingMarker. This script sends those positions.
// - A ProfilerSession is an Event, so the timeline cursor and playhead are
//   read with event.getCursorPosition / getPlayheadPosition(session.timeline).
//   Their behaviour on a profiler session is unverified (the log line
//   "Time source" reports what they return).
// - The app asks for the events named in the captures (GUIDs) and this
//   script answers with their name, path and spatializer ranges, looked up
//   in the open project, their Event Browser colour, and the events their
//   instruments play (child events, drawn at the parent instance).
//
// The view is the companion app Tools/ProfilerFirstPerson/profiler_first_person,
// started with studio.system.startAsync and linked over its stdin/stdout. A
// small "Profiler First Person link" window owns the timer that carries the link.
//
// One JSON object per line each way:
//   Studio → app: {"type":"hello"|"session"|"time"|"events"|"selection", ...}
//   ("selection": the events of the profiler tracks selected in the Profiler
//   window; while any are selected the view draws only those)
//   app → Studio: {"cmd":"events","ids":["{guid}", ...]}
// The app acknowledges every line it reads with an empty line at once:
// writeStandardInput waits for child output and blocks Studio until it gets
// some (see the Automation Curve Library's 0.6.0 note).

var PFP_VERSION = "0.5.0";
var PFP_TITLE = "Profiler First Person";
var pfpScriptDir = __dirname;       // valid only while this file loads

var PFP_TIMER_MS = 20;
var PFP_SESSION_CHECK_EVERY = 25;   // ticks between session / marker re-reads (~0.5 s)
var PFP_TIME_LOG_TICKS = 3;         // log the raw cursor/playhead values on the first ticks
var PFP_TIME_LOG_CHANGES = 60;      // then log the first jumps (clicks, scrubs; not smooth playback)

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Open Profiler First Person View...",
    execute: function () { var m = pfpLaunch(); if (m.error) alert(m.error); }
});

var PFP = {
    gen: 0,
    proc: null, buf: "", ended: true, tick: 0, out: [],
    session: null, sentSession: "", sentTime: "", status: "", lastDiag: "", ready: false,
    timeLogged: 0, sentSelection: "", changesLogged: 0, lastRaw: "", lastC: null, lastP: null, lastTimeAt: 0
};

function pfpLog(s) { studio.system.print("[" + PFP_TITLE + "] " + s); }
// Windows when Studio itself is an .exe (the Automation Curve Library's
// check); studio.os.platform is a backstop (its values aren't documented).
function pfpIsWindows() {
    try { if (("" + studio.application.filePath).toLowerCase().indexOf(".exe") >= 0) return true; } catch (e) { }
    try { return !!studio.os && /^win/i.test("" + studio.os.platform); } catch (e) { return false; }
}
function pfpExists(path) { try { return studio.system.getFile(path).exists(); } catch (e) { return false; } }
function pfpIsType(o, t) { try { return !!o && o.isOfType(t); } catch (e) { return false; } }
function pfpR(v) { return Math.round(v * 10000) / 10000; }
function pfpSlash(p) { return ("" + p).split("\\").join("/"); }

// <per-user or project folder>/Tools/ProfilerFirstPerson/, next to the Scripts
// folder this file was loaded from (relative to this script, never absolute).
function pfpToolsDir() {
    var d = pfpSlash(pfpScriptDir);
    if (d.charAt(d.length - 1) === "/") d = d.substring(0, d.length - 1);
    var p = d;
    while (p.length) {
        var cut = p.lastIndexOf("/");
        if (p.substring(cut + 1).toLowerCase() === "scripts") return p.substring(0, cut + 1) + "Tools/ProfilerFirstPerson/";
        if (cut <= 0) break;
        p = p.substring(0, cut);
    }
    return d.substring(0, d.lastIndexOf("/") + 1) + "Tools/ProfilerFirstPerson/";
}
function pfpAppName() { return pfpIsWindows() ? "profiler_first_person.exe" : "profiler_first_person"; }
function pfpAppPath() { return pfpToolsDir() + pfpAppName(); }

// ── The profiler session ─────────────────────────────────────────────────────

function pfpIsSession(o) {
    if (!o) return false;
    if (pfpIsType(o, "ProfilerSession")) return true;
    try { return o.entity === "ProfilerSession"; } catch (e) { return false; }
}

// A selected object's session: the session itself, or the session that owns
// a selected profiler track / marker.
function pfpOwningSession(o) {
    if (!o) return null;
    if (pfpIsSession(o)) return o;
    try { if (o.profilerSession && pfpIsSession(o.profilerSession)) return o.profilerSession; } catch (e) { }
    // A nested profiler track hangs off its parent track.
    try { for (var p = o.parentEvent, n = 0; p && n < 8; p = p.parentEvent, n++) if (p.profilerSession && pfpIsSession(p.profilerSession)) return p.profilerSession; } catch (e) { }
    try { if (o.event && pfpIsSession(o.event)) return o.event; } catch (e) { }
    try { if (o.timeline && o.timeline.event && pfpIsSession(o.timeline.event)) return o.timeline.event; } catch (e) { }
    return null;
}

function pfpFirstSession(list) {
    if (!list) return null;
    for (var i = 0; i < list.length; i++) { var s = pfpOwningSession(list[i]); if (s) return s; }
    return null;
}

// The session to follow: the focused editor's (the Profiler window), else a
// selected session (editor or browser), else the one already followed, else
// the project's only session.
function pfpPickSession() {
    var cur = null, why = "", all = null;
    try { cur = pfpOwningSession(studio.window.editorCurrent()); } catch (e) { cur = null; }
    if (cur) why = "editorCurrent";
    if (!cur) { try { cur = pfpFirstSession(studio.window.editorSelection()); } catch (e) { cur = null; } if (cur) why = "editorSelection"; }
    if (!cur) { try { cur = pfpFirstSession(studio.window.browserSelection()); } catch (e) { cur = null; } if (cur) why = "browserSelection"; }
    if (!cur) { try { cur = pfpOwningSession(studio.window.browserCurrent()); } catch (e) { cur = null; } if (cur) why = "browserCurrent"; }
    if (!cur && PFP.session && PFP.tick % PFP_SESSION_CHECK_EVERY !== 0) return PFP.session;
    if (!cur) {
        try { all = studio.project.model.ProfilerSession.findInstances(); } catch (e) { all = []; }
        if (PFP.session) for (var i = 0; i < all.length; i++) if (all[i].id === PFP.session.id) { cur = PFP.session; why = "kept"; }
        if (!cur && all.length === 1) { cur = all[0]; why = "the only session"; }
    }
    var diag = (cur ? "'" + cur.name + "' " + cur.id + " via " + why : "none") + " | sessions in project=" + (all ? all.length : "?");
    if (diag !== PFP.lastDiag && (why !== "kept" || !PFP.lastDiag)) { PFP.lastDiag = diag; pfpLog("Session: " + diag); }
    return cur;
}

// The session's folder: <project folder>/Metadata/ProfilerSession/{id}/.
// A session recorded since the last project save lives under
// <project folder>/.unsaved/ until the save moves it. The app reads both.
function pfpSessionDir(s, sub) {
    var f = pfpSlash(studio.project.filePath || "");
    var dir = f.substring(0, f.lastIndexOf("/") + 1);
    return dir + sub + "Metadata/ProfilerSession/" + s.id + "/";
}

// Recording start positions (seconds), sorted.
function pfpMarkers(s) {
    var out = [], seen = {}, lists = [], i, j, m;
    try { lists.push(s.timeline.markers); } catch (e) { }
    try { for (i = 0; i < s.markerTracks.length; i++) lists.push(s.markerTracks[i].markers); } catch (e) { }
    for (i = 0; i < lists.length; i++) {
        if (!lists[i]) continue;
        for (j = 0; j < lists[i].length; j++) {
            m = lists[i][j];
            if (!pfpIsType(m, "ProfilerRecordingMarker") || seen[m.id]) continue;
            seen[m.id] = true;
            out.push(m.position);
        }
    }
    out.sort(function (a, b) { return a - b; });
    return out;
}

function pfpSessionJson(s) {
    return JSON.stringify({ type: "session", id: s.id, name: s.name, dir: pfpSessionDir(s, ""), unsaved: pfpSessionDir(s, ".unsaved/"), markers: pfpMarkers(s) });
}

// The session timeline's cursor and playhead. Logged raw on the first ticks
// (their behaviour on a profiler session is the open question).
function pfpTime(s) {
    var c = null, p = null, ce = "", pe = "";
    try { c = s.getCursorPosition(s.timeline); } catch (e) { ce = "" + e; }
    try { p = s.getPlayheadPosition(s.timeline); } catch (e) { pe = "" + e; }
    var raw = "cursor=" + c + (ce ? " (" + ce + ")" : "") + " playhead=" + p + (pe ? " (" + pe + ")" : "");
    if (PFP.timeLogged < PFP_TIME_LOG_TICKS) {
        PFP.timeLogged++;
        pfpLog("Time source: " + raw);
    } else if (raw !== PFP.lastRaw && PFP.changesLogged < PFP_TIME_LOG_CHANGES) {
        // Log jumps only (a click or scrub on the timeline, playback starting
        // or stopping), not the smooth advance of playback: is scrubbing seen?
        var now = new Date().getTime(), dt = (now - PFP.lastTimeAt) / 1000;
        var smooth = isFinite(p) && isFinite(PFP.lastP) && c === PFP.lastC && p >= PFP.lastP && p - PFP.lastP <= dt * 1.5 + 0.1;
        if (!smooth) {
            PFP.changesLogged++;
            pfpLog("Time jumped: " + raw + (PFP.changesLogged === PFP_TIME_LOG_CHANGES ? " (no more time logging this run)" : ""));
        }
    }
    PFP.lastRaw = raw; PFP.lastC = c; PFP.lastP = p; PFP.lastTimeAt = new Date().getTime();
    var t = { type: "time", cursor: isFinite(c) && c !== null ? pfpR(c) : 0 };
    if (isFinite(p) && p !== null) t.playhead = pfpR(p);
    return t;
}

// Events of the profiler tracks selected in the Profiler window (tracks of
// buses are ignored). The view then draws only those.
var PFP_SELECTION_EVERY = 5;        // ticks between selection reads (~100 ms)

function pfpSelectedEvents() {
    var sel = null, ids = [], seen = {}, i, t, o;
    try { sel = studio.window.editorSelection(); } catch (e) { sel = null; }
    if (!sel) return ids;
    for (i = 0; i < sel.length; i++) {
        t = sel[i];
        if (!pfpIsType(t, "ProfilerTrack")) continue;
        try { o = t.objectBeingTracked; } catch (e) { o = null; }
        if (!o || !(pfpIsType(o, "Event") || o.entity === "Event" || o.entity === "Snapshot")) continue;
        if (!seen[o.id]) { seen[o.id] = true; ids.push(o.id); }
    }
    ids.sort();
    return ids;
}

// ── Events named in the captures ─────────────────────────────────────────────

// The real effect behind a chain entry: preset effects sit in the chain as a
// ProxyEffect whose preset holds the effect (for example, a preset spatializer
// on the master track was missed and the event showed as 2D).
function pfpRealEffect(fx) {
    if (pfpIsType(fx, "ProxyEffect")) {
        try { return fx.preset && fx.preset.effect ? fx.preset.effect : null; } catch (e) { return null; }
    }
    return fx;
}

function pfpSpatialOf(fx, macroMin, macroMax, track, preset) {
    return {
        min: pfpR(fx.overrideRange ? fx.minimumDistance : macroMin),
        max: pfpR(fx.overrideRange ? fx.maximumDistance : macroMax),
        is3D: true, rolloff: fx.distanceRolloffType, size: pfpR(fx.soundSize), minExtent: pfpR(fx.minimumExtent),
        extentMode: fx.extentMode, spatializer: fx.entity === "ObjectSpatialiserEffect" ? "object" : "spatializer",
        track: track, preset: preset, override: !!fx.overrideRange
    };
}

// Properties in an event automated on the built-in Distance (parameterType 3)
// or Distance (Normalized) (10) parameter, e.g. a snapshot instrument's
// intensity, or a gain-on-distance with the spatializer's curve Off. These
// react to the listener's position even without a distance curve.
// Scans each track's mixer group, effects (presets unwrapped) and instruments.
var PFP_DISTANCE_TYPES = { 3: "Distance", 10: "Distance (Normalized)" };

function pfpDistanceDriven(ev) {
    var out = [], objs = [], tracks = [ev.masterTrack], i, j, k, o;
    try { for (i = 0; i < ev.groupTracks.length; i++) tracks.push(ev.groupTracks[i]); } catch (e) { }
    for (i = 0; i < tracks.length; i++) {
        var t = tracks[i];
        if (!t) continue;
        var mg = t.mixerGroup;
        if (mg) {
            objs.push(mg);
            var chain = mg.effectChain;
            if (chain) for (j = 0; j < chain.effects.length; j++) {
                objs.push(chain.effects[j]);
                var real = pfpRealEffect(chain.effects[j]);
                if (real && real !== chain.effects[j]) objs.push(real);
            }
        }
        try { for (j = 0; j < t.modules.length; j++) objs.push(t.modules[j]); } catch (e) { }
    }
    for (i = 0; i < objs.length; i++) {
        o = objs[i];
        var autos = null;
        try { autos = o.automators; } catch (e) { autos = null; }
        if (!autos) continue;
        for (j = 0; j < autos.length; j++) {
            var curves = autos[j].automationCurves;
            for (k = 0; k < curves.length; k++) {
                var prm = curves[k].parameter, gp = prm;
                try { if (gp && gp.parameterType === undefined && gp.preset) gp = gp.preset; } catch (e) { }
                var type = gp ? gp.parameterType : -1;
                if (!PFP_DISTANCE_TYPES[type]) continue;
                var label = o.entity === "SnapshotModule" ? "snapshot intensity" : o.entity + " " + autos[j].nameOfPropertyBeingAutomated;
                try { if (o.entity === "SnapshotModule" && o.snapshot) label += " (" + o.snapshot.name + ")"; } catch (e) { }
                out.push({ what: label, normalized: type === 10, max: pfpR(gp.maximum) });
            }
        }
    }
    return out;
}

// Every spatializer in an event (master track first, then group tracks),
// with its effective range: its own when it overrides, else the event's
// min/max (event macros). The first one describes the event; the rest are
// sent as "others" (e.g. a smaller spatializer on one track).
function pfpSpatial(ev) {
    var a = ev.automatableProperties;
    var mMin = a ? a.minimumDistance : 1, mMax = a ? a.maximumDistance : 20;
    var tracks = [{ t: ev.masterTrack, name: "Master" }], i, j, chain, fx, real, all = [];
    try { for (i = 0; i < ev.groupTracks.length; i++) tracks.push({ t: ev.groupTracks[i], name: ev.groupTracks[i].mixerGroup.name || ("Track " + (i + 1)) }); } catch (e) { }
    for (i = 0; i < tracks.length; i++) {
        chain = tracks[i].t && tracks[i].t.mixerGroup && tracks[i].t.mixerGroup.effectChain;
        if (!chain) continue;
        for (j = 0; j < chain.effects.length; j++) {
            fx = chain.effects[j];
            real = pfpRealEffect(fx);
            if (!real || !pfpIsType(real, "SpatialEffect")) continue;
            var preset = "";
            if (real !== fx) { try { preset = fx.preset.name || ""; } catch (e) { preset = ""; } }
            all.push(pfpSpatialOf(real, mMin, mMax, tracks[i].name, preset));
        }
    }
    var dd = [];
    try { dd = pfpDistanceDriven(ev); } catch (e) { dd = []; }
    if (!all.length) return { min: pfpR(mMin), max: pfpR(mMax), is3D: false, rolloff: 0, size: 0, minExtent: 0, extentMode: 0, spatializer: "", track: "", preset: "", others: [], distanceDriven: dd };
    var r = all[0];
    r.others = all.slice(1);
    r.distanceDriven = dd;
    return r;
}

var PFP_CHILD_DEPTH = 4;            // nested events of nested events, this deep

// The Event Browser colour: "Default", "Red", "Cyan Light 2", ... (Colorable.color).
function pfpColor(o) { try { return "" + (o.color || "Default"); } catch (e) { return "Default"; } }

// Events played by an event's instruments (event instruments, also inside
// multi instruments and scatterers), recursively. Each runs at its parent
// instance's position (scatterer children around it).
function pfpChildEvents(ev, depth, parentId, seen, out) {
    var tracks = [ev.masterTrack], i, j;
    try { for (i = 0; i < ev.groupTracks.length; i++) tracks.push(ev.groupTracks[i]); } catch (e) { }
    function visit(m, scattered) {
        if (!m) return;
        if (pfpIsType(m, "EventSound")) {
            var c = null;
            try { c = m.event; } catch (e) { c = null; }
            if (!c || seen[c.id]) return;
            seen[c.id] = true;
            var item = { id: c.id, parent: parentId, name: c.name, color: pfpColor(c), scattered: !!scattered, depth: depth };
            try { item.path = c.getPath(); } catch (e) { item.path = ""; }
            try { item.spatial = pfpSpatial(c); } catch (e) { item.spatial = null; }
            out.push(item);
            if (depth < PFP_CHILD_DEPTH) pfpChildEvents(c, depth + 1, c.id, seen, out);
        } else if (pfpIsType(m, "SoundScatterer")) {
            try { visit(m.sound, true); } catch (e) { }
        } else if (pfpIsType(m, "MultiSound")) {
            try { for (var k = 0; k < m.sounds.length; k++) visit(m.sounds[k], scattered); } catch (e) { }
        }
    }
    for (i = 0; i < tracks.length; i++) {
        if (!tracks[i]) continue;
        try { for (j = 0; j < tracks[i].modules.length; j++) visit(tracks[i].modules[j], false); } catch (e) { }
    }
    return out;
}

function pfpEventInfo(id) {
    var o = null, r = { id: id };
    try { o = studio.project.lookup(id); } catch (e) { o = null; }
    if (!o || !(pfpIsType(o, "Event") || o.entity === "Event" || o.entity === "Snapshot")) { r.missing = true; return r; }
    r.name = o.name;
    r.color = pfpColor(o);
    try { r.kind = o.entity; } catch (e) { r.kind = ""; }
    try { r.path = o.getPath(); } catch (e) { r.path = ""; }
    if (r.kind !== "Snapshot") {
        try { r.spatial = pfpSpatial(o); } catch (e) { r.spatial = null; }
        var seen = {}; seen[o.id] = true;
        try { r.children = pfpChildEvents(o, 1, o.id, seen, []); } catch (e) { r.children = []; }
    }
    return r;
}

// ── The link ─────────────────────────────────────────────────────────────────

function pfpSend(obj) { PFP.out.push(typeof obj === "string" ? obj : JSON.stringify(obj)); }

function pfpFlush() {
    if (!PFP.out.length || !PFP.proc) return;
    var text = PFP.out.join("\n") + "\n";
    PFP.out = [];
    try { PFP.proc.writeStandardInput(text, 100); } catch (e) { pfpLog("Couldn't write to the view: " + e); }
}

// Each launch has its own link window. A window from an earlier launch (its
// view closed, then the menu item run again) closes itself, so there's only
// ever one, and two timers never poll the same view.
function pfpCloseLink(w) {
    try { w.closeDialog(); } catch (e) { pfpLog("Couldn't close the link window: " + e); }
}

function pfpPoll(w, gen) {
    if (gen !== PFP.gen) { pfpCloseLink(w); return; }
    var proc = PFP.proc, running = false;
    if (!proc) { pfpCloseLink(w); return; }
    try { running = proc.isRunning(); } catch (e) { running = false; }
    if (!running) {
        // Never read a finished ScriptProcess (that raises an uncatchable error).
        // The view was closed: the link window goes with it.
        PFP.proc = null; PFP.ended = true;
        pfpLog("The view was closed; closing its link window.");
        pfpCloseLink(w);
        return;
    }
    PFP.tick++;

    var chunk = "", err = "", nl, line, msg, i;
    try { chunk = "" + (proc.readAllStandardOutput() || ""); } catch (e) { chunk = ""; }
    try { err = "" + (proc.readAllStandardError() || ""); } catch (e) { err = ""; }
    if (err) pfpLog("view: " + err.split("\n").join(" ").trim());
    PFP.buf += chunk;
    var asked = [];
    while ((nl = PFP.buf.indexOf("\n")) >= 0) {
        line = PFP.buf.substring(0, nl).trim();
        PFP.buf = PFP.buf.substring(nl + 1);
        if (!line || line.charAt(0) !== "{") continue;
        try { msg = JSON.parse(line); } catch (e) { continue; }
        if (msg.cmd === "events" && msg.ids && msg.ids.length !== undefined) for (i = 0; i < msg.ids.length; i++) asked.push("" + msg.ids[i]);
    }

    if (!PFP.ready) {
        if (chunk.indexOf("\"ready\"") < 0) { pfpStatus(w, "Starting the Profiler First Person view…"); return; }
        PFP.ready = true;
        pfpSend({ type: "hello", studio: studio.version, script: PFP_VERSION, project: studio.project.filePath || "" });
    }

    if (asked.length) {
        var items = [];
        for (i = 0; i < asked.length; i++) items.push(pfpEventInfo(asked[i]));
        pfpSend({ type: "events", items: items });
    }

    var s = pfpPickSession();
    // By id: the model hands out a new wrapper object for each lookup.
    if ((s ? s.id : "") !== (PFP.session ? PFP.session.id : "")) { PFP.sentSession = ""; PFP.sentTime = ""; }
    PFP.session = s;
    if (!s) {
        var none = JSON.stringify({ type: "session", none: true });
        if (PFP.sentSession !== none) { PFP.sentSession = none; pfpSend(none); }
        pfpStatus(w, "Open a profiler session (Window > Profiler) and select it.");
        pfpFlush();
        return;
    }
    if (!PFP.sentSession || PFP.tick % PFP_SESSION_CHECK_EVERY === 0) {
        // Markers change while recording (a new recording adds one).
        var sj = pfpSessionJson(s);
        if (sj !== PFP.sentSession) { PFP.sentSession = sj; pfpSend(sj); }
    }
    if (PFP.tick % PFP_SELECTION_EVERY === 0) {
        var selj = JSON.stringify({ type: "selection", ids: pfpSelectedEvents() });
        if (selj !== PFP.sentSelection) {
            PFP.sentSelection = selj; pfpSend(selj);
            pfpLog("Selected profiler tracks: " + selj);
        }
    }
    var tj = JSON.stringify(pfpTime(s));
    if (tj !== PFP.sentTime) { PFP.sentTime = tj; pfpSend(tj); }
    pfpFlush();
    pfpStatus(w, "<b>Linked</b> to profiler session '" + s.name + "'. Keep this window open while you use the view.");
}

function pfpStatus(w, text) {
    if (text === PFP.status) return;
    PFP.status = text;
    w.findWidget("pfp_status").setText(text);
}

function pfpOpenLinkWindow() {
    var T = studio.ui.widgetType, Lt = studio.ui.layoutType, gen = PFP.gen;
    studio.ui.showModelessDialog({
        windowTitle: "Profiler First Person link  v" + PFP_VERSION,
        windowWidth: 380, windowHeight: 110,
        widgetType: T.Layout, layout: Lt.VBoxLayout,
        items: [
            { widgetType: T.Label, widgetId: "pfp_status", wordWrap: true, text: "Starting the Profiler First Person view…" },
            { widgetType: T.Label, wordWrap: true, text: "<span style=\"color:#8a8a8a\">This window carries the link between the Profiler and " +
                "the First Person view. Closing it unlinks the view; closing the view closes it.</span>" },
            {
                // Last widget, so the status label exists. Owns the polling timer.
                widgetType: T.Spacer,
                onConstructed: function () { try { this.startTimer(PFP_TIMER_MS); } catch (e) { pfpLog("Link timer failed: " + e); } },
                onTimerEvent: function () { pfpPoll(this, gen); }
            }
        ]
    });
}

function pfpRunning() {
    try { return !!PFP.proc && PFP.proc.isRunning(); } catch (e) { return false; }
}

function pfpLaunch() {
    if (pfpRunning()) return { error: "The Profiler First Person view is already open (look for its window)." };
    var exe = pfpAppPath();
    if (!pfpExists(exe)) {
        return { error: "The Profiler First Person view app (" + pfpAppName() + ") isn't installed. Expected it at:\n" + exe +
            "\n\nSee Tools/ProfilerFirstPerson/README.md for how to install it." };
    }
    try {
        PFP.proc = studio.system.startAsync(exe, { args: ["--bridge"], workingDir: pfpToolsDir() });
    } catch (e) {
        return { error: "Couldn't start the Profiler First Person view: " + e };
    }
    if (!PFP.proc) return { error: "Couldn't start the Profiler First Person view (no process)." };
    PFP.gen++;                          // retires any older link window
    PFP.buf = ""; PFP.out = []; PFP.ended = false; PFP.tick = 0; PFP.status = "";
    PFP.session = null; PFP.sentSession = ""; PFP.sentTime = ""; PFP.lastDiag = ""; PFP.ready = false; PFP.timeLogged = 0;
    PFP.changesLogged = 0; PFP.lastRaw = ""; PFP.lastC = null; PFP.lastP = null; PFP.lastTimeAt = 0; PFP.sentSelection = "";
    pfpLog("v" + PFP_VERSION + ": started " + exe);
    pfpOpenLinkWindow();
    return { ok: true };
}
