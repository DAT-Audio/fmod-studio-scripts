// Sandbox First Person
// Version: 0.3.0
//
// Part of DAT-audio Utilities. For FMOD Studio 2.02, 2.03 and 2.04 (tested on
// 2.03.14, macOS). Menu: Scripts > DAT-audio Utilities > Open Sandbox First
// Person View...
//
// A floating first-person 3D view of the open Sandbox scene, seen from its
// listener. Flying in the view moves the real Sandbox listener, and dragging
// the listener (or emitters) in the Sandbox moves the view, live.
//
// The view is the companion app Tools/SandboxFirstPerson/sandbox_first_person,
// started with studio.system.startAsync and linked over its stdin/stdout. A
// small "First Person link" window owns the ~15 ms timer that carries the link
// (Studio's script UI can't draw 3D, so the app does the drawing).
//
// One JSON object per line each way:
//   Studio → app: {"type":"hello"|"scene"|"listener"|"status", ...}
//   app → Studio: {"cmd":"move","x":..,"z":..,"rot":..}
//                 {"cmd":"emitters","set":[{"id":..,"x":..,"z":..}, ...]}
//
// Elevation (0.3.0): the Sandbox has no height, so while you're above or below
// the plane the app asks for distance-reactive emitters to be moved along the
// ground (each keeps its left/right offset from you and sits at its true 3D
// distance). "emitters" is the full set to hold moved; any emitter left out is
// put back. The script remembers each moved emitter's real position, reports
// real positions to the app, puts every emitter back when the view closes or
// the scene changes, and treats a Sandbox drag of a moved emitter as its new
// real position.
// The app acknowledges every line it reads with an empty line at once:
// writeStandardInput waits for child output and blocks Studio until it gets
// some (see the Automation Curve Library's 0.6.0 note).
//
// Findings this relies on (2.03.14):
// - SandboxListener/SandboxEmitter have positionX, positionZ, rotationY. No
//   height: the Sandbox is a flat plane. rotationY is degrees clockwise from
//   the top, forward = (sin rot, cos rot) in (X, Z).
// - Sandbox drags update the model live (~10-20 Hz), so polling sees them.
// - Each timer callback that writes is one undo step. beginUndoMacro fails
//   inside a callback, so a movement gesture is folded afterwards: undo our
//   own N steps with the UI's Undo action, then write the end pose once. The
//   previous first-person step is folded in too, so a whole session leaves a
//   single "listener moved" step. Guarded: only if the undo history grew by
//   exactly our writes, otherwise nothing is undone.

var SFP_VERSION = "0.3.0";
var SFP_TITLE = "Sandbox First Person";
var sfpScriptDir = __dirname;       // valid only while this file loads

var SFP_GESTURE_END_MS = 500;       // no moves for this long ends a gesture
var SFP_FOLD_EVERY = 90;            // long moves (auto spin/circle) fold every ~3 s, so no fold is huge
var SFP_SPATIAL_EVERY = 15;         // ticks between spatializer re-reads (~0.3 s)
var SFP_SCENE_CHECK_EVERY = 25;     // ticks between "is the scene still there" checks

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Open Sandbox First Person View...",
    execute: function () { var m = sfpLaunch(); if (m.error) alert(m.error); }
});

var SFP = {
    proc: null, buf: "", ended: true, tick: 0, out: [],
    scene: null, sentScene: "", sentListener: "", spatial: {},
    written: null,          // the pose we last wrote, so it isn't echoed back
    gesture: null,          // { start, writes, fold } while the app is moving us
    lastMoveT: 0,
    ownTop: -1,             // undo index of our last folded step (-1: none)
    recordOwnTop: false,
    folded: 0, skipped: 0, status: "", lastDiag: "", ready: false,
    orig: {},               // id -> {x, z}: real positions of emitters we've moved
    moved: {},              // id -> {x, z}: where we put them (to spot Sandbox drags)
    tickWrote: false
};

function sfpLog(s) { studio.system.print("[" + SFP_TITLE + "] " + s); }
function sfpNow() { return new Date().getTime(); }
// Windows when Studio itself is an .exe (the Automation Curve Library's
// check); studio.os.platform is a backstop (its values aren't documented).
function sfpIsWindows() {
    try { if (("" + studio.application.filePath).toLowerCase().indexOf(".exe") >= 0) return true; } catch (e) { }
    try { return !!studio.os && /^win/i.test("" + studio.os.platform); } catch (e) { return false; }
}
function sfpExists(path) { try { return studio.system.getFile(path).exists(); } catch (e) { return false; } }
function sfpIsType(o, t) { try { return !!o && o.isOfType(t); } catch (e) { return false; } }
function sfpUndoIndex() { try { return studio._internal.undoStackIndex(studio.project); } catch (e) { return -1; } }
function sfpR(v) { return Math.round(v * 10000) / 10000; }

