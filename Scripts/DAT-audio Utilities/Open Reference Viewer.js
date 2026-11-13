// Reference Viewer
// Version: 0.5.4
//
// Part of DAT-audio Utilities. For FMOD Studio 2.03, 2.04 and 2.02 (model
// checked headless on 2.03.14 and 2.04.00; 2.02.33 against its schema).
// Menu: Scripts > DAT-audio Utilities > Open Reference Viewer...
//
// A live window beside Studio that follows the selection and shows what is
// connected to it. It never changes the project; it only opens events:
// - an event: the events that use it (chained up) and the events it uses
//   (chained down), with each link's instrument, conditions, automation,
//   modulation, track, sheet and containers, plus each event's master track,
//   macros and its route through the mixer (for the gain estimate);
// - a parameter: every condition, automation, modulation, macro and passed
//   value that uses it;
// - a snapshot: the properties it changes and what drives its intensity.
//
// The window is the companion app Tools/ReferenceViewer/reference_viewer,
// started with studio.system.startAsync and linked over its stdin/stdout. A
// small "Reference Viewer link" window owns the 40 ms timer that carries the link.
//
// One JSON object per line each way:
//   Studio → app: {"type":"hello"|"focus"|"cursors"|"status", ...}
//   app → Studio: {"cmd":"open","id":"{guid}"}
// The app acknowledges every line it reads with an empty line at once:
// writeStandardInput waits for child output and blocks Studio until it gets some.
//
// Findings this relies on (verified headless on 2.03.14 and 2.04.00):
// - Event.references lists the EventSounds that use an event (not snapshot
//   instruments, which are in Snapshot.references). EventSound.event is the
//   REFERENCED event; the containing event is found by walking up (rfvEventOf).
// - GameParameter.parameterConditions / automationCurves / proxies[].references
//   give every use of a parameter without scanning the project.
// - GameParameter.cursorPosition is the parameter's current value (one per
//   parameter, not per event).
// - Objects are compared by .id (a new wrapper per lookup); every property read
//   is wrapped, since unknown names fail silently or throw.

var RFV_VERSION = "0.5.4";
var RFV_TITLE = "Reference Viewer";
var rfvScriptDir = __dirname;       // valid only while this file loads

var RFV_TIMER_MS = 40;
var RFV_REREAD_EVERY = 25;          // ticks between re-reads of the focus (~1 s), to catch edits
var RFV_MAX_DEPTH = 8;              // hops up or down from the focus
var RFV_MAX_NODES = 300;            // events per graph, so huge projects stay responsive
var RFV_MAX_CURSORS = 200;          // parameters whose live value is sent every tick

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Open Reference Viewer...",
    execute: function () { var m = rfvLaunch(); if (m.error) alert(m.error); }
});

var RFV = {
    proc: null, buf: "", out: [], ready: false, tick: 0, gen: 0, status: "",
    focus: null,            // { kind, obj, key }
    sentFocus: "", sentCursors: "",
    params: {},             // id -> GameParameter, for the live cursor values
    navigate: function (obj) { return studio.window.navigateTo(obj); }   // replaced in tests
};

function rfvLog(s) { studio.system.print("[" + RFV_TITLE + "] " + s); }
function rfvIsWindows() {
    try { if (("" + studio.application.filePath).toLowerCase().indexOf(".exe") >= 0) return true; } catch (e) { }
    try { return !!studio.os && /^win/i.test("" + studio.os.platform); } catch (e) { return false; }
}
function rfvExists(path) { try { return studio.system.getFile(path).exists(); } catch (e) { return false; } }

// ── Safe model access ────────────────────────────────────────────────────────

function rfvGet(o, k) { try { var v = o[k]; return v === undefined ? null : v; } catch (e) { return null; } }
function rfvIs(o, t) { try { return !!o && typeof o.isOfType === "function" && o.isOfType(t); } catch (e) { return false; } }
function rfvList(o, k) { var v = rfvGet(o, k); return v && typeof v.length === "number" ? v : []; }
function rfvId(o) { return o ? "" + rfvGet(o, "id") : ""; }
function rfvR(v) { return typeof v === "number" && isFinite(v) ? Math.round(v * 10000) / 10000 : v; }
function rfvNum(o, k) { var v = rfvGet(o, k); return typeof v === "number" ? rfvR(v) : null; }
function rfvEntity(o) { return "" + (rfvGet(o, "entity") || ""); }

function rfvPath(ev) {
    if (!ev) return "";
    try { return "" + ev.getPath(); } catch (e) { }
    return "" + (rfvGet(ev, "name") || "");
}
// Studio's colour name ("Default", "Red", "Blue Light 1"…), or "" when the object has none.
function rfvColor(o) { var c = rfvGet(o, "color"); return typeof c === "string" ? c : ""; }
function rfvEventRef(ev) { return ev ? { id: rfvId(ev), name: "" + (rfvGet(ev, "name") || ""), path: rfvPath(ev), snapshot: rfvIs(ev, "Snapshot"), color: rfvColor(ev) } : null; }

// Parameter name as Studio shows it: the preset's name, else the parameter's.
function rfvParamName(gp) {
    if (!gp) return "(missing parameter)";
    var owner = rfvGet(gp, "presetOwner"), n = owner ? rfvGet(owner, "name") : null;
    if (!n) n = rfvGet(gp, "name");
    return n ? "" + n : rfvId(gp);
}

// A GameParameter behind any parameter-ish object (a sheet's proxy, a preset).
function rfvGameParam(p) {
    if (!p) return null;
    if (rfvIs(p, "GameParameter")) return p;
    if (rfvIs(p, "ParameterProxy")) return rfvGet(p, "preset");
    if (rfvIs(p, "ParameterPreset")) return rfvGet(p, "parameter");
    return null;
}

// A reference to a parameter (GameParameter or the event's Timeline) that the
// app can look up in the focus message's `params` table.
function rfvParamRef(p) {
    if (!p) return null;
    if (rfvIs(p, "Timeline")) return { id: rfvId(p), name: "Timeline", timeline: true };
    var gp = rfvGameParam(p);
    if (!gp) return { id: rfvId(p), name: rfvEntity(p) };
    var id = rfvId(gp);
    RFV.params[id] = gp;
    return { id: id, name: rfvParamName(gp) };
}

function rfvParamInfo(gp) {
    var labels = rfvGet(gp, "enumerationLabels"), l = [], i;
    if (labels) for (i = 0; i < labels.length; i++) l.push("" + labels[i]);
    return {
        id: rfvId(gp), name: rfvParamName(gp), type: rfvNum(gp, "parameterType"),
        min: rfvNum(gp, "minimum"), max: rfvNum(gp, "maximum"), initial: rfvNum(gp, "initialValue"),
        cursor: rfvNum(gp, "cursorPosition"), labels: l, global: !!rfvGet(gp, "isGlobal"),
        readOnly: !!rfvGet(gp, "isReadOnly"), hold: !!rfvGet(gp, "isHeld"), velocity: rfvNum(gp, "velocity")
    };
}