// <per-user or project folder>/Tools/SandboxFirstPerson/, next to the Scripts
// folder this file was loaded from (relative to this script, never absolute).
function sfpToolsDir() {
    var d = ("" + sfpScriptDir).split("\\").join("/");
    if (d.charAt(d.length - 1) === "/") d = d.substring(0, d.length - 1);
    var p = d;
    while (p.length) {
        var cut = p.lastIndexOf("/");
        if (p.substring(cut + 1).toLowerCase() === "scripts") return p.substring(0, cut + 1) + "Tools/SandboxFirstPerson/";
        if (cut <= 0) break;
        p = p.substring(0, cut);
    }
    return d.substring(0, d.lastIndexOf("/") + 1) + "Tools/SandboxFirstPerson/";
}
function sfpAppName() { return sfpIsWindows() ? "sandbox_first_person.exe" : "sandbox_first_person"; }
function sfpAppPath() { return sfpToolsDir() + sfpAppName(); }

// ── Scene state ──────────────────────────────────────────────────────────────

// Is this object a Sandbox scene? (isOfType, with the entity name as a backstop.)
function sfpIsScene(o) {
    if (!o) return false;
    if (sfpIsType(o, "SandboxScene")) return true;
    try { return o.entity === "SandboxScene"; } catch (e) { return false; }
}

// The scene to follow: the focused editor's if it's a Sandbox (or a Sandbox
// object is selected there), else the one we already follow while it exists,
// else the project's first scene.
function sfpPickScene() {
    var cur = null, sel = null, all = null, i, why;
    try { cur = studio.window.editorCurrent(); } catch (e) { cur = null; }
    if (sfpIsScene(cur)) { why = "editorCurrent"; }
    else {
        try { sel = studio.window.editorSelection(); } catch (e) { sel = null; }
        var first = sel && sel.length ? sel[0] : null;
        var owner = first ? (sfpIsScene(first) ? first : (first.scene && sfpIsScene(first.scene) ? first.scene : null)) : null;
        if (owner) { cur = owner; why = "editorSelection"; }
        else cur = null;
    }
    if (!cur && SFP.scene && SFP.tick % SFP_SCENE_CHECK_EVERY !== 0) return SFP.scene;
    if (!cur) {
        try { all = studio.project.model.SandboxScene.findInstances(); } catch (e) { all = []; }
        if (SFP.scene) for (i = 0; i < all.length; i++) if (all[i].id === SFP.scene.id) { cur = SFP.scene; why = "kept"; }
        if (!cur && all.length) { cur = all[0]; why = "first of " + all.length; }
    }
    // Diagnostics: log what the editor reports whenever the outcome changes.
    var diag = (cur ? "'" + cur.name + "' " + cur.id + " via " + why : "none") + " | editorCurrent=" + sfpDescribeObj(sfpSafe(function () { return studio.window.editorCurrent(); })) +
        " | scenes in project=" + (all ? all.length : "?");
    if (diag !== SFP.lastDiag && (why !== "kept" || !SFP.lastDiag)) { SFP.lastDiag = diag; sfpLog("Scene: " + diag); }
    return cur;
}

function sfpSafe(f) { try { return f(); } catch (e) { return "(error: " + e + ")"; } }
function sfpDescribeObj(o) {
    if (!o) return "null";
    if (typeof o === "string") return o;
    try { return o.entity + (o.name !== undefined ? " '" + o.name + "'" : ""); } catch (e) { return "" + o; }
}

// The real effect behind a chain entry: preset effects sit in the chain as a
// ProxyEffect whose preset holds the effect (for example, a preset spatializer
// on the master track was missed and the event showed as 2D).
function sfpRealEffect(fx) {
    if (sfpIsType(fx, "ProxyEffect")) {
        try { return fx.preset && fx.preset.effect ? fx.preset.effect : null; } catch (e) { return null; }
    }
    return fx;
}

function sfpSpatialOf(fx, macroMin, macroMax, track, preset) {
    return {
        min: sfpR(fx.overrideRange ? fx.minimumDistance : macroMin),
        max: sfpR(fx.overrideRange ? fx.maximumDistance : macroMax),
        is3D: true, rolloff: fx.distanceRolloffType, size: sfpR(fx.soundSize), minExtent: sfpR(fx.minimumExtent),
        extentMode: fx.extentMode, spatializer: fx.entity === "ObjectSpatialiserEffect" ? "object" : "spatializer",
        track: track, preset: preset, override: !!fx.overrideRange
    };
}

// Properties in an event automated on the built-in Distance (parameterType 3)
// or Distance (Normalized) (10) parameter, e.g. a snapshot instrument's
// intensity, or a gain-on-distance with the spatializer's curve Off. These
// react to the listener's position even without a distance curve.
// Scans each track's mixer group, effects (presets unwrapped) and instruments.
var SFP_DISTANCE_TYPES = { 3: "Distance", 10: "Distance (Normalized)" };

function sfpDistanceDriven(ev) {
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
                var real = sfpRealEffect(chain.effects[j]);
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
                if (!SFP_DISTANCE_TYPES[type]) continue;
                var label = o.entity === "SnapshotModule" ? "snapshot intensity" : o.entity + " " + autos[j].nameOfPropertyBeingAutomated;
                try { if (o.entity === "SnapshotModule" && o.snapshot) label += " (" + o.snapshot.name + ")"; } catch (e) { }
                out.push({ what: label, normalized: type === 10, max: sfpR(gp.maximum) });
            }
        }
    }
    return out;
}

// Every spatializer in an event (master track first, then group tracks),
// with its effective range: its own when it overrides, else the event's
// min/max (event macros). The first one describes the event; the rest are
// sent as "others" (e.g. a smaller spatializer on one track).
function sfpSpatial(ev) {
    var a = ev.automatableProperties;
    var mMin = a ? a.minimumDistance : 1, mMax = a ? a.maximumDistance : 20;
    var tracks = [{ t: ev.masterTrack, name: "Master" }], i, j, chain, fx, real, all = [];
    try { for (i = 0; i < ev.groupTracks.length; i++) tracks.push({ t: ev.groupTracks[i], name: ev.groupTracks[i].mixerGroup.name || ("Track " + (i + 1)) }); } catch (e) { }
    for (i = 0; i < tracks.length; i++) {
        chain = tracks[i].t && tracks[i].t.mixerGroup && tracks[i].t.mixerGroup.effectChain;
        if (!chain) continue;
        for (j = 0; j < chain.effects.length; j++) {
            fx = chain.effects[j];
            real = sfpRealEffect(fx);
            if (!real || !sfpIsType(real, "SpatialEffect")) continue;
            var preset = "";
            if (real !== fx) { try { preset = fx.preset.name || ""; } catch (e) { preset = ""; } }
            all.push(sfpSpatialOf(real, mMin, mMax, tracks[i].name, preset));
        }
    }
    var dd = [];
    try { dd = sfpDistanceDriven(ev); } catch (e) { dd = []; }
    if (!all.length) return { min: sfpR(mMin), max: sfpR(mMax), is3D: false, rolloff: 0, size: 0, minExtent: 0, extentMode: 0, spatializer: "", track: "", preset: "", others: [], distanceDriven: dd };
    var r = all[0];
    r.others = all.slice(1);
    r.distanceDriven = dd;
    return r;
}

function sfpSceneJson(sc) {
    var em = [], i, e, s, x, z, refresh = SFP.tick % SFP_SPATIAL_EVERY === 0;
    for (i = 0; i < sc.emitters.length; i++) {
        e = sc.emitters[i];
        s = SFP.spatial[e.id];
        if (!s || refresh) {
            // Name, path and spatializer are re-read every SFP_SPATIAL_EVERY ticks, positions every tick.
            s = { name: "(no event)", path: "", spatial: null, kind: "" };
            if (e.event) {
                s.name = e.event.name;
                // "Snapshot" for snapshots placed in the Sandbox: not positional.
                try { s.kind = e.event.entity; } catch (x) { s.kind = ""; }
                try { s.path = e.event.getPath(); } catch (x) { s.path = ""; }
                if (s.kind !== "Snapshot") { try { s.spatial = sfpSpatial(e.event); } catch (x) { s.spatial = null; } }
            }
            SFP.spatial[e.id] = s;
        }
        // Real positions: a moved emitter reports where it really is.
        x = e.positionX; z = e.positionZ;
        var o = SFP.orig[e.id], m = SFP.moved[e.id];
        if (o) {
            if (m && (Math.abs(x - m.x) > 1e-3 || Math.abs(z - m.z) > 1e-3)) {
                // Moved by someone else (a Sandbox drag, an Undo): that's its real position now.
                SFP.orig[e.id] = o = { x: x, z: z };
                delete SFP.moved[e.id];
            }
            x = o.x; z = o.z;
        }
        em.push({
            id: e.id, name: s.name, path: s.path, kind: s.kind,
            x: sfpR(x), z: sfpR(z), rot: sfpR(e.rotationY), loop: !!e.loopPlayback,
            spatial: s.spatial
        });
    }
    return JSON.stringify({ type: "scene", id: sc.id, name: sc.name, listeners: sc.listeners.length, emitters: em });
}