// Plain settings of an object (numbers and booleans), for the detail panel.
var RFV_SKIP_PROPS = { name: 1, color: 1, note: 1, parent: 1, toString: 1, dump: 1 };
function rfvSettings(o, max) {
    var out = {}, n = 0, props = rfvGet(o, "properties"), k, v;
    if (!props) return out;
    for (k in props) {
        if (RFV_SKIP_PROPS[k] || k.indexOf("ui") === 0) continue;
        v = rfvGet(o, k);
        if (typeof v === "number") v = rfvR(v);
        else if (typeof v !== "boolean") continue;
        out[k] = v;
        if (++n >= (max || 40)) break;
    }
    return out;
}

// ── Where things are ─────────────────────────────────────────────────────────

// The event that CONTAINS an object. Never reads `.event` off a Sound: on an
// event or snapshot instrument that is the referenced event, not the parent.
function rfvEventOf(o) {
    var guard = 0, next;
    while (o && guard++ < 40) {
        if (rfvIs(o, "Event")) return o;
        next = null;
        if (rfvIs(o, "Modulator")) next = rfvGet(o, "objectBeingModulated");
        else if (rfvIs(o, "TriggerCondition") || rfvIs(o, "ParameterProperty")) next = rfvGet(o, "owner");
        else if (rfvIs(o, "SnapshotProperty")) next = rfvGet(o, "snapshot");
        else if (rfvIs(o, "EventAutomatableProperties")) next = rfvGet(o, "event");
        else if (rfvIs(o, "MixerEffect")) next = rfvGet(o, "owner");
        else if (rfvIs(o, "EffectChain")) next = rfvGet(o, "bus") || rfvGet(o, "owner");
        else if (rfvIs(o, "EventMixerGroup") || rfvIs(o, "EventMixerMaster")) next = rfvGet(o, "groupTrack");
        else if (rfvIs(o, "EventMixerReturn")) next = rfvGet(o, "returnTrack");
        else if (rfvIs(o, "SnapshotMasterTrack") || rfvIs(o, "SnapshotTrack")) next = rfvGet(o, "snapshot");
        else if (rfvIs(o, "Track") || rfvIs(o, "Parameter") || rfvIs(o, "MixerInput")) next = rfvGet(o, "event");
        else if (rfvIs(o, "Module") || rfvIs(o, "MultiSound")) {
            next = rfvGet(o, "owner") || rfvGet(o, "scattererOwner") || rfvGet(o, "audioTrack") || rfvGet(o, "parameter");
        }
        else if (rfvIs(o, "AutomationCurve")) next = rfvGet(rfvGet(o, "automator"), "objectBeingAutomated");
        o = next;
    }
    return null;
}

var RFV_KIND_NAMES = {
    EventSound: "Event instrument", SnapshotModule: "Snapshot instrument", SingleSound: "Single instrument",
    MultiSound: "Multi instrument", SoundScatterer: "Scatterer instrument", ProgrammerSound: "Programmer instrument",
    CommandSound: "Command instrument", PluginSound: "Plug-in instrument", SilenceSound: "Silence instrument",
    EventMixerGroup: "Track", EventMixerMaster: "Master track", EventMixerReturn: "Return track",
    MixerGroup: "Bus", MixerMaster: "Master Bus", MixerReturn: "Return bus", MixerVCA: "VCA", MixerInput: "Event input",
    EventAutomatableProperties: "Event macros", ParameterCondition: "Parameter condition", EventCondition: "Event condition",
    ParameterProperty: "Passed parameter value", SnapshotProperty: "Snapshot property", GainEffect: "Gain",
    MixerSend: "Send", MixerBusFader: "Fader", SpatialiserEffect: "Spatializer", ObjectSpatialiserEffect: "Object spatializer",
    AudioFile: "Asset", EffectPreset: "Shared effect", ProxyEffect: "Shared effect"
};

function rfvKind(o) {
    var e = rfvEntity(o);
    if (RFV_KIND_NAMES[e]) return RFV_KIND_NAMES[e];
    if (/Modulator$/.test(e)) return e.replace(/Modulator$/, "") + " modulator";
    if (/Effect$/.test(e)) return e.replace(/Effect$/, "");
    return e || "Object";
}