function sfpListener() {
    var sc = SFP.scene;
    return sc && sc.listeners.length ? sc.listeners[0] : null;
}

// ── Movement from the app, and folding it into one undo step ─────────────────

// Every tick that writes is one undo step; gestures count them.
function sfpWriting() {
    if (!SFP.gesture) {
        var idx = sfpUndoIndex();
        SFP.gesture = { start: idx, writes: 0, fold: SFP.ownTop >= 0 && idx === SFP.ownTop };
    }
    SFP.tickWrote = true;
}

function sfpApplyMove(m) {
    var L = sfpListener();
    if (!L) return;
    sfpWriting();
    L.positionX = m.x; L.positionZ = m.z; L.rotationY = m.rot;
    SFP.written = sfpR(L.positionX) + "," + sfpR(L.positionZ) + "," + sfpR(L.rotationY);
    SFP.sentListener = SFP.written;             // no echo back to the app
}

// Holds the emitters in `set` ({id: {x, z}}) where the app asks, and puts
// every other moved emitter back. With set = {} it puts them all back.
function sfpApplyEmitters(set) {
    var sc = SFP.scene, i, e, t, o;
    if (!sc) return;
    for (i = 0; i < sc.emitters.length; i++) {
        e = sc.emitters[i];
        t = set[e.id];
        o = SFP.orig[e.id];
        if (t) {
            if (!o) SFP.orig[e.id] = { x: e.positionX, z: e.positionZ };
            if (Math.abs(e.positionX - t.x) > 1e-4 || Math.abs(e.positionZ - t.z) > 1e-4) {
                sfpWriting();
                e.positionX = t.x; e.positionZ = t.z;
            }
            SFP.moved[e.id] = { x: e.positionX, z: e.positionZ };
        } else if (o) {
            if (Math.abs(e.positionX - o.x) > 1e-4 || Math.abs(e.positionZ - o.z) > 1e-4) {
                sfpWriting();
                e.positionX = o.x; e.positionZ = o.z;
            }
            delete SFP.orig[e.id]; delete SFP.moved[e.id];
        }
    }
}

// Puts every moved emitter back (view closed, scene changed). Its own step,
// outside any gesture.
function sfpRestoreAll() {
    var sc = SFP.scene, i, e, o, n = 0;
    if (sc) for (i = 0; i < sc.emitters.length; i++) {
        e = sc.emitters[i];
        o = SFP.orig[e.id];
        if (!o) continue;
        try { e.positionX = o.x; e.positionZ = o.z; n++; } catch (x) { }
    }
    SFP.orig = {}; SFP.moved = {};
    if (n) sfpLog("Put " + n + " moved emitter" + (n === 1 ? "" : "s") + " back in place.");
}

function sfpEndGesture() {
    var g = SFP.gesture, L = sfpListener(), sc = SFP.scene, i, e;
    SFP.gesture = null;
    if (!L || g.start < 0) return;
    // The end state: the listener and every emitter (moved ones included).
    var end = { x: L.positionX, z: L.positionZ, rot: L.rotationY }, ends = [];
    if (sc) for (i = 0; i < sc.emitters.length; i++) ends.push({ e: sc.emitters[i], x: sc.emitters[i].positionX, z: sc.emitters[i].positionZ });
    var n = sfpUndoIndex() - g.start, total = n + (g.fold ? 1 : 0), before, a = studio.window.actions.Undo;
    if (n !== g.writes) {
        // Something else was recorded meanwhile (a Sandbox drag, an edit): leave the history alone.
        SFP.skipped++; SFP.ownTop = -1;
        sfpLog("Left " + n + " undo steps as they are (expected " + g.writes + ": something else was recorded meanwhile).");
        return;
    }
    var t0 = sfpNow();
    for (i = 0; i < total; i++) {
        before = sfpUndoIndex();
        try { studio.window.triggerAction(a); } catch (x) { sfpLog("Undo action failed: " + x); break; }
        if (sfpUndoIndex() !== before - 1) { sfpLog("Undo action didn't step back as expected; stopped folding."); break; }
    }
    // Write the end state back as one step.
    L.positionX = end.x; L.positionZ = end.z; L.rotationY = end.rot;
    for (i = 0; i < ends.length; i++) {
        e = ends[i];
        if (e.e.positionX !== e.x || e.e.positionZ !== e.z) { e.e.positionX = e.x; e.e.positionZ = e.z; }
    }
    SFP.written = sfpR(end.x) + "," + sfpR(end.z) + "," + sfpR(end.rot);
    SFP.sentListener = SFP.written;
    SFP.recordOwnTop = true;                    // this write's step lands when the callback ends
    SFP.folded++;
    var ms = sfpNow() - t0;
    if (ms > 40) sfpLog("Folded " + total + " undo steps in " + ms + " ms.");
}

// ── The link ─────────────────────────────────────────────────────────────────

function sfpSend(obj) { SFP.out.push(typeof obj === "string" ? obj : JSON.stringify(obj)); }

function sfpFlush() {
    if (!SFP.out.length || !SFP.proc) return;
    var text = SFP.out.join("\n") + "\n";
    SFP.out = [];
    try { SFP.proc.writeStandardInput(text, 100); } catch (e) { sfpLog("Couldn't write to the view: " + e); }
}

function sfpPoll(w) {
    var proc = SFP.proc, running = false;
    if (!proc) return;
    try { running = proc.isRunning(); } catch (e) { running = false; }
    if (!running) {
        // Never read a finished ScriptProcess (that raises an uncatchable error).
        SFP.proc = null; SFP.ended = true;
        if (SFP.gesture) sfpEndGesture();
        sfpRestoreAll();
        w.findWidget("sfp_status").setText("The First Person view was closed. You can close this window.");
        return;
    }
    SFP.tick++;
    if (SFP.recordOwnTop) { SFP.ownTop = sfpUndoIndex(); SFP.recordOwnTop = false; }

    var chunk = "", err = "", nl, line, msg, move = null, emitters = null;
    try { chunk = "" + (proc.readAllStandardOutput() || ""); } catch (e) { chunk = ""; }
    try { err = "" + (proc.readAllStandardError() || ""); } catch (e) { err = ""; }
    if (err) sfpLog("view: " + err.split("\n").join(" ").trim());
    SFP.buf += chunk;
    while ((nl = SFP.buf.indexOf("\n")) >= 0) {
        line = SFP.buf.substring(0, nl).trim();
        SFP.buf = SFP.buf.substring(nl + 1);
        if (!line || line.charAt(0) !== "{") continue;
        try { msg = JSON.parse(line); } catch (e) { continue; }
        if (msg.cmd === "move" && isFinite(msg.x) && isFinite(msg.z) && isFinite(msg.rot)) move = msg;  // only the latest counts
        if (msg.cmd === "emitters" && msg.set && msg.set.length !== undefined) {
            emitters = {};
            for (var k = 0; k < msg.set.length; k++) {
                var t = msg.set[k];
                if (t && t.id && isFinite(t.x) && isFinite(t.z)) emitters[t.id] = { x: t.x, z: t.z };
            }
        }
    }

    if (!SFP.ready) {
        if (chunk.indexOf("\"ready\"") < 0) { sfpStatus(w, "Starting the First Person view…"); return; }
        SFP.ready = true;
        sfpSend({ type: "hello", studio: studio.version, script: SFP_VERSION, project: studio.project.filePath || "" });
    }

    var sc = sfpPickScene();
    if (sc !== SFP.scene) {
        if (SFP.gesture) sfpEndGesture();
        sfpRestoreAll();                        // emitters of the scene we're leaving
        SFP.scene = sc; SFP.sentScene = ""; SFP.sentListener = ""; SFP.spatial = {}; SFP.ownTop = -1;
    }
    if (!sc) {
        var none = JSON.stringify({ type: "scene", none: true });
        if (SFP.sentScene !== none) { SFP.sentScene = none; sfpSend(none); }
        sfpStatus(w, "Open a Sandbox scene (Window > Sandbox) and click into it.");
        sfpFlush();
        return;
    }

    if ((move || emitters) && SFP.gesture && SFP.gesture.writes >= SFP_FOLD_EVERY) {
        // A long continuous move: fold what we have now (before this tick's
        // write, so every counted step is on the stack), then carry on as a
        // new gesture. Its first step already holds the folded pose.
        sfpEndGesture();
        SFP.ownTop = -1;
    }
    SFP.tickWrote = false;
    var sj = sfpSceneJson(sc);                  // first: spots Sandbox drags of moved emitters
    if (move) sfpApplyMove(move);
    if (emitters) sfpApplyEmitters(emitters);
    if (SFP.tickWrote) { SFP.gesture.writes++; SFP.lastMoveT = sfpNow(); }   // one undo step per writing tick
    else if (SFP.gesture && sfpNow() - SFP.lastMoveT > SFP_GESTURE_END_MS) sfpEndGesture();

    if (sj !== SFP.sentScene) { SFP.sentScene = sj; sfpSend(sj); }
    var L = sfpListener();
    if (L) {
        var pose = sfpR(L.positionX) + "," + sfpR(L.positionZ) + "," + sfpR(L.rotationY);
        if (pose !== SFP.sentListener) {
            SFP.sentListener = pose;
            sfpSend({ type: "listener", x: sfpR(L.positionX), z: sfpR(L.positionZ), rot: sfpR(L.rotationY) });
        }
    }
    sfpFlush();
    sfpStatus(w, L ? "<b>Linked</b> to '" + sc.name + "'. Keep this window open while you use the view." +
        (SFP.folded ? " <span style=\"color:#8a8a8a\">" + SFP.folded + " walks folded into one undo step.</span>" : "")
        : "Scene '" + sc.name + "' has no listener. Add one in the Sandbox.");
}