function rfvBaseName(p) { p = "" + p; return p.substring(Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")) + 1); }

function rfvObjectName(o) {
    var n = rfvGet(o, "name");
    if (n) return "" + n;
    if (rfvIs(o, "AudioFile")) return rfvBaseName(rfvGet(o, "assetPath") || "");
    if (rfvIs(o, "ProxyEffect")) return "" + (rfvGet(rfvGet(o, "preset"), "name") || "");
    if (rfvIs(o, "SingleSound")) {
        var p = rfvGet(rfvGet(o, "audioFile"), "assetPath");
        if (p) { p = "" + p; return p.substring(Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")) + 1); }
    }
    if (rfvIs(o, "EventSound")) return "" + (rfvGet(rfvGet(o, "event"), "name") || "");
    return "";
}

function rfvDescribe(o) {
    if (!o) return "(nothing)";
    var n = rfvObjectName(o);
    return rfvKind(o) + (n ? " '" + n + "'" : "");
}

// ── Automation, modulation, conditions ───────────────────────────────────────

function rfvPoints(curve) {
    var pts = rfvList(curve, "automationPoints"), out = [], i, p;
    for (i = 0; i < pts.length; i++) {
        p = pts[i];
        out.push([rfvNum(p, "position"), rfvNum(p, "value"), rfvNum(p, "curveShape") || 0, !!rfvGet(p, "isSCurve")]);
    }
    out.sort(function (a, b) { return a[0] - b[0]; });
    return out;
}

function rfvAutomation(o) {
    var autos = rfvList(o, "automators"), out = [], i, j, curves, cs;
    for (i = 0; i < autos.length; i++) {
        curves = rfvList(autos[i], "automationCurves"); cs = [];
        for (j = 0; j < curves.length; j++) cs.push({ param: rfvParamRef(rfvGet(curves[j], "parameter")), points: rfvPoints(curves[j]) });
        out.push({ property: "" + rfvGet(autos[i], "nameOfPropertyBeingAutomated"), curves: cs });
    }
    return out;
}

// Settings per modulator type (from the Copy-Paste Conditions tool, object-model
// XML 2.02.33 + 2.03.14). Names a build doesn't have read as null and are dropped.
var RFV_MOD_PROPS = {
    RandomizerModulator: ["amount"],
    SeekModulator: ["speedAscending", "speedDescending", "asymmetric"],
    ADSRModulator: ["initialValue", "attackTime", "attackShape", "peakValue", "holdTime", "decayTime",
        "decayShape", "sustainValue", "releaseTime", "releaseShape", "finalValue"],
    SidechainModulator: ["levelMode", "amount", "attackTime", "releaseTime", "minimumThreshold", "maximumThreshold"],
    LFOModulator: ["shape", "isTempoSync", "rate", "beats", "phase", "depth", "direction"],
    AutopitchModulator: ["root", "pitchAtMinimum"],
    SpectralSidechainModulator: ["mode", "amount", "attackTime", "releaseTime", "minimumThresholdLevel",
        "maximumThresholdLevel", "minimumThresholdFrequency", "maximumThresholdFrequency"]
};

function rfvModulation(o) {
    var mods = rfvList(o, "modulators"), out = [], i, j, m, names, s, v;
    for (i = 0; i < mods.length; i++) {
        m = mods[i]; names = RFV_MOD_PROPS[rfvEntity(m)]; s = {};
        if (names) { for (j = 0; j < names.length; j++) { v = rfvGet(m, names[j]); if (typeof v === "number" || typeof v === "boolean") s[names[j]] = rfvR(v); } }
        else s = rfvSettings(m);
        v = rfvGet(m, "clockSource"); if (typeof v === "number") s.clockSource = v;
        out.push({ id: rfvId(m), type: rfvEntity(m), kind: rfvKind(m), property: "" + rfvGet(m, "nameOfPropertyBeingModulated"),
            settings: s, automation: rfvAutomation(m) });
    }
    return out;
}

function rfvConditions(owner) {
    var cs = rfvList(owner, "triggerConditions"), out = [], i, c, gp, labels, l, j;
    for (i = 0; i < cs.length; i++) {
        c = cs[i];
        if (rfvIs(c, "ParameterCondition")) {
            gp = rfvGet(c, "parameter");
            labels = rfvGet(gp, "enumerationLabels"); l = [];
            if (labels) for (j = 0; j < labels.length; j++) l.push("" + labels[j]);
            out.push({ kind: "parameter", param: rfvParamRef(gp), min: rfvNum(c, "minimum"), max: rfvNum(c, "maximum"),
                inverted: !!rfvGet(c, "isInverted"), labels: l, automation: rfvAutomation(c) });
        } else if (rfvIs(c, "EventCondition")) {
            out.push({ kind: "event", eventState: rfvNum(c, "eventState") });
        } else out.push({ kind: rfvEntity(c) });
    }
    return out;
}

// ── Instruments, containers, tracks, effects, mixer strips ───────────────────

function rfvInstrument(m) {
    var r = {
        id: rfvId(m), type: rfvEntity(m), kind: rfvKind(m), name: rfvObjectName(m),
        volume: rfvNum(m, "volume"), pitch: rfvNum(m, "pitch"),
        probabilityOn: !!rfvGet(m, "triggerProbabilityEnabled"), probability: rfvNum(m, "triggerProbability"),
        conditionMode: rfvNum(m, "triggerConditionMode"), conditions: rfvConditions(m),
        delay: { type: rfvNum(m, "delayType"), min: rfvNum(m, "minimumTimeDelay"), max: rfvNum(m, "maximumTimeDelay") },
        start: rfvNum(m, "start"), length: rfvNum(m, "length"), startOffset: rfvNum(m, "startOffset"),
        async: !!rfvGet(m, "isAsync"), cutoff: !!rfvGet(m, "isCutoff"), looping: !!rfvGet(m, "looping"),
        quantization: rfvNum(m, "quantizationInterval"), maxVoices: rfvNum(m, "maxVoices"),
        automation: rfvAutomation(m), modulation: rfvModulation(m)
    };
    if (rfvIs(m, "EventSound")) {
        var ps = rfvList(m, "parameters"), i, pp, proxy;
        r.params = [];
        for (i = 0; i < ps.length; i++) {
            pp = ps[i]; proxy = rfvGet(pp, "parameter");
            r.params.push({ param: rfvParamRef(proxy), value: rfvNum(pp, "position"), automation: rfvAutomation(pp), modulation: rfvModulation(pp) });
        }
    }
    if (rfvIs(m, "SnapshotModule")) r.intensity = rfvNum(m, "intensity");
    return r;
}

// Multi and Scatterer instruments an instrument sits inside, innermost first.
// Each carries its own instrument details (volume, conditions…), which also
// apply on the way.
function rfvContainers(m) {
    var out = [], cur = m, guard = 0, owner, sc, pp;
    while (cur && guard++ < 16) {
        owner = rfvGet(cur, "owner");
        if (owner && rfvIs(owner, "MultiSound")) {
            pp = rfvGet(cur, "playPercentage");
            var c = rfvInstrument(owner);
            c.container = "Multi"; c.entries = rfvList(owner, "sounds").length;
            c.playlistMode = rfvNum(owner, "playlistMode"); c.playbackMode = rfvNum(owner, "playbackMode");
            c.entryPercentage = pp ? rfvNum(pp, "percentage") : null;
            sc = rfvGet(owner, "scattererOwner");
            if (sc) {
                c.container = "Scatterer playlist";
                out.push(c);
                var s = rfvInstrument(sc);
                s.container = "Scatterer"; s.settings = rfvSettings(sc);
                out.push(s);
                cur = sc;
                continue;
            }
            out.push(c);
            cur = owner;
            continue;
        }
        break;
    }
    return out;
}

function rfvOutermost(m) {
    var cur = m, guard = 0, next;
    while (cur && guard++ < 16) {
        next = rfvGet(cur, "owner");
        if (next && rfvIs(next, "MultiSound")) { cur = rfvGet(next, "scattererOwner") || next; continue; }
        break;
    }
    return cur;
}

function rfvSheet(m) {
    var outer = rfvOutermost(m), p = rfvGet(outer, "parameter");
    var r = { start: rfvNum(outer, "start"), length: rfvNum(outer, "length") };
    if (rfvIs(p, "Timeline")) r.kind = "timeline";
    else if (rfvIs(p, "ParameterProxy")) { r.kind = "parameter"; r.param = rfvParamRef(p); }
    else r.kind = rfvEntity(p) || "unknown";
    return r;
}

function rfvEffects(chain) {
    var fx = rfvList(chain, "effects"), out = [], i, f, real, e;
    for (i = 0; i < fx.length; i++) {
        f = fx[i];
        real = f;
        e = { id: rfvId(f), bypass: !!rfvGet(f, "bypass") };
        if (rfvIs(f, "ProxyEffect")) {
            var preset = rfvGet(f, "preset");
            e.preset = "" + (rfvGet(preset, "name") || "");
            real = rfvGet(preset, "effect") || f;
        }
        e.type = rfvEntity(real); e.kind = rfvKind(real); e.name = "" + (rfvGet(real, "name") || "");
        if (rfvIs(real, "MixerBusFader")) e.fader = true;
        if (rfvIs(real, "GainEffect")) e.gain = rfvNum(real, "gain");
        if (rfvIs(real, "MixerSend")) { e.level = rfvNum(real, "level"); e.to = "" + (rfvGet(rfvGet(real, "mixerReturn"), "name") || ""); }
        if (rfvIs(real, "SpatialEffect")) {
            e.spatial = { override: !!rfvGet(real, "overrideRange"), min: rfvNum(real, "minimumDistance"), max: rfvNum(real, "maximumDistance"),
                rolloff: rfvNum(real, "distanceRolloffType") };
        }
        e.settings = rfvSettings(real);
        e.automation = rfvAutomation(f).concat(real !== f ? rfvAutomation(real) : []);
        e.modulation = rfvModulation(f).concat(real !== f ? rfvModulation(real) : []);
        e.snapshots = rfvSnapshotsOn(f).concat(real !== f ? rfvSnapshotsOn(real) : []);
        out.push(e);
    }
    return out;
}

// Snapshots that change this object: property overrides, and (for mixer
// strips) snapshots that scope the strip.
function rfvSnapshotsOn(o) {
    var out = [], sps = rfvList(o, "snapshotProperties"), i, sp, sn, tr;
    for (i = 0; i < sps.length; i++) {
        sp = sps[i]; sn = rfvGet(sp, "snapshot");
        out.push({ snapshot: rfvEventRef(sn), property: "" + rfvGet(sp, "propertyName"), value: rfvNum(sp, "value"),
            intensity: rfvNum(rfvGet(sn, "automatableProperties"), "snapshotIntensity"), automation: rfvAutomation(sp) });
    }
    tr = rfvList(o, "snapshotTracks");
    for (i = 0; i < tr.length; i++) out.push({ snapshot: rfvEventRef(rfvGet(tr[i], "snapshot")), scoped: true });
    return out;
}

function rfvVcas(strip) {
    var vs = rfvList(strip, "masters"), out = [], i;
    for (i = 0; i < vs.length; i++) {
        out.push({ id: rfvId(vs[i]), name: "" + (rfvGet(vs[i], "name") || ""), volume: rfvNum(vs[i], "volume"),
            automation: rfvAutomation(vs[i]), snapshots: rfvSnapshotsOn(vs[i]) });
    }
    return out;
}

function rfvStrip(s) {
    if (!s) return null;
    return {
        id: rfvId(s), type: rfvEntity(s), kind: rfvKind(s), name: rfvObjectName(s), color: rfvColor(s),
        volume: rfvNum(s, "volume"), pitch: rfvNum(s, "pitch"),
        automation: rfvAutomation(s), modulation: rfvModulation(s),
        effects: rfvEffects(rfvGet(s, "effectChain")), vcas: rfvVcas(s), snapshots: rfvSnapshotsOn(s)
    };
}

function rfvTrackOf(m) {
    var t = rfvGet(rfvOutermost(m), "audioTrack");
    if (!t) return null;
    var mg = rfvGet(t, "mixerGroup");
    var r = { id: rfvId(t), type: rfvEntity(t), name: "" + (rfvGet(mg, "name") || "") };
    if (rfvIs(t, "MasterTrack")) { r.master = true; r.name = "Master"; }   // shown on the parent node
    else r.strip = rfvStrip(mg);
    return r;
}

// The event's route through the mixer: its input strip, then each bus to the Master Bus.
function rfvRoute(ev) {
    var out = [], s = rfvGet(ev, "mixerInput"), guard = 0;
    while (s && guard++ < 32) {
        out.push(rfvStrip(s));
        s = rfvGet(s, "output");
    }
    return out;
}

function rfvMacros(ev) {
    var a = rfvGet(ev, "automatableProperties");
    if (!a) return null;
    return { settings: rfvSettings(a), automation: rfvAutomation(a), modulation: rfvModulation(a) };
}

// Every instrument in an event, recursing into Multi and Scatterer playlists.
function rfvModules(ev) {
    var tracks = [rfvGet(ev, "masterTrack")].concat(rfvToArray(rfvList(ev, "groupTracks")), rfvToArray(rfvList(ev, "returnTracks")));
    var out = [], i, j;
    function walk(m) {
        if (!m) return;
        out.push(m);
        if (rfvIs(m, "MultiSound")) { var ss = rfvList(m, "sounds"); for (var k = 0; k < ss.length; k++) walk(ss[k]); }
        if (rfvIs(m, "SoundScatterer")) walk(rfvGet(m, "sound"));
    }
    for (i = 0; i < tracks.length; i++) {
        var ms = rfvList(tracks[i], "modules");
        for (j = 0; j < ms.length; j++) walk(ms[j]);
    }
    return out;
}
function rfvToArray(l) { var a = [], i; for (i = 0; i < l.length; i++) a.push(l[i]); return a; }

// Event instruments (EventSound, not snapshot instruments) in an event.
function rfvEventSounds(ev) {
    var all = rfvModules(ev), out = [], i;
    for (i = 0; i < all.length; i++) if (rfvIs(all[i], "EventSound") && !rfvIs(all[i], "SnapshotModule")) out.push(all[i]);
    return out;
}

function rfvSnapshotInstruments(ev) {
    var all = rfvModules(ev), out = [], i, r;
    for (i = 0; i < all.length; i++) {
        if (!rfvIs(all[i], "SnapshotModule")) continue;
        r = rfvInstrument(all[i]);
        r.snapshot = rfvEventRef(rfvGet(all[i], "event"));
        r.track = rfvTrackOf(all[i]);
        out.push(r);
    }
    return out;
}

// ── The graph ────────────────────────────────────────────────────────────────
// Nodes are events, assets, mixer strips (buses, returns, VCAs), event tracks
// and shared effects. Edges are in signal-flow terms: childId is the source,
// parentId where it goes (a child event plays into the event that uses it, an
// asset into its event, a bus into its output, a send into its return, a
// shared effect into what it's on). `kind` says which. `depth` is negative
// toward destinations and positive toward sources; the app lays out by flow.

function rfvNode(ev) {
    var mt = rfvGet(ev, "masterTrack");
    var n = rfvEventRef(ev);
    n.master = rfvStrip(rfvGet(mt, "mixerGroup"));
    n.macros = rfvMacros(ev);
    n.snapshots = rfvSnapshotInstruments(ev);
    n.usedBy = 0;
    try { n.usedBy = rfvUsers(ev).length; } catch (e) { n.usedBy = 0; }
    // Banks it is assigned to directly (not through a parent event).
    var bs = rfvList(ev, "banks"), i;
    n.banks = [];
    for (i = 0; i < bs.length; i++) n.banks.push("" + (rfvGet(bs[i], "name") || ""));
    return n;
}

function rfvUsers(ev) {
    var refs = rfvList(ev, "references"), out = [], i;
    for (i = 0; i < refs.length; i++) if (!rfvIs(refs[i], "SnapshotModule")) out.push(refs[i]);
    return out;
}

// An event-internal strip (a track, the master track, a return track)?
function rfvInEvent(s) { return rfvIs(s, "EventMixerGroup") || rfvIs(s, "EventMixerMaster") || rfvIs(s, "EventMixerReturn"); }

function rfvNewGraph(foci) {
    var g = { nodes: {}, objs: {}, order: [], seen: {}, list: [], truncated: false, focus: {} }, i;
    for (i = 0; i < foci.length; i++) g.focus[rfvId(foci[i])] = true;
    return g;
}

function rfvAddNode(g, o, kind, depth) {
    var id = rfvId(o);
    if (g.nodes[id]) return g.nodes[id];
    if (g.order.length >= RFV_MAX_NODES) { g.truncated = true; return null; }
    var n;
    if (kind === "event") n = rfvNode(o);
    else n = { id: id, name: rfvObjectName(o) || rfvKind(o), path: "" };
    n.kind = kind; n.isFocus = !!g.focus[id]; n.depth = depth; n.label = rfvKind(o); n.color = rfvColor(o);
    if (kind === "asset") {
        n.path = "" + (rfvGet(o, "assetPath") || "");
        n.asset = { length: rfvNum(o, "length"), channels: rfvNum(o, "channelCount"), rate: rfvNum(o, "frequencyInKHz"),
            streaming: !!rfvGet(o, "isStreaming"), uses: rfvList(o, "sounds").length };
    }
    if (kind === "bus" || kind === "return" || kind === "vca" || kind === "track") n.strip = rfvStrip(o);
    if (kind === "track") {
        var ev = rfvEventOf(o);
        n.event = rfvEventRef(ev);
        n.name = (ev ? rfvGet(ev, "name") + " · " : "") + (rfvIs(o, "EventMixerMaster") ? "Master" : rfvObjectName(o) || rfvKind(o));
        n.path = rfvPath(ev);
    }
    if (kind === "vca") n.slaves = rfvList(o, "slaves").length;
    if (kind === "preset") {
        var fx = rfvGet(o, "effect");
        n.effect = { type: rfvEntity(fx), kind: rfvKind(fx), settings: rfvSettings(fx) };
        n.uses = rfvList(o, "proxies").length;
    }
    g.nodes[id] = n; g.objs[id] = o; g.order.push(id);
    return n;
}

function rfvAddEdge(g, id, kind, child, parent, extra) {
    if (g.seen[id]) return;
    g.seen[id] = true;
    var e = extra || {};
    e.id = id; e.kind = kind; e.childId = rfvId(child); e.parentId = rfvId(parent);
    g.list.push(e);
}

function rfvEventEdge(m) { return { instrument: rfvInstrument(m), containers: rfvContainers(m), track: rfvTrackOf(m), sheet: rfvSheet(m) }; }

// Up from these events: the events that use them, and so on.
function rfvEventsUp(g, starts) {
    var queue = [], seen = {}, q, users, i, parent;
    for (i = 0; i < starts.length; i++) { queue.push(starts[i]); seen[rfvId(starts[i].ev)] = true; }
    while (queue.length) {
        q = queue.shift();
        users = rfvUsers(q.ev);
        for (i = 0; i < users.length; i++) {
            parent = rfvEventOf(users[i]);
            if (!parent) continue;
            if (Math.abs(q.d - 1) > RFV_MAX_DEPTH) { g.truncated = true; continue; }
            if (!rfvAddNode(g, parent, "event", q.d - 1)) continue;
            rfvAddEdge(g, rfvId(users[i]), "event", q.ev, parent, rfvEventEdge(users[i]));
            if (!seen[rfvId(parent)]) { seen[rfvId(parent)] = true; queue.push({ ev: parent, d: q.d - 1 }); }
        }
    }
}

// Down from these events: the events they use, and so on.
function rfvEventsDown(g, starts) {
    var queue = [], seen = {}, q, sounds, i, child;
    for (i = 0; i < starts.length; i++) { queue.push(starts[i]); seen[rfvId(starts[i].ev)] = true; }
    while (queue.length) {
        q = queue.shift();
        sounds = rfvEventSounds(q.ev);
        for (i = 0; i < sounds.length; i++) {
            child = rfvGet(sounds[i], "event");
            if (!child) continue;
            if (q.d + 1 > RFV_MAX_DEPTH) { g.truncated = true; continue; }
            if (!rfvAddNode(g, child, "event", q.d + 1)) continue;
            rfvAddEdge(g, rfvId(sounds[i]), "event", child, q.ev, rfvEventEdge(sounds[i]));
            if (!seen[rfvId(child)]) { seen[rfvId(child)] = true; queue.push({ ev: child, d: q.d + 1 }); }
        }
    }
}

// The node for a mixer strip: the event for an event's input, a track node for
// strips inside an event, else the bus/return/VCA itself.
function rfvStripNode(g, s, depth) {
    if (rfvIs(s, "MixerInput")) { var ev = rfvGet(s, "event"); return ev ? { node: rfvAddNode(g, ev, "event", depth), obj: ev } : null; }
    var kind = rfvInEvent(s) ? "track" : rfvIs(s, "MixerVCA") ? "vca" : rfvIs(s, "MixerReturn") ? "return" : "bus";
    return { node: rfvAddNode(g, s, kind, depth), obj: s };
}

// The strip a send or an effect sits on.
function rfvChainOwner(fx) { var c = rfvGet(fx, "owner"); return rfvGet(c, "bus") || rfvGet(c, "owner"); }

function rfvSendEdge(fx, owner) {
    var e = rfvEffects({ effects: [fx] })[0] || {};
    return { send: e, place: rfvDescribe(owner) };
}

// A strip's outputs (its output chain to the Master Bus) and its sends.
function rfvStripOutputs(g, s, depth) {
    var cur = s, d = depth, out, guard = 0, fx, i, ret, rn;
    var from = rfvStripNode(g, s, depth);
    if (!from || !from.node) return;
    var fxs = rfvList(rfvGet(s, "effectChain"), "effects");
    for (i = 0; i < fxs.length; i++) {
        if (!rfvIs(fxs[i], "MixerSend")) continue;
        ret = rfvGet(fxs[i], "mixerReturn");
        rn = ret ? rfvStripNode(g, ret, depth - 1) : null;
        if (rn && rn.node) rfvAddEdge(g, rfvId(fxs[i]), "send", from.obj, ret, rfvSendEdge(fxs[i], s));
    }
    while (cur && guard++ < 32) {
        out = rfvGet(cur, "output");
        if (!out) break;
        d--;
        var on = rfvStripNode(g, out, d);
        if (!on || !on.node) break;
        rfvAddEdge(g, rfvId(cur) + ">" + rfvId(out), "route", rfvIs(cur, "MixerInput") ? rfvGet(cur, "event") : cur, out, {});
        cur = out;
    }
}

// What feeds a strip: buses and event inputs routed into it, sends into a
// return, recursively.
function rfvStripInputs(g, s, depth) {
    if (depth > RFV_MAX_DEPTH) { g.truncated = true; return; }
    var ins = rfvList(s, "input"), i, x, owner, n;
    for (i = 0; i < ins.length; i++) {
        x = ins[i];
        if (rfvIs(x, "MixerSend")) {
            owner = rfvChainOwner(x);
            n = owner ? rfvStripNode(g, owner, depth) : null;
            if (!n || !n.node) continue;
            rfvAddEdge(g, rfvId(x), "send", n.obj, s, rfvSendEdge(x, owner));
        } else {
            n = rfvStripNode(g, x, depth);
            if (!n || !n.node) continue;
            rfvAddEdge(g, rfvId(x) + ">" + rfvId(s), "route", n.obj, s, {});
            if (!rfvIs(x, "MixerInput")) rfvStripInputs(g, x, depth + 1);
        }
    }
}

function rfvBuildGraph(sel) {
    var g = rfvNewGraph(sel.objs), i, o, starts = [], j, ev, sounds, proxies, owner, on;
    for (i = 0; i < sel.objs.length; i++) {
        o = sel.objs[i];
        if (sel.kind === "event") {
            rfvAddNode(g, o, "event", 0);
            starts.push({ ev: o, d: 0 });
        } else if (sel.kind === "asset") {
            rfvAddNode(g, o, "asset", 0);
            sounds = rfvList(o, "sounds");
            for (j = 0; j < sounds.length; j++) {
                ev = rfvEventOf(sounds[j]);
                if (!ev || !rfvAddNode(g, ev, "event", -1)) continue;
                rfvAddEdge(g, rfvId(sounds[j]), "asset", o, ev, rfvEventEdge(sounds[j]));
                starts.push({ ev: ev, d: -1 });
            }
        } else if (sel.kind === "preset") {
            rfvAddNode(g, o, "preset", 0);
            proxies = rfvList(o, "proxies");
            for (j = 0; j < proxies.length; j++) {
                owner = rfvChainOwner(proxies[j]);
                if (!owner) continue;
                ev = rfvInEvent(owner) ? rfvEventOf(owner) : null;
                on = ev ? { node: rfvAddNode(g, ev, "event", -1), obj: ev } : rfvStripNode(g, owner, -1);
                if (!on || !on.node) continue;
                var fx = rfvEffects({ effects: [proxies[j]] })[0] || {};
                rfvAddEdge(g, rfvId(proxies[j]), "effect", o, on.obj, { effect: fx, place: rfvDescribe(owner) + (ev ? " in " + rfvGet(ev, "name") : "") });
                if (ev) starts.push({ ev: ev, d: -1 });
                else rfvStripOutputs(g, owner, -1);
            }
        } else if (sel.kind === "send") {
            var ret = rfvGet(o, "mixerReturn");
            if (ret) { g.focus[rfvId(ret)] = true; rfvStripNode(g, ret, 0); rfvStripInputs(g, ret, 1); rfvStripOutputs(g, ret, 0); }
            g.highlight = rfvId(o);
        } else if (sel.kind === "vca") {
            rfvAddNode(g, o, "vca", 0);
            var sl = rfvList(o, "slaves");
            for (j = 0; j < sl.length; j++) {
                on = rfvStripNode(g, sl[j], 1);
                if (on && on.node) rfvAddEdge(g, rfvId(sl[j]) + ">" + rfvId(o), "vca", on.obj, o, {});
            }
        } else {   // bus, return
            rfvStripNode(g, o, 0);
            rfvStripInputs(g, o, 1);
            rfvStripOutputs(g, o, 0);
        }
    }
    if (starts.length) {
        rfvEventsUp(g, starts);
        if (sel.kind === "event") rfvEventsDown(g, starts);
    }
    // Mixer routes for events: the selected ones, and every event nothing uses (a top-level event).
    var list = [], id, ids = [], n;
    for (i = 0; i < g.order.length; i++) {
        id = g.order[i]; n = g.nodes[id];
        if (n.kind === "event" && (n.isFocus || n.usedBy === 0)) n.route = rfvRoute(g.objs[id]);
        if (n.isFocus) ids.push(id);
        list.push(n);
    }
    return { mode: sel.kind, focusId: ids[0] || "", focusIds: ids, highlight: g.highlight || null, nodes: list, edges: g.list,
        truncated: g.truncated, maxDepth: RFV_MAX_DEPTH, maxNodes: RFV_MAX_NODES };
}

// ── Parameter mode ───────────────────────────────────────────────────────────

function rfvUseKind(o) {
    if (rfvIs(o, "Modulator")) return "modulation";
    if (rfvIs(o, "EventAutomatableProperties")) return "macro";
    if (rfvIs(o, "SnapshotProperty")) return "snapshot";
    if (rfvIs(o, "ParameterProperty")) return "passed";
    if (rfvIs(o, "TriggerCondition")) return "condition";
    return "automation";
}

function rfvParamUses(gp) {
    var uses = [], i, j, c, o, a, prop, ev, refs;
    var conds = rfvList(gp, "parameterConditions");
    for (i = 0; i < conds.length; i++) {
        c = conds[i]; o = rfvGet(c, "owner");
        uses.push({ kind: "condition", event: rfvEventRef(rfvEventOf(o)), target: rfvDescribe(o), targetId: rfvId(o),
            min: rfvNum(c, "minimum"), max: rfvNum(c, "maximum"), inverted: !!rfvGet(c, "isInverted") });
    }
    var curves = rfvList(gp, "automationCurves");
    for (i = 0; i < curves.length; i++) {
        a = rfvGet(curves[i], "automator"); o = rfvGet(a, "objectBeingAutomated");
        prop = "" + rfvGet(a, "nameOfPropertyBeingAutomated");
        var u = { kind: rfvUseKind(o), event: rfvEventRef(rfvEventOf(o)), target: rfvDescribe(o), targetId: rfvId(o),
            property: prop, points: rfvPoints(curves[i]) };
        if (u.kind === "modulation") {
            var mt = rfvGet(o, "objectBeingModulated");
            u.modulates = rfvDescribe(mt) + " " + rfvGet(o, "nameOfPropertyBeingModulated");
        }
        if (u.kind === "snapshot") u.snapshotProperty = "" + rfvGet(o, "propertyName") + " on " + rfvDescribe(rfvGet(o, "automatableObject"));
        if (!u.event) u.mixer = true;
        uses.push(u);
    }
    var proxies = rfvList(gp, "proxies");
    for (i = 0; i < proxies.length; i++) {
        ev = rfvGet(proxies[i], "event");
        uses.push({ kind: "exposed", event: rfvEventRef(ev), target: "Parameter in " + rfvPath(ev) });
        refs = rfvList(proxies[i], "references");
        for (j = 0; j < refs.length; j++) {
            o = rfvGet(refs[j], "owner");
            uses.push({ kind: "passed", event: rfvEventRef(rfvEventOf(o)), child: rfvEventRef(ev), target: rfvDescribe(o),
                targetId: rfvId(o), value: rfvNum(refs[j], "position"), automation: rfvAutomation(refs[j]) });
        }
    }
    var sb = rfvList(gp, "sandboxParameters").length;
    return { uses: uses, sandbox: sb };
}

// ── Snapshot mode ────────────────────────────────────────────────────────────

function rfvSnapshotInfo(sn) {
    var r = rfvEventRef(sn), i, sp, tgt, base, tr, refs, sm;
    r.behavior = rfvNum(sn, "behavior");
    r.intensity = rfvNum(rfvGet(sn, "automatableProperties"), "snapshotIntensity");
    r.macros = rfvMacros(sn);
    r.properties = [];
    var sps = rfvList(sn, "snapshotProperties");
    for (i = 0; i < sps.length; i++) {
        sp = sps[i]; tgt = rfvGet(sp, "automatableObject");
        base = rfvGet(tgt, "" + rfvGet(sp, "propertyName"));
        r.properties.push({ id: rfvId(sp), property: "" + rfvGet(sp, "propertyName"), value: rfvNum(sp, "value"),
            target: rfvDescribe(tgt), targetId: rfvId(tgt), base: typeof base === "number" ? rfvR(base) : null,
            automation: rfvAutomation(sp), modulation: rfvModulation(sp) });
    }
    r.scoped = [];
    tr = rfvList(sn, "snapshotTracks");
    for (i = 0; i < tr.length; i++) r.scoped.push({ id: rfvId(tr[i]), strip: rfvDescribe(rfvGet(tr[i], "mixerStrip")) });
    r.triggers = [];
    refs = rfvList(sn, "references");
    for (i = 0; i < refs.length; i++) {
        sm = refs[i];
        var t = rfvInstrument(sm);
        t.event = rfvEventRef(rfvEventOf(sm));
        t.track = rfvTrackOf(sm);
        r.triggers.push(t);
    }
    return r;
}

// ── Focus ────────────────────────────────────────────────────────────────────
// Studio has three selections: the browser (events, parameters, assets,
// shared effects, the Mixer window's routing), the editor (event instruments,
// mixer strips) and the deck (the effect panel: sends, effects). The one
// changed most recently drives the view; a click on something the viewer
// can't show (a track, an audio instrument) leaves the view as it is.

var RFV_MAX_FOCI = 25;              // selected items shown together

function rfvClassify(o, src) {
    if (!o) return null;
    if (rfvIs(o, "SnapshotModule")) return src === "browser" ? null : { kind: "snapshot", obj: rfvGet(o, "event") };
    if (rfvIs(o, "EventSound")) return { kind: "event", obj: rfvGet(o, "event") };
    if (rfvIs(o, "Snapshot")) return { kind: "snapshot", obj: o };
    if (rfvIs(o, "Event")) return { kind: "event", obj: o };
    if (rfvIs(o, "MixerInput")) return { kind: "event", obj: rfvGet(o, "event") };
    if (rfvIs(o, "AudioFile")) return { kind: "asset", obj: o };
    if (rfvIs(o, "EffectPreset")) return { kind: "preset", obj: o };
    if (rfvIs(o, "ProxyEffect")) return { kind: "preset", obj: rfvGet(o, "preset") };
    // The effect inside a shared effect (what the deck selects when you click it).
    if (rfvIs(o, "MixerEffect") && rfvGet(o, "presetOwner")) return { kind: "preset", obj: rfvGet(o, "presetOwner") };
    if (rfvIs(o, "MixerSend")) return { kind: "send", obj: o };
    if (rfvIs(o, "ReturnTrack")) return { kind: "return", obj: rfvGet(o, "mixerReturn") };
    if (rfvIs(o, "MixerReturn")) return { kind: "return", obj: o };
    if (rfvIs(o, "MixerVCA")) return { kind: "vca", obj: o };
    if (rfvIs(o, "MixerGroup") && !rfvInEvent(o)) return { kind: "bus", obj: o };
    var gp = rfvGameParam(o);
    if (gp) return { kind: "parameter", obj: gp };
    return null;
}

// What a selection list shows: the first usable item's kind, with every
// selected item of that kind (up to RFV_MAX_FOCI). Others are counted.
function rfvSelectionOf(list, src) {
    var kind = null, objs = [], ids = {}, others = {}, i, c, id, n = 0;
    if (!list) return null;
    for (i = 0; i < list.length && i < 200; i++) {
        c = rfvClassify(list[i], src);
        if (!c || !c.obj) continue;
        if (!kind) kind = c.kind;
        if (c.kind !== kind) { others[c.kind] = (others[c.kind] || 0) + 1; continue; }
        id = rfvId(c.obj);
        if (ids[id]) continue;
        ids[id] = true;
        if (objs.length >= RFV_MAX_FOCI) { n++; continue; }
        objs.push(c.obj);
    }
    if (!kind) return null;
    var keys = [];
    for (i = 0; i < objs.length; i++) keys.push(rfvId(objs[i]));
    return { kind: kind, objs: objs, others: others, more: n, source: src, key: kind + ":" + keys.join(",") };
}

RFV.read = function (src) {
    if (src === "browser") return studio.window.browserSelection();
    if (src === "editor") return studio.window.editorSelection();
    return studio.window.deckSelection();
};
RFV.srcKeys = {};

function rfvRawKey(list) {
    var k = [], i;
    if (!list) return "";
    for (i = 0; i < list.length && i < 200; i++) k.push(rfvId(list[i]));
    return k.join(",");
}

// What a selection holds, for the log: "SpatialiserEffect, GroupTrack"…
function rfvDescribeList(list) {
    var out = [], i;
    for (i = 0; i < list.length && i < 5; i++) out.push(rfvEntity(list[i]) + (rfvObjectName(list[i]) ? " '" + rfvObjectName(list[i]) + "'" : ""));
    return out.join(", ") + (list.length > 5 ? " +" + (list.length - 5) : "");
}

function rfvPickFocus() {
    // Deck, then editor, then browser: when several change in one tick, the browser wins.
    var srcs = ["deck", "editor", "browser"], i, list, raw, sel;
    for (i = 0; i < srcs.length; i++) {
        try { list = RFV.read(srcs[i]); } catch (e) { list = null; }
        raw = rfvRawKey(list);
        if (raw === RFV.srcKeys[srcs[i]]) continue;
        RFV.srcKeys[srcs[i]] = raw;
        sel = rfvSelectionOf(list, srcs[i]);
        if (sel) RFV.active = sel;
        else if (list && list.length) rfvLog("Not shown (" + srcs[i] + " selection): " + rfvDescribeList(list));
    }
    var a = RFV.active;
    if (a) {
        try { for (i = 0; i < a.objs.length; i++) if (!a.objs[i].isValid) { RFV.active = null; return null; } } catch (e) { RFV.active = null; return null; }
    }
    return a || null;
}

function rfvFocusMessage(f) {
    RFV.params = {};
    var msg = { type: "focus", kind: f ? f.kind : "none" }, id, p = {};
    if (!f) return msg;
    msg.selection = { count: f.objs.length, others: f.others, more: f.more, source: f.source };
    if (f.kind === "parameter") { msg.param = rfvParamInfo(f.objs[0]); RFV.params[msg.param.id] = f.objs[0]; var u = rfvParamUses(f.objs[0]); msg.uses = u.uses; msg.sandbox = u.sandbox; }
    else if (f.kind === "snapshot") msg.snapshot = rfvSnapshotInfo(f.objs[0]);
    else msg.graph = rfvBuildGraph(f);
    for (id in RFV.params) p[id] = rfvParamInfo(RFV.params[id]);
    msg.params = p;
    return msg;
}

function rfvCursors() {
    var v = {}, n = 0, id;
    for (id in RFV.params) {
        if (n++ >= RFV_MAX_CURSORS) break;
        v[id] = rfvNum(RFV.params[id], "cursorPosition");
    }
    return v;
}

function rfvOpen(id) {
    var obj = null;
    try { obj = studio.project.lookup(id); } catch (e) { obj = null; }
    if (!obj) { rfvLog("Couldn't find " + id + " to open."); return false; }
    var ok = false;
    try { ok = RFV.navigate(obj); } catch (e) { rfvLog("Couldn't open " + rfvDescribe(obj) + ": " + e); }
    return ok;
}

// ── The link ─────────────────────────────────────────────────────────────────

function rfvSend(obj) { RFV.out.push(typeof obj === "string" ? obj : JSON.stringify(obj)); }

function rfvFlush() {
    if (!RFV.out.length || !RFV.proc) return;
    var text = RFV.out.join("\n") + "\n";
    RFV.out = [];
    try { RFV.proc.writeStandardInput(text, 100); } catch (e) { rfvLog("Couldn't write to the viewer: " + e); }
}

function rfvStatus(w, text) {
    if (text === RFV.status) return;
    RFV.status = text;
    try { w.findWidget("rfv_status").setText(text); } catch (e) { }
}

function rfvPoll(w) {
    var proc = RFV.proc, running = false;
    if (!proc) return;
    try { running = proc.isRunning(); } catch (e) { running = false; }
    if (!running) {
        // Never read a finished ScriptProcess (that raises an uncatchable error).
        RFV.proc = null;
        rfvStatus(w, "The Reference Viewer was closed. You can close this window.");
        return;
    }
    RFV.tick++;
    var chunk = "", err = "", nl, line, msg;
    try { chunk = "" + (proc.readAllStandardOutput() || ""); } catch (e) { chunk = ""; }
    try { err = "" + (proc.readAllStandardError() || ""); } catch (e) { err = ""; }
    if (err) rfvLog("viewer: " + err.split("\n").join(" ").trim());
    RFV.buf += chunk;
    var opens = [];
    while ((nl = RFV.buf.indexOf("\n")) >= 0) {
        line = RFV.buf.substring(0, nl).trim();
        RFV.buf = RFV.buf.substring(nl + 1);
        if (!line || line.charAt(0) !== "{") continue;
        try { msg = JSON.parse(line); } catch (e) { continue; }
        if (msg.type === "ready") RFV.ready = true;
        if (msg.cmd === "open" && msg.id) opens.push("" + msg.id);
    }
    if (!RFV.ready) { rfvStatus(w, "Starting the Reference Viewer…"); return; }
    if (!RFV.helloSent) {
        RFV.helloSent = true;
        rfvSend({ type: "hello", studio: "" + studio.version, script: RFV_VERSION, project: "" + (studio.project.filePath || "") });
    }
    // Opening an event moves Studio's selection, so the focus follows next tick.
    if (opens.length) rfvOpen(opens[opens.length - 1]);

    var f = rfvPickFocus(), changed = (f ? f.key : "") !== (RFV.focus ? RFV.focus.key : "");
    RFV.focus = f;
    if (changed || !RFV.sentFocus || RFV.tick % RFV_REREAD_EVERY === 0) {
        var fm, t0 = new Date().getTime();
        try { fm = JSON.stringify(rfvFocusMessage(f)); }
        catch (e) { fm = JSON.stringify({ type: "focus", kind: "error", error: "" + e }); rfvLog("Reading the selection failed: " + e); }
        var ms = new Date().getTime() - t0;
        if (ms > 200) rfvLog("Reading the selection took " + ms + " ms.");
        if (fm !== RFV.sentFocus) { RFV.sentFocus = fm; RFV.sentCursors = ""; rfvSend(fm); }
    }
    var cur = JSON.stringify(rfvCursors());
    if (cur !== RFV.sentCursors) { RFV.sentCursors = cur; rfvSend("{\"type\":\"cursors\",\"values\":" + cur + "}"); }
    rfvFlush();
    rfvStatus(w, f ? "<b>Linked.</b> Showing " + (f.objs.length > 1 ? f.objs.length + " " + f.kind + "s" : f.kind + " '" + rfvObjectName(f.objs[0]) + "'") +
        ". Keep this window open while you use the viewer."
        : "<b>Linked.</b> Select an event, parameter, snapshot, asset, bus, send or shared effect in Studio.");
}

function rfvOpenLinkWindow(gen) {
    var T = studio.ui.widgetType, Lt = studio.ui.layoutType;
    studio.ui.showModelessDialog({
        windowTitle: "Reference Viewer link  v" + RFV_VERSION,
        windowWidth: 380, windowHeight: 110,
        widgetType: T.Layout, layout: Lt.VBoxLayout,
        items: [
            { widgetType: T.Label, widgetId: "rfv_status", wordWrap: true, text: "Starting the Reference Viewer…" },
            { widgetType: T.Label, wordWrap: true, text: "<span style=\"color:#8a8a8a\">This window carries the link between Studio and " +
                "the Reference Viewer. Closing it unlinks the viewer.</span>" },
            {
                // Last widget, so the status label exists. Owns the polling timer.
                widgetType: T.Spacer,
                onConstructed: function () { try { this.startTimer(RFV_TIMER_MS); } catch (e) { rfvLog("Link timer failed: " + e); } },
                onTimerEvent: function () {
                    if (gen !== RFV.gen) {   // a newer launch owns the link: this window is stale
                        try { this.stopTimer(); } catch (e) { }
                        rfvStatus(this, "A newer Reference Viewer link replaced this one. You can close this window.");
                        return;
                    }
                    rfvPoll(this);
                }
            }
        ]
    });
}

// <per-user or project folder>/Tools/ReferenceViewer/, next to the Scripts
// folder this file was loaded from (relative to this script, never absolute).
function rfvToolsDir() {
    var d = ("" + rfvScriptDir).split("\\").join("/");
    if (d.charAt(d.length - 1) === "/") d = d.substring(0, d.length - 1);
    var p = d;
    while (p.length) {
        var cut = p.lastIndexOf("/");
        if (p.substring(cut + 1).toLowerCase() === "scripts") return p.substring(0, cut + 1) + "Tools/ReferenceViewer/";
        if (cut <= 0) break;
        p = p.substring(0, cut);
    }
    return d.substring(0, d.lastIndexOf("/") + 1) + "Tools/ReferenceViewer/";
}
function rfvAppName() { return rfvIsWindows() ? "reference_viewer.exe" : "reference_viewer"; }
function rfvAppPath() { return rfvToolsDir() + rfvAppName(); }

function rfvRunning() { try { return !!RFV.proc && RFV.proc.isRunning(); } catch (e) { return false; } }

function rfvLaunch() {
    if (rfvRunning()) return { error: "The Reference Viewer is already open (look for its window)." };
    var exe = rfvAppPath();
    if (!rfvExists(exe)) {
        return { error: "The Reference Viewer app (" + rfvAppName() + ") isn't installed. Expected it at:\n" + exe +
            "\n\nSee Tools/ReferenceViewer/README.md for how to install it." };
    }
    try { RFV.proc = studio.system.startAsync(exe, { args: ["--bridge"], workingDir: rfvToolsDir() }); }
    catch (e) { return { error: "Couldn't start the Reference Viewer: " + e }; }
    if (!RFV.proc) return { error: "Couldn't start the Reference Viewer (no process)." };
    RFV.gen++;
    RFV.buf = ""; RFV.out = []; RFV.ready = false; RFV.helloSent = false; RFV.tick = 0; RFV.status = "";
    RFV.focus = null; RFV.sentFocus = ""; RFV.sentCursors = ""; RFV.params = {}; RFV.srcKeys = {}; RFV.active = null;
    rfvLog("v" + RFV_VERSION + ": started " + exe);
    rfvOpenLinkWindow(RFV.gen);
    return { ok: true };
}