function sfpStatus(w, text) {
    if (text === SFP.status) return;
    SFP.status = text;
    w.findWidget("sfp_status").setText(text);
}

function sfpOpenLinkWindow() {
    var T = studio.ui.widgetType, Lt = studio.ui.layoutType;
    studio.ui.showModelessDialog({
        windowTitle: "First Person link  v" + SFP_VERSION,
        windowWidth: 380, windowHeight: 110,
        widgetType: T.Layout, layout: Lt.VBoxLayout,
        items: [
            { widgetType: T.Label, widgetId: "sfp_status", wordWrap: true, text: "Starting the First Person view…" },
            { widgetType: T.Label, wordWrap: true, text: "<span style=\"color:#8a8a8a\">This window carries the link between the Sandbox and " +
                "the First Person view. Closing it unlinks the view.</span>" },
            {
                // Last widget, so the status label exists. Owns the polling timer.
                widgetType: T.Spacer,
                onConstructed: function () { try { this.startTimer(15); } catch (e) { sfpLog("Link timer failed: " + e); } },
                onTimerEvent: function () { sfpPoll(this); }
            }
        ]
    });
}

function sfpRunning() {
    try { return !!SFP.proc && SFP.proc.isRunning(); } catch (e) { return false; }
}

function sfpLaunch() {
    if (sfpRunning()) return { error: "The First Person view is already open (look for its window)." };
    var exe = sfpAppPath();
    if (!sfpExists(exe)) {
        return { error: "The First Person view app (" + sfpAppName() + ") isn't installed. Expected it at:\n" + exe +
            "\n\nSee Tools/SandboxFirstPerson/README.md for how to install it." };
    }
    try {
        SFP.proc = studio.system.startAsync(exe, { args: ["--bridge"], workingDir: sfpToolsDir() });
    } catch (e) {
        return { error: "Couldn't start the First Person view: " + e };
    }
    if (!SFP.proc) return { error: "Couldn't start the First Person view (no process)." };
    SFP.buf = ""; SFP.out = []; SFP.ended = false; SFP.tick = 0; SFP.status = "";
    SFP.scene = null; SFP.sentScene = ""; SFP.sentListener = ""; SFP.spatial = {};
    SFP.gesture = null; SFP.ownTop = -1; SFP.recordOwnTop = false; SFP.folded = 0; SFP.skipped = 0;
    SFP.lastDiag = ""; SFP.ready = false; SFP.orig = {}; SFP.moved = {};
    sfpLog("v" + SFP_VERSION + ": started " + exe);
    sfpOpenLinkWindow();
    return { ok: true };
}
