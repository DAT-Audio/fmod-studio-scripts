// FMOD Studio Script — Copy-Paste Conditions & Automation
// Version: 1.5.0
// ---------------------------------------------------------------------------
// Opens a NON-BLOCKING (modeless) window for copying an instrument's trigger
// conditions, and its automation & modulation, and pasting them onto other
// instruments. The main Studio window stays usable while the tool is open:
// select an instrument, Copy, select another (or several), Paste.
//
//   Trigger Conditions      — parameter conditions (range, labels, inverted)
//                             and event-state conditions, plus the condition
//                             mode. Works on any instrument type (single,
//                             multi, event/referenced, scatterer, programmer,
//                             …) and on loop regions / transition markers too.
//   Automation & Modulation — every automation curve (points, curve shapes)
//                             and every modulator (all settings, sidechain
//                             links, and automation on the modulator itself).
//                             For event instruments, automation of the nested
//                             event's parameter values is included.
//
// One Copy and one Paste. "Include" (Trigger Conditions, Automation &
// Modulation, or both) picks what they handle. Copy replaces the clipboard
// with the ticked parts; Paste pastes the ticked parts the clipboard holds.
// Conditions also paste onto loop regions and transition markers, which get
// no automation.
//
//   Instrument Settings     — (optional, off by default) delay, quantization,
//                             probability, volume, pitch, voices, stealing,
//                             and the scatterer / multi settings.
//
//   Track Automation        — (1.5.0) select automation lanes under a track
//                             header (or the whole track, or one of its
//                             effects) instead of an instrument, and Copy
//                             takes those lanes: volume, pan, insert effect
//                             and plug-in settings. Paste onto other tracks,
//                             in any event, puts each lane on the same place
//                             (strip, panner, or the same effect type in the
//                             same order) and adds the lane when the track
//                             doesn't show it. Effects are never added: a
//                             lane whose effect the target lacks is skipped
//                             and reported. Follows the Automation &
//                             Modulation tick. Replace swaps the lane's
//                             curves; merge swaps only curves on the same
//                             parameters. Other lanes are left alone.
//
// Pasting onto an instrument in another event re-targets the curves: timeline
// automation goes onto that event's timeline. Conditions and curves point at
// the project-level parameter directly, so the target event doesn't need a
// parameter sheet for them. "Add missing parameter sheets" (off by default)
// also adds a sheet to the target event for each parameter it doesn't have
// one for (the report says so).
//
// The window has two zones. 1 Copy: Include, a fixed-width Copy button, the
// clipboard's counts, and Save as profile… (saves the clipboard). 2 Paste:
// "Paste from" chooses what Paste pastes, the clipboard (the first entry, and
// chosen again after every Copy) or a saved profile; Manage… renames, replaces
// or deletes profiles (both ask before overwriting or deleting); and Paste is
// the large button. Copy never changes a profile.
//
// A Preview pane, to the right of the controls, draws what Paste will paste
// (the source chosen under "Paste from"): a card per condition with a range
// bar like Studio's Trigger Behavior drawer, a chart per automation curve, an
// envelope for AHDSR, and a tag per modulator.
//
// An entry picked in a Multi or Scatterer instrument's playlist is detected
// and used instead of the instrument itself, and the window says so.
//
// The copied data is plain values (no live object handles), so it survives
// the source instrument being edited or deleted. The clipboards last until
// Scripts > Reload. Profiles are saved per user and per project, in the
// project's .user folder (which FMOD keeps out of source control), so they
// survive restarts. They hold the project's own object IDs, so they only
// apply to the project they were saved in.
//
// Nothing outside the FMOD project is read or written — no paths are hardcoded,
// so the script works in any project it is dropped into.
//
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
// A report of every paste is mirrored to: Scripts > Show Script Output
//
// Menu: Scripts > DAT-audio Utilities > Copy-Paste Conditions & Automation
// Part of the DAT-audio Utilities pack — loaded by "DAT-audio Utilities.js"
// in the Scripts directory above this folder.
// ---------------------------------------------------------------------------

"use strict";

var CCP_TITLE = "Copy-Paste Conditions & Automation";
var CCP_VERSION = "1.5.0";   // shown in the window title and the report header

// The clipboard: { conditions, automation }, each the captured data or null
// (not copied). One Copy fills the parts ticked under "Include" and clears the
// others. In memory: survives closing and reopening the window, not Scripts >
// Reload. Copy never writes into a saved profile.
var ccpClipboard = null;

// The window's checkboxes, remembered across closing and reopening it.
var ccpOptions = { conditions: true, automation: true, settings: false, replace: true, sheets: false };

// Saved profiles, [{ name, data: { conditions, automation } }] in the order
// they were saved. Written only by Save as… and Manage…, and saved to disk
// (see "Saved profiles") so they survive Scripts > Reload and restarts.
var CCP_MAX_PROFILES = 50;
var CCP_NAME_MAX = 40;
// "tracks" (track automation lanes) has no Include checkbox of its own: it
// follows Automation & Modulation, and is used when tracks are selected.
var CCP_PARTS = ["conditions", "automation", "settings", "tracks"];

var ccpProfiles = [];
var ccpProfilesFor = null;   // the .fspro path ccpProfiles belongs to

// ── Generic managed-object helpers ───────────────────────────────────────────
// Every managed-object call is wrapped: a stale or partially loaded object
// throws rather than returning false, and unknown names fail silently.

function ccpIs(obj, typeName) {
    try { return !!obj && obj.isOfType(typeName); } catch (e) { return false; }
}

function ccpIsExactly(obj, typeName) {
    try { return !!obj && obj.isOfExactType(typeName); } catch (e) { return false; }
}

function ccpIsObject(v) {
    return !!v && typeof v === "object" && typeof v.isOfType === "function";
}

function ccpGet(obj, key) {
    try {
        var v = obj[key];
        return (v === undefined) ? null : v;
    } catch (e) { return null; }
}

// ToMany relationships return arrays. Copied, so deleting while iterating is safe.
function ccpList(obj, key) {
    var v = ccpGet(obj, key);
    var out = [];
    if (v && typeof v.length === "number") {
        for (var i = 0; i < v.length; i++) out.push(v[i]);
    }
    return out;
}

function ccpId(obj) {
    try { return obj ? "" + obj.id : ""; } catch (e) { return ""; }
}

function ccpEntity(obj) {
    try { return "" + obj.entity; } catch (e) { return ""; }
}

function ccpHasProp(obj, name) {
    try { return !!(obj && obj.properties && obj.properties[name]); } catch (e) { return false; }
}

function ccpSameValue(a, b) {
    if (typeof a === "number" && typeof b === "number") {
        return Math.abs(a - b) <= 1e-4 * Math.max(1, Math.abs(a), Math.abs(b));
    }
    return a === b;
}

// Writes through the property map (the documented route) and verifies by
// reading back — assigning a wrong name to a ManagedObject does not throw.
function ccpSetProp(obj, name, value) {
    if (!ccpHasProp(obj, name)) return false;
    try { obj.properties[name].setValue(value); }
    catch (e) {
        try { obj[name] = value; } catch (e2) { return false; }
    }
    return ccpSameValue(ccpGet(obj, name), value);
}

function ccpDelete(obj) {
    try { return !!studio.project.deleteObject(obj); } catch (e) { return false; }
}

function ccpLookup(id) {
    if (!id) return null;
    try { return studio.project.lookup(id) || null; } catch (e) { return null; }
}

// Fallback creation route when an extension method is missing or refuses:
// create the entity, let `init` set it up, then attach it to its owner.
function ccpCreateAttached(entityName, owner, relationshipName, init) {
    var obj = null;
    try { obj = studio.project.create(entityName); } catch (e) { return null; }
    if (!obj) return null;
    try {
        if (init) init(obj);
        if (owner.relationships[relationshipName].add(obj) === false) throw new Error("add refused");
    } catch (e) {
        ccpDelete(obj);
        return null;
    }
    return obj;
}

function ccpEscape(s) {
    s = "" + s;
    var out = "";
    for (var i = 0; i < s.length; i++) {
        var c = s.charAt(i);
        out += (c === "&") ? "&amp;" : (c === "<") ? "&lt;" : (c === ">") ? "&gt;" : c;
    }
    return out;
}

function ccpPlural(n, word) {
    return n + " " + word + (n === 1 ? "" : "s");
}

function ccpFormatNumber(v) {
    if (typeof v !== "number") return "" + v;
    var r = Math.round(v * 1000) / 1000;
    return "" + r;
}

// ── Describing things ────────────────────────────────────────────────────────

var CCP_KIND_NAMES = {
    SingleSound:      "Single Instrument",
    MultiSound:       "Multi Instrument",
    EventSound:       "Event Instrument",
    SnapshotModule:   "Snapshot Instrument",
    ProgrammerSound:  "Programmer Instrument",
    SilenceSound:     "Silence Instrument",
    SoundScatterer:   "Scatterer Instrument",
    CommandSound:     "Command Instrument",
    PluginSound:      "Plug-in Instrument",
    LoopRegion:       "Loop Region",
    TransitionMarker: "Transition Marker",
    TransitionRegion: "Transition Region",
    GroupTrack:       "Audio Track",
    MasterTrack:      "Master Track",
    ReturnTrack:      "Return Track"
};

// The event that CONTAINS an object. Never reads `.event` off a Sound: on an
// event/snapshot instrument that is the *referenced* event, not the parent.
var CCP_UPWARD = ["owner", "scattererOwner", "parameter", "audioTrack", "timeline", "markerTrack"];

function ccpEventOf(obj) {
    var cur = obj;
    for (var depth = 0; cur && depth < 32; depth++) {
        if (ccpIs(cur, "Event")) return cur;
        if (ccpIs(cur, "Parameter") || ccpIs(cur, "Track")) {
            var ev = ccpGet(cur, "event");
            if (ccpIs(ev, "Event")) return ev;
        }
        var next = null;
        for (var i = 0; i < CCP_UPWARD.length && !next; i++) {
            var c = ccpGet(cur, CCP_UPWARD[i]);
            if (ccpIsObject(c)) next = c;
        }
        cur = next;
    }
    return null;
}

function ccpEventPath(ev) {
    if (!ev) return "";
    try { return "" + ev.getPath(); } catch (e) {}
    var n = ccpGet(ev, "name");
    return n ? "" + n : "";
}

function ccpBaseName(path) {
    path = "" + path;
    var cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return cut >= 0 ? path.substring(cut + 1) : path;
}

function ccpObjectName(obj) {
    var name = ccpGet(obj, "name");
    if (name) return "" + name;
    if (ccpIsExactly(obj, "SingleSound")) {
        var file = ccpGet(obj, "audioFile");
        var p = file ? ccpGet(file, "assetPath") : null;
        if (p) return ccpBaseName(p);
    }
    if (ccpIs(obj, "AudioTrack")) return ccpTrackName(obj);
    if (ccpIs(obj, "EventSound")) {
        var ref = ccpGet(obj, "event");
        var rn = ref ? ccpGet(ref, "name") : null;
        if (rn) return "" + rn;
    }
    return "";
}

function ccpKind(obj) {
    var e = ccpEntity(obj);
    return CCP_KIND_NAMES[e] || e || "Object";
}

function ccpDescribe(obj) {
    var name = ccpObjectName(obj);
    var path = ccpEventPath(ccpEventOf(obj));
    return ccpKind(obj) + (name ? " \"" + name + "\"" : "") + (path ? " in " + path : "");
}

function ccpShortName(obj) {
    return ccpObjectName(obj) || ccpKind(obj);
}

function ccpParamName(gp) {
    if (!gp) return "(missing parameter)";
    var owner = ccpGet(gp, "presetOwner");
    var n = owner ? ccpGet(owner, "name") : null;
    if (!n) n = ccpGet(gp, "name");
    return n ? "" + n : ccpId(gp);
}

// ── Selection ────────────────────────────────────────────────────────────────
// The event editor's selection. An entry picked in a Multi or Scatterer
// instrument's playlist (the "deck", the panel under the event editor) is a
// separate selection in Studio. The window uses it instead when the entry's
// container is selected in the event editor and the playlist was clicked more
// recently (see observeSelection in the window).

function ccpArray(fn) {
    try {
        var a = fn();
        var out = [];
        if (a && typeof a.length === "number") for (var i = 0; i < a.length; i++) out.push(a[i]);
        return out;
    } catch (e) { return []; }
}

function ccpEditorSelection() {
    var arr = ccpArray(function () { return studio.window.editorSelection(); });
    if (!arr.length) {
        try { var cur = studio.window.editorCurrent(); if (cur) arr = [cur]; } catch (e) {}
    }
    return arr;
}

// Playlist entries whose container is one of `editorSel`. Only
// deckSelection(): deckCurrent() may name an entry nobody picked.
function ccpDeckEntries(editorSel) {
    var ids = {};
    for (var i = 0; i < editorSel.length; i++) ids["#" + ccpId(editorSel[i])] = true;
    var deck = ccpArray(function () { return studio.window.deckSelection(); });
    var out = [], parent = null;
    for (var d = 0; d < deck.length; d++) {
        if (!ccpIsInstrument(deck[d])) continue;
        var p = ccpGet(deck[d], "owner");
        if (!ccpIsObject(p)) p = ccpGet(deck[d], "scattererOwner");
        if (!ccpIsObject(p) || !ids["#" + ccpId(p)]) continue;
        out.push(deck[d]);
        if (!parent) parent = p;
    }
    return { entries: out, parent: parent };
}

function ccpCurrent(fromDeck) {
    try { return (fromDeck ? studio.window.deckCurrent() : studio.window.editorCurrent()) || null; }
    catch (e) { return null; }
}

function ccpIsInstrument(obj) {
    // "Sound", not "Module": TransitionSourceSound / TransitionDestinationSound
    // derive from Module but are not instruments.
    return ccpIs(obj, "Sound");
}

// Instruments, loop regions, transition markers / regions. Modules that are not
// Sounds (the transition-timeline source/destination stubs) are Triggerable in
// the object model but are not something a user puts conditions on.
function ccpIsTriggerable(obj) {
    if (ccpIs(obj, "Module")) return ccpIsInstrument(obj);
    if (ccpIs(obj, "Triggerable")) return true;
    try { return ccpHasProp(obj, "triggerConditionMode"); } catch (e) { return false; }
}

// kind: "conditions" | "automation"
function ccpFilterTargets(raw, kind) {
    var accept = (kind === "conditions") ? ccpIsTriggerable : ccpIsInstrument;
    var seen = {};
    var out = [];
    for (var i = 0; i < raw.length; i++) {
        var o = raw[i];
        if (!accept(o)) continue;
        var key = "#" + ccpId(o);
        if (key !== "#" && seen[key]) continue;
        seen[key] = true;
        out.push(o);
    }
    return { targets: out, total: raw.length };
}

// ── Trigger conditions: capture ──────────────────────────────────────────────

function ccpCaptureConditions(src) {
    var data = {
        source:   ccpDescribe(src),
        sourceName: ccpShortName(src),
        sourceId: ccpId(src),
        mode:     ccpHasProp(src, "triggerConditionMode") ? ccpGet(src, "triggerConditionMode") : null,
        items:    [],
        skipped:  []
    };
    var conds = ccpList(src, "triggerConditions");
    for (var i = 0; i < conds.length; i++) {
        var c = conds[i];
        if (ccpIs(c, "ParameterCondition")) {
            var gp = ccpGet(c, "parameter");
            var labels = gp ? ccpGet(gp, "enumerationLabels") : null;
            data.items.push({
                kind:       "parameter",
                paramId:    ccpId(gp),
                paramName:  ccpParamName(gp),
                labels:     (labels && typeof labels.length === "number") ? Array.prototype.slice.call(labels) : null,
                minimum:    ccpGet(c, "minimum"),
                maximum:    ccpGet(c, "maximum"),
                isInverted: !!ccpGet(c, "isInverted")
            });
        } else if (ccpIs(c, "EventCondition")) {
            data.items.push({ kind: "event", eventState: ccpGet(c, "eventState") });
        } else {
            data.skipped.push(ccpEntity(c) || "unknown condition");
        }
    }
    return data;
}

function ccpConditionValueText(item, v) {
    if (item.labels && typeof v === "number" && v === Math.floor(v) &&
        v >= 0 && v < item.labels.length) {
        return "\"" + item.labels[v] + "\"";
    }
    return ccpFormatNumber(v);
}

// Event condition states as Studio names them. 0 = "Not Stopping" was checked
// in Studio; 1 = "Stopping" is inferred (the drawer
// offers just those two).
var CCP_EVENT_STATES = { 0: "Not Stopping", 1: "Stopping" };

function ccpEventStateName(v) {
    return CCP_EVENT_STATES.hasOwnProperty(v) ? CCP_EVENT_STATES[v] : "state " + v;
}

function ccpConditionText(item) {
    if (item.kind === "event") {
        return "Event State: " + ccpEventStateName(item.eventState);
    }
    var range = ccpSameValue(item.minimum, item.maximum)
        ? "= " + ccpConditionValueText(item, item.minimum)
        : ccpConditionValueText(item, item.minimum) + " to " + ccpConditionValueText(item, item.maximum);
    return item.paramName + " " + range + (item.isInverted ? "  (inverted)" : "");
}

function ccpConditionSignature(item) {
    if (item.kind === "event") return "event|" + item.eventState;
    return "param|" + item.paramId + "|" + ccpFormatNumber(item.minimum) + "|" +
           ccpFormatNumber(item.maximum) + "|" + (item.isInverted ? 1 : 0);
}

// ── Parameters on the target event ───────────────────────────────────────────

// Whether `ev` already has a parameter sheet (a ParameterProxy) for `gp`.
// Reads the sheets themselves, not getParameterPresets(): with that check a
// ticked paste added no sheets in Studio, most likely because it
// also lists parameters the event only refers to through conditions or
// automation, such as those left by an earlier paste without sheets.
function ccpEventHasParameter(ev, gp) {
    if (!ev || !gp) return true;
    var gpId = ccpId(gp);
    var params = ccpList(ev, "parameters");
    for (var i = 0; i < params.length; i++) {
        if (ccpIs(params[i], "ParameterProxy") && ccpId(ccpGet(params[i], "preset")) === gpId) return true;
    }
    return false;
}

// Makes sure `ev` has a parameter sheet for `gp`. Returns false only if it is
// missing and could not be added. With `allowAdd` false nothing is added, and
// that's not a failure: a condition or curve references the GameParameter
// itself, never the sheet (addParameterCondition / addAutomationCurve only set
// `.parameter`; addGameParameter is what creates the sheet's ParameterProxy).
function ccpEnsureParameter(ev, gp, log, done, allowAdd) {
    if (!ev || !gp) return true;
    var key = "#" + ccpId(ev) + "|" + ccpId(gp);
    if (done && done[key]) return true;
    if (ccpEventHasParameter(ev, gp)) {
        if (done) done[key] = true;
        return true;
    }
    if (allowAdd === false) {
        log.push("    = no sheet added for \"" + ccpParamName(gp) + "\" (adding sheets is off)");
        if (done) done[key] = true;
        return true;
    }

    var added = null;
    try { added = ev.addGameParameter(gp); } catch (e) { added = null; }
    if (added) {
        log.push("    + added parameter \"" + ccpParamName(gp) + "\" to " + ccpEventPath(ev));
        if (done) done[key] = true;
        return true;
    }
    log.push("    ! could not add parameter \"" + ccpParamName(gp) + "\" to " + ccpEventPath(ev));
    return false;
}

// ── Trigger conditions: paste ────────────────────────────────────────────────

function ccpPasteConditions(clip, target, replace, addSheets, log, stats) {
    var ev = ccpEventOf(target);
    var existing = ccpList(target, "triggerConditions");
    var have = {};

    if (replace) {
        var removed = 0;
        for (var r = 0; r < existing.length; r++) {
            if (ccpDelete(existing[r])) removed++;
            else log.push("    ! could not remove an existing condition");
        }
        if (removed) log.push("    - removed " + ccpPlural(removed, "existing condition"));
    } else {
        var current = ccpCaptureConditions(target).items;
        for (var h = 0; h < current.length; h++) have[ccpConditionSignature(current[h])] = true;
    }

    if (clip.mode !== null && clip.mode !== undefined && ccpHasProp(target, "triggerConditionMode")) {
        if (!ccpSetProp(target, "triggerConditionMode", clip.mode)) {
            log.push("    ! could not set the condition mode");
        }
    }

    for (var i = 0; i < clip.items.length; i++) {
        var item = clip.items[i];
        var text = ccpConditionText(item);

        if (have[ccpConditionSignature(item)]) {
            log.push("    = already present: " + text);
            stats.skipped++;
            continue;
        }

        var cond = null;

        if (item.kind === "parameter") {
            var gp = ccpLookup(item.paramId);
            if (!gp) {
                log.push("    ! skipped " + text + " — the parameter no longer exists in this project");
                stats.failed++;
                continue;
            }
            ccpEnsureParameter(ev, gp, log, stats.paramsDone, addSheets);

            try { cond = target.addParameterCondition(gp, item.minimum, item.maximum); }
            catch (e) { cond = null; }
            if (!cond) {
                cond = ccpCreateAttached("ParameterCondition", target, "triggerConditions",
                    function (c) { c.relationships.parameter.add(gp); });
            }
            if (cond) {
                // Set explicitly: addParameterCondition ignores `max` for
                // labeled parameters, and does not take the inverted flag.
                var ok = ccpSetProp(cond, "minimum", item.minimum) &&
                         ccpSetProp(cond, "maximum", item.maximum) &&
                         ccpSetProp(cond, "isInverted", item.isInverted);
                if (!ok) log.push("    ! " + text + " was added but its range/inversion did not read back as copied");
            }
        } else {
            cond = ccpCreateAttached("EventCondition", target, "triggerConditions", null);
            if (cond && !ccpSetProp(cond, "eventState", item.eventState)) {
                log.push("    ! event state condition added but its state did not read back as copied");
            }
        }

        if (cond) {
            log.push("    + " + text);
            stats.pasted++;
        } else {
            log.push("    ! could not add " + text);
            stats.failed++;
        }
    }
}

// ── Automation & modulation: capture ─────────────────────────────────────────

// Settings per modulator type (object-model XML, 2.02.33 + 2.03.14). Names a
// build doesn't have are skipped at capture time.
var CCP_MOD_PROPS = {
    RandomizerModulator:        ["amount"],
    SeekModulator:              ["speedAscending", "speedDescending", "asymmetric"],
    ADSRModulator:              ["initialValue", "attackTime", "attackShape", "peakValue",
                                 "holdTime", "decayTime", "decayShape", "sustainValue",
                                 "releaseTime", "releaseShape", "finalValue"],
    SidechainModulator:         ["levelMode", "amount", "attackTime", "releaseTime",
                                 "minimumThreshold", "maximumThreshold"],
    LFOModulator:               ["shape", "isTempoSync", "rate", "beats", "phase",
                                 "depth", "direction"],
    AutopitchModulator:         ["root", "pitchAtMinimum"],
    SpectralSidechainModulator: ["mode", "amount", "attackTime", "releaseTime",
                                 "minimumThresholdLevel", "maximumThresholdLevel",
                                 "minimumThresholdFrequency", "maximumThresholdFrequency"]
};
var CCP_MOD_COMMON = ["clockSource"];

// Modulator types and settings that only exist from a given Studio version
// (object-model XML: none of these are in 2.02.33). The scripting model is the
// authority at run time (ccpModelHas); this table only words the message.
var CCP_MOD_SINCE  = { SeekModulator: "2.03", SpectralSidechainModulator: "2.03" };
var CCP_PROP_SINCE = { clockSource: "2.03" };

// Whether the running Studio's object model has this entity. If the model
// can't be read at all, assume it does and let creation decide.
function ccpModelHas(type) {
    var model = null;
    try { model = studio.project.model; } catch (e) { model = null; }
    if (!model) return true;
    try { var ent = model[type]; return ent !== undefined && ent !== null; } catch (e) { return true; }
}

function ccpStudioVersion() {
    try { return ("" + studio.version).replace(/^Version\s*/, ""); } catch (e) { return ""; }
}

var CCP_MOD_NAMES = {
    RandomizerModulator: "Random", SeekModulator: "Seek", ADSRModulator: "AHDSR",
    SidechainModulator: "Sidechain", LFOModulator: "LFO", AutopitchModulator: "Autopitch",
    SpectralSidechainModulator: "Spectral Sidechain"
};

function ccpParamRef(p) {
    if (ccpIsExactly(p, "Timeline")) return { kind: "timeline", name: "Timeline" };
    if (ccpIs(p, "GameParameter"))  return { kind: "game", id: ccpId(p), name: ccpParamName(p) };
    return { kind: "unknown", id: ccpId(p), name: ccpEntity(p) || "unknown sheet" };
}

// Only what belongs to `obj` itself. Track and effect automation lives on the
// track's mixer group / effect chain and is never reached from an instrument;
// this check makes that a guarantee rather than a consequence of traversal.
function ccpOwned(list, inverseName, obj) {
    var id = ccpId(obj);
    var out = [];
    for (var i = 0; i < list.length; i++) {
        var owner = ccpGet(list[i], inverseName);
        if (!owner || ccpId(owner) === id) out.push(list[i]);
    }
    return out;
}

function ccpModKey(type, property) { return "#" + type + "|" + property; }

function ccpFindModulator(obj, type, property) {
    var mods = ccpList(obj, "modulators");
    for (var i = 0; i < mods.length; i++) {
        if (ccpEntity(mods[i]) === type &&
            "" + ccpGet(mods[i], "nameOfPropertyBeingModulated") === property) return mods[i];
    }
    return null;
}

// An automator's curves as plain values: [{ param, points }].
function ccpCaptureCurves(aut) {
    var curves = ccpList(aut, "automationCurves");
    var cdata = [];
    for (var c = 0; c < curves.length; c++) {
        var pts = ccpList(curves[c], "automationPoints");
        var pdata = [];
        for (var p = 0; p < pts.length; p++) {
            pdata.push({
                position:   ccpGet(pts[p], "position"),
                value:      ccpGet(pts[p], "value"),
                curveShape: ccpGet(pts[p], "curveShape"),
                isSCurve:   !!ccpGet(pts[p], "isSCurve")
            });
        }
        pdata.sort(function (x, y) { return x.position - y.position; });
        cdata.push({ param: ccpParamRef(ccpGet(curves[c], "parameter")), points: pdata });
    }
    return cdata;
}

function ccpCaptureAutoMod(obj, depth) {
    var data = { automators: [], modulators: [] };
    if (!obj || depth > 4) return data;

    var auts = ccpOwned(ccpList(obj, "automators"), "objectBeingAutomated", obj);
    for (var a = 0; a < auts.length; a++) {
        data.automators.push({
            property: "" + ccpGet(auts[a], "nameOfPropertyBeingAutomated"),
            curves:   ccpCaptureCurves(auts[a])
        });
    }

    var mods = ccpOwned(ccpList(obj, "modulators"), "objectBeingModulated", obj);
    for (var m = 0; m < mods.length; m++) {
        var mod  = mods[m];
        var type = ccpEntity(mod);
        var names = (CCP_MOD_PROPS[type] || []).concat(CCP_MOD_COMMON);
        var props = {};
        for (var n = 0; n < names.length; n++) {
            if (ccpHasProp(mod, names[n])) props[names[n]] = ccpGet(mod, names[n]);
        }
        var sidechains = ccpList(mod, "sidechains");
        var scIds = [];
        for (var s = 0; s < sidechains.length; s++) scIds.push(ccpId(sidechains[s]));
        data.modulators.push({
            type:       type,
            known:      !!CCP_MOD_PROPS[type],
            property:   "" + ccpGet(mod, "nameOfPropertyBeingModulated"),
            props:      props,
            sidechains: scIds,
            nested:     ccpCaptureAutoMod(mod, depth + 1)
        });
    }
    return data;
}

function ccpAutoModIsEmpty(d) {
    return !d || (d.automators.length === 0 && d.modulators.length === 0);
}

// The proxy behind an event instrument's parameter value (ParameterProperty).
function ccpPropertyPresetId(pp) {
    var proxy = ccpGet(pp, "parameter");
    var preset = proxy ? ccpGet(proxy, "preset") : null;
    return ccpId(preset);
}

function ccpCaptureInstrument(src) {
    var data = {
        source:      ccpDescribe(src),
        sourceName:  ccpShortName(src),
        sourceId:    ccpId(src),
        self:        ccpCaptureAutoMod(src, 0),
        eventParams: [],
        notes:       []
    };
    var pps = ccpList(src, "parameters");     // only event/snapshot instruments have these
    for (var i = 0; i < pps.length; i++) {
        var d = ccpCaptureAutoMod(pps[i], 0);
        if (ccpAutoModIsEmpty(d)) continue;
        var proxy = ccpGet(pps[i], "parameter");
        var preset = proxy ? ccpGet(proxy, "preset") : null;
        data.eventParams.push({ presetId: ccpId(preset), name: ccpParamName(preset), data: d });
    }
    if (ccpIs(src, "PluginSound")) {
        data.notes.push("Plug-in parameter automation lives on the plug-in, not the instrument, and is not copied.");
    }
    return data;
}

function ccpAutoModLines(d, indent) {
    var lines = [];
    for (var a = 0; a < d.automators.length; a++) {
        var au = d.automators[a];
        var parts = [];
        for (var c = 0; c < au.curves.length; c++) {
            parts.push(au.curves[c].param.name + " (" + ccpPlural(au.curves[c].points.length, "point") + ")");
        }
        lines.push(indent + au.property + ": automation on " + (parts.length ? parts.join(", ") : "no curves"));
        // The points, with any bend, so a curve can be checked against Studio.
        for (var cp = 0; cp < au.curves.length; cp++) {
            var pts = au.curves[cp].points;
            if (!pts.length || pts.length > 8) continue;
            var txt = [];
            for (var q = 0; q < pts.length; q++) {
                txt.push(ccpFormatNumber(pts[q].position) + "→" + ccpFormatNumber(pts[q].value) +
                         (pts[q].isSCurve ? " (S-curve)" : pts[q].curveShape ? " (bend " + ccpFormatNumber(pts[q].curveShape) + ")" : ""));
            }
            lines.push(indent + "    · " + au.curves[cp].param.name + ": " + txt.join(", "));
        }
    }
    for (var m = 0; m < d.modulators.length; m++) {
        var mo = d.modulators[m];
        lines.push(indent + mo.property + ": " + (CCP_MOD_NAMES[mo.type] || mo.type) + " modulator");
        var sub = ccpAutoModLines(mo.nested, indent + "    ");
        for (var k = 0; k < sub.length; k++) lines.push(sub[k]);
    }
    return lines;
}

function ccpInstrumentLines(clip) {
    var lines = ccpAutoModLines(clip.self, "");
    for (var i = 0; i < clip.eventParams.length; i++) {
        lines.push("Event parameter \"" + clip.eventParams[i].name + "\":");
        var sub = ccpAutoModLines(clip.eventParams[i].data, "    ");
        for (var k = 0; k < sub.length; k++) lines.push(sub[k]);
    }
    return lines;
}

function ccpCountAutoMod(d) {
    var n = { automators: d.automators.length, modulators: d.modulators.length };
    for (var m = 0; m < d.modulators.length; m++) {
        var sub = ccpCountAutoMod(d.modulators[m].nested);
        n.automators += sub.automators;
        n.modulators += sub.modulators;
    }
    return n;
}

// ── Automation & modulation: paste ───────────────────────────────────────────

// Clears `obj`'s automation and modulation, except modulators whose type and
// property also appear in `incoming` — those are updated in place instead.
function ccpClearAutoMod(obj, log, label, incoming) {
    var keep = {};
    var mods = incoming ? incoming.modulators : [];
    for (var k = 0; k < mods.length; k++) keep[ccpModKey(mods[k].type, mods[k].property)] = true;

    var removed = 0, failed = 0;
    var list = ccpList(obj, "automators");
    var current = ccpList(obj, "modulators");
    for (var c = 0; c < current.length; c++) {
        var key = ccpModKey(ccpEntity(current[c]), "" + ccpGet(current[c], "nameOfPropertyBeingModulated"));
        if (keep[key]) { keep[key] = false; continue; }   // keep one; a duplicate still goes
        list.push(current[c]);
    }
    for (var i = 0; i < list.length; i++) {
        if (ccpDelete(list[i])) removed++; else failed++;
    }
    if (removed) log.push("    - removed " + ccpPlural(removed, "existing automation/modulator") + label);
    if (failed)  log.push("    ! could not remove " + ccpPlural(failed, "existing automation/modulator") + label);
}

// Returns { param }: the parameter to put the curve on, or null.
function ccpResolveCurveParameter(ref, ev, log, stats) {
    if (ref.kind === "timeline") {
        var tl = ev ? ccpGet(ev, "timeline") : null;
        if (!tl) log.push("    ! no timeline found on the target event");
        return { param: tl };
    }
    if (ref.kind === "game") {
        var gp = ccpLookup(ref.id);
        if (!gp) {
            log.push("    ! parameter \"" + ref.name + "\" no longer exists in this project");
            return { param: null };
        }
        if (!ccpEnsureParameter(ev, gp, log, stats.paramsDone, stats.addSheets)) return { param: null };
        return { param: gp };
    }
    log.push("    ! curves on a " + ref.name + " can't be re-targeted and were skipped");
    return { param: null };
}

function ccpAddPoint(curve, pt, param) {
    // addAutomationPoint does not range-check, so do it here for parameters
    // (the timeline has no upper bound).
    if (ccpIs(param, "GameParameter")) {
        var lo = ccpGet(param, "minimum"), hi = ccpGet(param, "maximum");
        if (typeof lo === "number" && typeof hi === "number" &&
            (pt.position < lo - 1e-9 || pt.position > hi + 1e-9)) return false;
    }
    var p = null;
    if (typeof curve.addAutomationPoint === "function") {
        try { p = curve.addAutomationPoint(pt.position, pt.value); } catch (e) { return false; }
    } else {
        p = ccpCreateAttached("AutomationPoint", curve, "automationPoints", function (o) {
            ccpSetProp(o, "position", pt.position);
            ccpSetProp(o, "value", pt.value);
        });
    }
    if (!p) return false;
    ccpSetProp(p, "curveShape", pt.curveShape);
    ccpSetProp(p, "isSCurve", pt.isSCurve);
    return ccpSameValue(ccpGet(p, "position"), pt.position) &&
           ccpSameValue(ccpGet(p, "value"), pt.value);
}

function ccpPasteAutomator(au, obj, ev, log, stats, label) {
    // Resolve every curve's parameter first, so that an automator none of
    // whose curves can be placed doesn't remove the target's own automation.
    var resolved = [];
    for (var c = 0; c < au.curves.length; c++) {
        var res = ccpResolveCurveParameter(au.curves[c].param, ev, log, stats);
        if (res.param) resolved.push({ data: au.curves[c], param: res.param });
    }
    if (au.curves.length && !resolved.length) {
        log.push("    ! automation for " + au.property + label + " skipped — none of its curves could be placed");
        stats.failed++;
        return;
    }

    // An incoming automator replaces the target's automator on the same property.
    var olds = ccpList(obj, "automators");
    for (var o = 0; o < olds.length; o++) {
        if ("" + ccpGet(olds[o], "nameOfPropertyBeingAutomated") === au.property) ccpDelete(olds[o]);
    }

    var aut = null;
    try { aut = obj.addAutomator(au.property); } catch (e) { aut = null; }
    if (!aut) {
        aut = ccpCreateAttached("Automator", obj, "automators", function (o) {
            ccpSetProp(o, "nameOfPropertyBeingAutomated", au.property);
        });
    }
    if (!aut) {
        log.push("    ! could not add automation for " + au.property + label);
        stats.failed++;
        return;
    }

    var made = ccpAddCurves(aut, resolved, au.property + label, log);
    if (resolved.length && !made.curves) {
        ccpDelete(aut);
        log.push("    ! automation for " + au.property + label + " skipped — none of its curves could be placed");
        stats.failed++;
        return;
    }
    log.push("    + " + au.property + label + ": automation, " + ccpCurvesText(made));
    stats.pasted++;
}

// Adds each resolved curve ({ data, param }) to `aut`. Returns the counts.
function ccpAddCurves(aut, resolved, what, log) {
    var made = { curves: 0, points: 0, bad: 0 };
    for (var r = 0; r < resolved.length; r++) {
        var cd = resolved[r].data;
        var param = resolved[r].param;

        var curve = null;
        try { curve = aut.addAutomationCurve(param); } catch (e) { curve = null; }
        if (!curve) {
            curve = ccpCreateAttached("AutomationCurve", aut, "automationCurves", function (o) {
                o.relationships.parameter.add(param);
            });
        }
        if (!curve) {
            log.push("    ! could not add the " + cd.param.name + " curve for " + what);
            continue;
        }
        made.curves++;

        // Studio's addAutomationCurve doesn't seed points; this is a guard.
        var seeded = ccpList(curve, "automationPoints");
        for (var s = 0; s < seeded.length; s++) ccpDelete(seeded[s]);

        for (var p = 0; p < cd.points.length; p++) {
            made.points++;
            if (!ccpAddPoint(curve, cd.points[p], param)) made.bad++;
        }
    }
    return made;
}

function ccpCurvesText(made) {
    return ccpPlural(made.curves, "curve") + ", " + ccpPlural(made.points - made.bad, "point") +
           (made.bad ? "  (" + ccpPlural(made.bad, "point") + " out of range or not placed)" : "");
}

function ccpPasteModulator(md, obj, ev, log, stats, label, depth) {
    var name = (CCP_MOD_NAMES[md.type] || md.type) + " modulator";
    if (!md.known) {
        log.push("    ! " + name + " on " + md.property + label + " is a type this tool doesn't know — skipped");
        stats.failed++;
        return;
    }

    // A modulator of this type already on this property is updated in place —
    // the target never ends up with two (e.g. two Random modulators on pitch).
    // A type the running Studio doesn't have (e.g. Seek, new in 2.03, pasted
    // into a 2.02 project) can't be created; say why rather than "could not add".
    if (!ccpModelHas(md.type)) {
        var ver = ccpStudioVersion();
        log.push("    ! " + name + " on " + md.property + label + " skipped — it needs FMOD Studio " +
                 (CCP_MOD_SINCE[md.type] || "a newer version") + " or newer" + (ver ? " (this is " + ver + ")" : ""));
        stats.failed++;
        return;
    }

    var mod = ccpFindModulator(obj, md.type, md.property);
    var reused = !!mod;
    if (!mod) {
        // Never via addModulator(): Studio's implementation returns ANY existing
        // modulator on the same property instead of creating the requested
        // type, so a Random pasted after an LFO on pitch silently became the
        // LFO. This is what addModulator does internally, minus that return.
        mod = ccpCreateAttached(md.type, obj, "modulators", function (o) {
            ccpSetProp(o, "nameOfPropertyBeingModulated", md.property);
        });
        if (mod && ccpEntity(mod) !== md.type) mod = null;
    }
    if (!mod) {
        log.push("    ! could not add " + name + " on " + md.property + label);
        stats.failed++;
        return;
    }

    var bad = [], older = [];
    for (var k in md.props) {
        if (!Object.prototype.hasOwnProperty.call(md.props, k)) continue;
        if (!ccpHasProp(mod, k)) {
            // e.g. clockSource on 2.02: only worth a note when it isn't the default.
            if (md.props[k] !== 0 && md.props[k] !== false && md.props[k] !== null) older.push(k);
            continue;
        }
        if (!ccpSetProp(mod, k, md.props[k])) bad.push(k);
    }

    // Sidechain links are replaced, not accumulated, on a reused modulator.
    if (reused) {
        var oldSc = ccpList(mod, "sidechains");
        for (var r = 0; r < oldSc.length; r++) {
            try { mod.relationships.sidechains.remove(oldSc[r]); } catch (e) {}
        }
    }
    var missingSc = 0;
    for (var s = 0; s < md.sidechains.length; s++) {
        var sc = ccpLookup(md.sidechains[s]);
        var ok = false;
        if (sc) { try { ok = mod.relationships.sidechains.add(sc) !== false; } catch (e) { ok = false; } }
        if (!ok) missingSc++;
    }

    log.push("    " + (reused ? "~ " : "+ ") + md.property + label + ": " + name +
             (reused ? " (updated the existing one)" : "") +
             (bad.length ? "  (settings not applied: " + bad.join(", ") + ")" : "") +
             (older.length ? "  (not in this FMOD version, not applied: " + older.join(", ") + ")" : "") +
             (missingSc ? "  (" + ccpPlural(missingSc, "sidechain link") + " not restored)" : ""));
    stats.pasted++;

    var nestedLabel = label + " › " + name;
    if (reused && stats.replace) ccpClearAutoMod(mod, log, nestedLabel, md.nested);
    ccpApplyAutoMod(md.nested, mod, ev, log, stats, nestedLabel, depth + 1);
}

// Merge semantics: an incoming automator replaces the target's automator on
// the same property; an incoming modulator UPDATES one of the same type on the
// same property (never a second one). Everything else is left alone.
function ccpApplyAutoMod(data, obj, ev, log, stats, label, depth) {
    if (!data || depth > 4) return;

    for (var a = 0; a < data.automators.length; a++) {
        var au = data.automators[a];
        if (!ccpHasProp(obj, au.property)) {
            log.push("    ! skipped automation for " + au.property + label + " — " +
                     ccpKind(obj) + " has no such property");
            stats.failed++;
            continue;
        }
        ccpPasteAutomator(au, obj, ev, log, stats, label);
    }

    for (var m = 0; m < data.modulators.length; m++) {
        var md = data.modulators[m];
        if (!ccpHasProp(obj, md.property)) {
            log.push("    ! skipped " + (CCP_MOD_NAMES[md.type] || md.type) + " modulator on " +
                     md.property + label + " — " + ccpKind(obj) + " has no such property");
            stats.failed++;
            continue;
        }
        ccpPasteModulator(md, obj, ev, log, stats, label, depth);
    }
}

function ccpPasteInstrument(clip, target, replace, addSheets, log, stats) {
    var ev = ccpEventOf(target);
    var targetParams = ccpList(target, "parameters");

    stats.replace = replace;
    stats.addSheets = addSheets;
    if (replace) {
        ccpClearAutoMod(target, log, "", clip.self);
        for (var t = 0; t < targetParams.length; t++) {
            var incoming = null;
            for (var e = 0; e < clip.eventParams.length && !incoming; e++) {
                if (clip.eventParams[e].presetId === ccpPropertyPresetId(targetParams[t])) {
                    incoming = clip.eventParams[e].data;
                }
            }
            ccpClearAutoMod(targetParams[t], log, " on event parameter \"" +
                            ccpParamName(ccpGet(ccpGet(targetParams[t], "parameter"), "preset")) + "\"",
                            incoming);
        }
    }

    ccpApplyAutoMod(clip.self, target, ev, log, stats, "", 0);

    for (var i = 0; i < clip.eventParams.length; i++) {
        var ep = clip.eventParams[i];
        var label = " (event parameter \"" + ep.name + "\")";
        if (!ccpIs(target, "EventSound")) {
            log.push("    ! skipped" + label + " — the target is not an event instrument");
            stats.failed++;
            continue;
        }
        var match = null;
        for (var j = 0; j < targetParams.length && !match; j++) {
            if (ccpPropertyPresetId(targetParams[j]) === ep.presetId) match = targetParams[j];
        }
        if (!match) {
            log.push("    ! skipped" + label + " — the referenced event has no such parameter");
            stats.failed++;
            continue;
        }
        ccpApplyAutoMod(ep.data, match, ev, log, stats, label, 0);
    }
}

// ── Track automation: lanes ──────────────────────────────────────────────────
// The rows under a track header are AutomationTracks ("lanes"), each pointing
// at an Automator. What that automates depends on the lane (2.02.33 and
// 2.03.14 models, identical here): Volume sits on the track's mixer strip
// (EventMixerGroup / EventMixerMaster / EventMixerReturn), pan on the strip's
// panner, an insert's settings on the effect in the strip's effect chain, a
// plug-in's settings on its PluginParameter. A lane is stored by where that
// object sits on its track, so the same place can be found on another track.
// Effects are never added: a target track without the effect is reported.

function ccpIsAudioTrack(o) { return ccpIs(o, "AudioTrack"); }

// GroupTrack / MasterTrack → mixerGroup, ReturnTrack → mixerReturn.
function ccpTrackStrip(track) {
    var s = ccpGet(track, "mixerGroup");
    if (!ccpIsObject(s)) s = ccpGet(track, "mixerReturn");
    return ccpIsObject(s) ? s : null;
}

function ccpTrackEffects(track) {
    var strip = ccpTrackStrip(track);
    var chain = strip ? ccpGet(strip, "effectChain") : null;
    return chain ? ccpList(chain, "effects") : [];
}

function ccpTrackName(track) {
    if (ccpIs(track, "MasterTrack")) return "Master";
    var strip = ccpTrackStrip(track);
    var n = strip ? ccpGet(strip, "name") : null;
    return n ? "" + n : "";
}

// The event track a strip, panner, effect or plug-in setting belongs to, or
// null (a mixer bus outside an event has no track).
function ccpTrackOfObject(o) {
    for (var d = 0; o && d < 8; d++) {
        if (ccpIsAudioTrack(o)) return o;
        if (ccpIs(o, "MixerStrip")) {
            var t = ccpGet(o, "groupTrack");
            if (!ccpIsObject(t)) t = ccpGet(o, "returnTrack");
            return ccpIsAudioTrack(t) ? t : null;
        }
        var next = null;
        if (ccpIs(o, "PluginParameter")) {
            var pl = ccpGet(o, "plugin");
            next = pl ? ccpGet(pl, "owner") : null;
        } else if (ccpIs(o, "MixerEffect")) {
            next = ccpGet(o, "bus");                        // the panner, or an effect chain
            if (!ccpIsObject(next)) {
                var chain = ccpGet(o, "owner");
                next = chain ? ccpGet(chain, "bus") : null;
            }
        }
        o = ccpIsObject(next) ? next : null;
    }
    return null;
}

var CCP_EFFECT_NAMES = { MixerBusFader: "Fader", MixerBusPanner: "Panner", MixerSend: "Send",
                         ThreeEQEffect: "3-EQ", ITEchoEffect: "IT Echo", SFXReverbEffect: "Reverb" };

function ccpPluginIdentifier(fx) {
    var pl = ccpGet(fx, "plugin");
    var id = pl ? ccpGet(pl, "identifier") : null;
    return id ? "" + id : "";
}

function ccpEffectName(fx) {
    var e = ccpEntity(fx);
    if (CCP_EFFECT_NAMES[e]) return CCP_EFFECT_NAMES[e];
    if (e === "PluginEffect") return ccpPluginIdentifier(fx) || "Plug-in";
    return ccpPretty(e.replace(/Effect$/, ""));
}

// Effects match across tracks by type, plus which plug-in or which return.
function ccpEffectKey(fx) {
    var e = ccpEntity(fx);
    if (e === "PluginEffect") return e + "|" + ccpPluginIdentifier(fx);
    if (e === "MixerSend") {
        var ret = ccpGet(fx, "mixerReturn");
        return e + "|" + (ret ? ccpGet(ret, "name") || "" : "");
    }
    return e;
}

// Where `obj` (what a lane automates) sits on `track`: { kind, … } or { error }.
function ccpLaneAddress(obj, track) {
    var strip = ccpTrackStrip(track);
    if (strip && ccpId(obj) === ccpId(strip)) return { kind: "strip" };
    if (ccpIs(obj, "EventAutomatableProperties")) return { kind: "event" };
    if (ccpIs(obj, "MixerBusPanner")) return { kind: "panner" };
    var param = null;
    if (ccpIs(obj, "PluginParameter")) {
        param = "" + ccpGet(obj, "name");
        var pl = ccpGet(obj, "plugin");
        obj = pl ? ccpGet(pl, "owner") : null;
    }
    if (ccpIs(obj, "MixerEffect")) {
        var fxs = ccpTrackEffects(track), key = ccpEffectKey(obj), n = 0;
        for (var i = 0; i < fxs.length; i++) {
            if (ccpId(fxs[i]) === ccpId(obj)) {
                return { kind: "effect", key: key, n: n, name: ccpEffectName(obj), param: param };
            }
            if (ccpEffectKey(fxs[i]) === key) n++;
        }
        // Not in the chain: a preset effect's settings live on the shared
        // preset, so its automation is shared by every use.
        return { error: "a preset effect's automation is shared by every use of the preset, so it isn't copied" };
    }
    return { error: "it automates a " + (ccpEntity(obj) || "an unknown object") + ", which this tool doesn't know" };
}

// The object at `addr` on `track`, or null.
function ccpResolveLane(addr, track) {
    var strip = ccpTrackStrip(track);
    if (addr.kind === "strip") return strip;
    if (addr.kind === "panner") return strip ? ccpGet(strip, "panner") : null;
    if (addr.kind === "event") {
        var ev = ccpEventOf(track);
        return ev ? ccpGet(ev, "automatableProperties") : null;
    }
    if (addr.kind !== "effect") return null;
    var fxs = ccpTrackEffects(track), n = 0, fx = null;
    for (var i = 0; i < fxs.length && !fx; i++) {
        if (ccpEffectKey(fxs[i]) !== addr.key) continue;
        if (n === addr.n) fx = fxs[i];
        n++;
    }
    if (!fx || addr.param === null || addr.param === undefined) return fx;
    var pl = ccpGet(fx, "plugin");
    var params = pl ? ccpList(pl, "pluginParameters") : [];
    for (var p = 0; p < params.length; p++) {
        if ("" + ccpGet(params[p], "name") === addr.param) return params[p];
    }
    return null;
}

// "Volume", "Panner Stereo Pan", "Lowpass Cutoff", "fmod.myplugin Drive".
function ccpLaneLabel(addr, property) {
    if (addr.kind === "strip") return ccpPretty(property);
    if (addr.kind === "panner") return "Panner " + ccpPretty(property);
    if (addr.kind === "event") return "Event " + ccpPretty(property);
    var what = addr.n ? addr.name + " " + (addr.n + 1) : addr.name;
    return what + " " + (addr.param !== null && addr.param !== undefined ? addr.param : ccpPretty(property));
}

// One lane as plain values, or { error }.
function ccpCaptureLane(lane, track) {
    var aut = ccpGet(lane, "automator");
    var obj = aut ? ccpGet(aut, "objectBeingAutomated") : null;
    if (!obj) return { error: "a lane with no automation behind it" };
    var property = "" + ccpGet(aut, "nameOfPropertyBeingAutomated");
    var addr = ccpLaneAddress(obj, track);
    if (addr.error) return { error: ccpPretty(property) + " — " + addr.error };
    return { addr: addr, property: property, label: ccpLaneLabel(addr, property), curves: ccpCaptureCurves(aut) };
}

// The chosen lanes of `track` (all of them when `lanes` is null).
function ccpCaptureTrack(track, lanes) {
    var data = { source: ccpDescribe(track), sourceName: ccpShortName(track), sourceId: ccpId(track),
                 lanes: [], skipped: [] };
    var list = lanes || ccpList(track, "automationTracks");
    for (var i = 0; i < list.length; i++) {
        var d = ccpCaptureLane(list[i], track);
        if (d.error) data.skipped.push(d.error);
        else data.lanes.push(d);
    }
    return data;
}

// Makes sure `track` shows a lane for `aut`. Returns "had", "added" or "".
function ccpEnsureLane(track, obj, aut, property) {
    var lanes = ccpList(track, "automationTracks");
    for (var i = 0; i < lanes.length; i++) {
        if (ccpId(ccpGet(lanes[i], "automator")) === ccpId(aut)) return "had";
    }
    // addAutomationTrack goes through addAutomator, which returns the existing
    // automator on the property, so the new lane shows `aut`. Checked anyway.
    var lane = null;
    try { lane = track.addAutomationTrack(obj, property); } catch (e) { lane = null; }
    if (lane && ccpId(ccpGet(lane, "automator")) !== ccpId(aut)) { ccpDelete(lane); lane = null; }
    if (!lane) {
        lane = ccpCreateAttached("AutomationTrack", track, "automationTracks", function (o) {
            o.relationships.automator.add(aut);
        });
    }
    return lane ? "added" : "";
}

// Pastes one lane onto `track`. The lane is the same place on this track
// (same strip property, panner, or the same effect type in the same order).
// Replace: the lane's curves are replaced. Merge: only curves on the same
// parameters are. Other lanes on the track are never touched.
function ccpPasteLane(lane, track, ev, replace, log, stats) {
    var label = lane.label;
    var obj = ccpResolveLane(lane.addr, track);
    if (!obj) {
        log.push("    ! skipped " + label + " — " + (ccpTrackName(track) || "this track") + " has no " +
                 (lane.addr.kind === "effect" ? (lane.addr.n ? "matching " : "") + lane.addr.name +
                  (lane.addr.param ? " with a \"" + lane.addr.param + "\" setting" : "") + " effect (effects aren't added)"
                  : "such control"));
        stats.failed++;
        return;
    }
    if (!ccpHasProp(obj, lane.property)) {
        log.push("    ! skipped " + label + " — the target has no " + lane.property + " property");
        stats.failed++;
        return;
    }

    var resolved = [], incoming = {};
    for (var c = 0; c < lane.curves.length; c++) {
        var res = ccpResolveCurveParameter(lane.curves[c].param, ev, log, stats);
        if (res.param) {
            resolved.push({ data: lane.curves[c], param: res.param });
            incoming["#" + ccpId(res.param)] = true;
        }
    }
    if (!resolved.length) {
        log.push("    ! " + label + " skipped — none of its curves could be placed");
        stats.failed++;
        return;
    }

    var aut = null, created = false;
    var auts = ccpOwned(ccpList(obj, "automators"), "objectBeingAutomated", obj);
    for (var a = 0; a < auts.length && !aut; a++) {
        if ("" + ccpGet(auts[a], "nameOfPropertyBeingAutomated") === lane.property) aut = auts[a];
    }
    if (!aut) {
        try { aut = obj.addAutomator(lane.property); } catch (e) { aut = null; }
        if (!aut) {
            aut = ccpCreateAttached("Automator", obj, "automators", function (o) {
                ccpSetProp(o, "nameOfPropertyBeingAutomated", lane.property);
            });
        }
        if (!aut) {
            log.push("    ! could not add automation for " + label);
            stats.failed++;
            return;
        }
        created = true;
    }

    var removed = 0;
    var old = ccpList(aut, "automationCurves");
    for (var o = 0; o < old.length; o++) {
        if ((replace || incoming["#" + ccpId(ccpGet(old[o], "parameter"))]) && ccpDelete(old[o])) removed++;
    }

    var made = ccpAddCurves(aut, resolved, label, log);
    if (!made.curves) {
        if (created) ccpDelete(aut);
        log.push("    ! " + label + " skipped — none of its curves could be placed");
        stats.failed++;
        return;
    }
    var laneState = ccpEnsureLane(track, obj, aut, lane.property);
    log.push("    + " + label + ": " + ccpCurvesText(made) +
             (removed ? ", replaced " + ccpPlural(removed, "existing curve") : "") +
             (laneState === "added" ? "  (lane added)" : ""));
    if (!laneState) log.push("    ! " + label + " was pasted, but its lane couldn't be added to the track");
    stats.pasted++;
}

function ccpPasteTrack(clip, track, replace, log, stats) {
    var ev = ccpEventOf(track);
    for (var i = 0; i < clip.lanes.length; i++) ccpPasteLane(clip.lanes[i], track, ev, replace, log, stats);
}

// ── Track automation: selection ──
// A lane, a point or curve in a lane, a track header, or a track's strip or
// effect. Returns { track, lanes } (lanes: the AutomationTracks picked, or
// null for the whole track), or null.
function ccpTrackRef(o) {
    if (ccpIs(o, "Sound")) return null;
    if (ccpIs(o, "AutomationPoint")) o = ccpGet(o, "automationCurve");
    if (ccpIs(o, "AutomationCurve")) {
        var au = ccpGet(o, "automator");
        var ls = au ? ccpList(au, "automationTracks") : [];
        for (var i = 0; i < ls.length; i++) {
            if (ccpIsAudioTrack(ccpGet(ls[i], "audioTrack"))) return { track: ccpGet(ls[i], "audioTrack"), lanes: [ls[i]] };
        }
        return null;
    }
    if (ccpIs(o, "AutomationTrack")) {
        var at = ccpGet(o, "audioTrack");
        return ccpIsAudioTrack(at) ? { track: at, lanes: [o] } : null;
    }
    var track = ccpTrackOfObject(o);
    if (!track) return null;
    if (!ccpIs(o, "MixerEffect") || ccpIs(o, "EffectChain")) return { track: track, lanes: null };
    // An effect: its own lanes (including its plug-in settings). The Fader
    // also stands for the strip: Studio automates track volume on the strip.
    var also = ccpIs(o, "MixerBusFader") ? ccpId(ccpTrackStrip(track)) : null;
    var out = [], lanes = ccpList(track, "automationTracks");
    for (var k = 0; k < lanes.length; k++) {
        var aut = ccpGet(lanes[k], "automator");
        var obj = aut ? ccpGet(aut, "objectBeingAutomated") : null;
        if (ccpIs(obj, "PluginParameter")) { var pl = ccpGet(obj, "plugin"); obj = pl ? ccpGet(pl, "owner") : null; }
        if (obj && (ccpId(obj) === ccpId(o) || ccpId(obj) === also)) out.push(lanes[k]);
    }
    return { track: track, lanes: out };
}

// The tracks in a selection, with the lanes picked on each. A specific lane
// wins over its whole track being selected too.
function ccpTrackSelection(raw) {
    var targets = [], picks = {};
    for (var i = 0; i < raw.length; i++) {
        var r = ccpTrackRef(raw[i]);
        if (!r) continue;
        var key = "#" + ccpId(r.track);
        if (!picks[key]) { picks[key] = { lanes: [], whole: false, seen: {} }; targets.push(r.track); }
        var p = picks[key];
        if (!r.lanes) { p.whole = true; continue; }
        for (var k = 0; k < r.lanes.length; k++) {
            var lk = "#" + ccpId(r.lanes[k]);
            if (!p.seen[lk]) { p.seen[lk] = true; p.lanes.push(r.lanes[k]); }
        }
    }
    return { targets: targets, total: raw.length, picks: picks };
}

// The lanes to copy from `track`: the ones picked, or all of them.
function ccpPickedLanes(sel, track) {
    var p = sel.picks ? sel.picks["#" + ccpId(track)] : null;
    if (!p || (!p.lanes.length && p.whole)) return null;
    return p.lanes;
}

// ── Instrument settings: capture and paste ───────────────────────────────────
// The instrument's own settings: trigger behaviour (delay, quantization,
// probability), base volume/pitch and playback, voices, and the settings a
// type adds (scatterer spawning, multi playlist). Not its place on the
// timeline (start, length), name or colour. From the 2.03.14 object model.

var CCP_SETTINGS = [
    // [property, group, label, unit]
    ["volume", "Instrument", "Volume", "dB"],
    ["pitch", "Instrument", "Pitch", "st"],
    ["startOffset", "Instrument", "Start offset", "%"],
    ["isAsync", "Instrument", "Async", ""],
    ["isCutoff", "Instrument", "Cut-off", ""],
    ["looping", "Instrument", "Loop", ""],
    ["playCount", "Instrument", "Play count", ""],
    ["maxVoices", "Instrument", "Max voices", ""],
    ["voiceStealing", "Instrument", "Voice stealing", "enum"],
    ["duration", "Instrument", "Duration", "ms"],
    ["intensity", "Instrument", "Intensity", "%"],
    ["delayType", "Trigger behaviour", "Delay type", "enum"],
    ["minimumTimeDelay", "Trigger behaviour", "Delay min", "ms"],
    ["maximumTimeDelay", "Trigger behaviour", "Delay max", "ms"],
    ["quantizationInterval", "Trigger behaviour", "Quantization", "enum"],
    ["triggerProbabilityEnabled", "Trigger behaviour", "Probability on", ""],
    ["triggerProbability", "Trigger behaviour", "Probability", "%"],
    ["playlistMode", "Playlist", "Playlist mode", "enum"],
    ["playbackMode", "Playlist", "Playback mode", "enum"],
    ["polyphony", "Scatterer", "Polyphony", ""],
    ["soundStealing", "Scatterer", "Sound stealing", "enum"],
    ["totalSounds", "Scatterer", "Total sounds", ""],
    ["spawnDelayType", "Scatterer", "Spawn delay type", "enum"],
    ["minimumTimeBetweenSounds", "Scatterer", "Spawn interval min", "ms"],
    ["maximumTimeBetweenSounds", "Scatterer", "Spawn interval max", "ms"],
    ["spawnQuantizationInterval", "Scatterer", "Spawn quantization", "enum"],
    ["spawnRate", "Scatterer", "Spawn rate", "%"],
    ["minimumScatterDistance", "Scatterer", "Distance min", ""],
    ["maximumScatterDistance", "Scatterer", "Distance max", ""],
    ["volumeRandomization", "Scatterer", "Volume randomization", "dB"],
    ["pitchRandomization", "Scatterer", "Pitch randomization", "st"]
];

// Settings Studio hides for a type (isHidden in the object model): never
// copied from it or pasted onto it.
var CCP_HIDDEN = {
    SilenceSound:   ["volume", "pitch", "startOffset", "looping", "playCount"],
    CommandSound:   ["isAsync", "isCutoff", "startOffset", "volume", "pitch", "voiceStealing", "maxVoices"],
    PluginSound:    ["isAsync", "isCutoff", "startOffset", "volume", "pitch"],
    SnapshotModule: ["volume", "pitch", "startOffset"],
    SoundScatterer: ["startOffset"]
};

function ccpSettingHidden(obj, name) {
    var h = CCP_HIDDEN[ccpEntity(obj)];
    return !!h && h.indexOf(name) >= 0;
}

function ccpCaptureSettings(src) {
    var data = { source: ccpDescribe(src), sourceName: ccpShortName(src), sourceId: ccpId(src),
                 type: ccpEntity(src), props: {} };
    for (var i = 0; i < CCP_SETTINGS.length; i++) {
        var k = CCP_SETTINGS[i][0];
        if (ccpHasProp(src, k) && !ccpSettingHidden(src, k)) data.props[k] = ccpGet(src, k);
    }
    return data;
}

function ccpSettingsCount(d) {
    var n = 0;
    for (var k in d.props) if (Object.prototype.hasOwnProperty.call(d.props, k)) n++;
    return n;
}

function ccpPasteSettings(clip, target, log, stats) {
    var changed = [], same = 0, notHere = [], failed = [];
    for (var i = 0; i < CCP_SETTINGS.length; i++) {
        var k = CCP_SETTINGS[i][0];
        if (!Object.prototype.hasOwnProperty.call(clip.props, k)) continue;
        var v = clip.props[k];
        if (!ccpHasProp(target, k) || ccpSettingHidden(target, k)) { notHere.push(CCP_SETTINGS[i][2]); continue; }
        var before = ccpGet(target, k);
        if (ccpSameValue(before, v)) { same++; continue; }
        if (ccpSetProp(target, k, v)) changed.push(CCP_SETTINGS[i][2] + " " + ccpFormatNumber(before) + " → " + ccpFormatNumber(v));
        else failed.push(CCP_SETTINGS[i][2]);
    }
    log.push("    " + (changed.length ? "+ " : "= ") + "settings: " +
             (changed.length ? ccpPlural(changed.length, "change") + " (" + changed.join(", ") + ")" : "nothing to change") +
             (same ? ", " + same + " already the same" : "") +
             (notHere.length ? "; not on " + ccpKind(target) + ": " + notHere.join(", ") : ""));
    if (failed.length) { log.push("    ! settings not applied: " + failed.join(", ")); stats.failed++; }
    if (changed.length) stats.pasted++;
    else stats.skipped++;
}

// ── Visual display ───────────────────────────────────────────────────────────
// The scripting UI has no canvas, but Labels and TextEdits render Qt rich
// text, including tables with per-cell colours and percentage widths. The
// range bars and curve charts below are built from those.

var CCP_C = {
    card: "#2f2f2f", track: "#444444", on: "#9ccc65", text: "#e6e6e6", dim: "#9a9a9a",
    badge: "#ffd54f", badgeText: "#000000", settings: "#64b5f6", chartBg: "#1c1c1c", grid: "#343434", curve: "#ff3d6e",
    mod: "#3d2b45", modText: "#e1bee7", env: "#ce93d8", warn: "#ffb74d"
};

function ccpFmt(v) {
    if (typeof v !== "number") return "" + v;
    var a = Math.abs(v);
    var r = a >= 100 ? Math.round(v) : a >= 10 ? Math.round(v * 10) / 10 : Math.round(v * 100) / 100;
    return "" + r;
}

// "startOffset" → "Start Offset"
function ccpPretty(name) {
    var s = "" + name, out = "";
    for (var i = 0; i < s.length; i++) {
        var c = s.charAt(i);
        out += (i === 0) ? c.toUpperCase() : (c >= "A" && c <= "Z" ? " " + c : c);
    }
    return out;
}

function ccpSpan(color, text, extra) {
    return "<span style=\"color:" + color + ";" + (extra || "") + "\">" + text + "</span>";
}

// A horizontal bar: [{ w: percent, color }]
function ccpBar(segments, height) {
    var cells = "";
    for (var i = 0; i < segments.length; i++) {
        if (!(segments[i].w > 0)) continue;
        cells += "<td width=\"" + segments[i].w.toFixed(1) + "%\" bgcolor=\"" + segments[i].color +
                 "\" style=\"font-size:" + (height || 7) + "px;\">&nbsp;</td>";
    }
    return "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" border=\"0\"><tr>" + cells + "</tr></table>";
}

function ccpCard(inner, bg) {
    // The colour is set on the cell itself: Qt doesn't always carry a body
    // colour into table cells.
    return "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"7\" bgcolor=\"" + (bg || CCP_C.card) +
           "\"><tr><td style=\"color:" + CCP_C.text + ";\">" + inner + "</td></tr></table><div style=\"font-size:4px;\">&nbsp;</div>";
}

function ccpBadge(text) {
    return "<span style=\"background-color:" + CCP_C.badge + "; color:" + CCP_C.badgeText +
           "; font-size:11px; font-weight:bold;\">&nbsp;" + text + "&nbsp;</span>";
}

// One trigger condition, drawn like Studio's Trigger Behavior drawer.
function ccpConditionCard(item) {
    if (item.kind === "event") {
        return ccpCard("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr><td><b>Event State</b></td>" +
                       "<td align=\"right\"><span style=\"background-color:#1d1d1d; color:" + CCP_C.text + ";\">&nbsp;&nbsp;" +
                       ccpEscape(ccpEventStateName(item.eventState)) + "&nbsp;&nbsp;</span></td></tr></table>");
    }
    var gp = ccpLookup(item.paramId);
    var head = "<b>" + ccpEscape(item.paramName) + "</b>" + (item.isInverted ? "&nbsp;&nbsp;" + ccpBadge("Inverted") : "");
    var segs = [], under = "";
    var inside = function (on) { return (on !== item.isInverted) ? CCP_C.on : CCP_C.track; };

    if (item.labels && item.labels.length) {
        var n = item.labels.length, names = "";
        for (var i = 0; i < n; i++) {
            var on = i >= item.minimum - 1e-9 && i <= item.maximum + 1e-9;
            segs.push({ w: 100 / n, color: inside(on) });
            names += "<td width=\"" + (100 / n).toFixed(1) + "%\" align=\"center\">" +
                     ccpSpan(on !== item.isInverted ? CCP_C.text : CCP_C.dim, ccpEscape(item.labels[i]), "font-size:11px;") + "</td>";
        }
        // Thin gaps between label segments read as separate choices.
        var withGaps = [];
        for (var g = 0; g < segs.length; g++) {
            if (g) withGaps.push({ w: 0.6, color: CCP_C.card });
            withGaps.push({ w: segs[g].w - (g ? 0.6 : 0), color: segs[g].color });
        }
        segs = withGaps;
        under = "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"1\"><tr>" + names + "</tr></table>";
    } else {
        var lo = gp ? ccpGet(gp, "minimum") : null, hi = gp ? ccpGet(gp, "maximum") : null;
        if (typeof lo !== "number" || typeof hi !== "number" || hi <= lo) {
            lo = Math.min(0, item.minimum); hi = Math.max(1, item.maximum);
        }
        var span = hi - lo;
        var a = Math.max(0, Math.min(100, (item.minimum - lo) / span * 100));
        var b = Math.max(1.2, Math.min(100 - a, (item.maximum - item.minimum) / span * 100));
        segs = [{ w: a, color: inside(false) }, { w: b, color: inside(true) }, { w: 100 - a - b, color: inside(false) }];
        under = "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"1\"><tr><td>" +
                ccpSpan(CCP_C.text, ccpFmt(item.minimum) + " – " + ccpFmt(item.maximum), "font-size:11px;") +
                "</td><td align=\"right\">" + ccpSpan(CCP_C.dim, "range " + ccpFmt(lo) + " – " + ccpFmt(hi), "font-size:11px;") +
                "</td></tr></table>";
    }
    return ccpCard(head + "<br/>" + ccpBar(segs, 8) + under);
}

function ccpConditionsHtml(d) {
    if (!d.items.length) return ccpSpan(CCP_C.dim, "No trigger conditions.");
    var out = "";
    for (var i = 0; i < d.items.length; i++) out += ccpConditionCard(d.items[i]);
    if (d.mode) out += ccpSpan(CCP_C.dim, "Condition mode " + ccpEscape(d.mode), "font-size:11px;");
    return out;
}

// How far along a segment's value range the curve is at t (0–1). Studio
// stores the bend on the segment's first point: curveShape 0 is a straight
// line, positive accelerates (starts slow), negative decelerates (starts
// fast), per the setFadeInCurve docs; isSCurve eases both ends. The exact maths
// isn't documented, so this is an approximation for display only.
function ccpShaped(t, shape, sCurve) {
    shape = shape || 0;
    var k = Math.min(12, 1 + Math.abs(shape) * 4);
    if (sCurve) {
        var e = Math.max(2, k);
        return t < 0.5 ? 0.5 * Math.pow(2 * t, e) : 1 - 0.5 * Math.pow(2 - 2 * t, e);
    }
    if (!shape) return t;
    return shape > 0 ? Math.pow(t, k) : 1 - Math.pow(1 - t, k);
}

// Value of the curve at x (held flat outside its points).
function ccpCurveAt(points, x) {
    if (!points.length) return 0;
    if (x <= points[0].position) return points[0].value;
    for (var i = 1; i < points.length; i++) {
        if (x <= points[i].position) {
            var p0 = points[i - 1], p1 = points[i];
            var t = (p1.position === p0.position) ? 1 : (x - p0.position) / (p1.position - p0.position);
            return p0.value + (p1.value - p0.value) * ccpShaped(t, p0.curveShape, p0.isSCurve);
        }
    }
    return points[points.length - 1].value;
}

// "#rrggbb" a→b by f (0–1).
function ccpMix(a, b, f) {
    var out = "#";
    for (var i = 1; i < 7; i += 2) {
        var x = parseInt(a.substr(i, 2), 16), y = parseInt(b.substr(i, 2), 16);
        var v = Math.round(x + (y - x) * Math.max(0, Math.min(1, f)));
        out += (v < 16 ? "0" : "") + v.toString(16);
    }
    return out;
}

// A line chart drawn with table cells: the cell the curve passes through is
// bright (blended by how far into it the curve reaches), cells under it are a
// softer fill. Runs of one colour are merged with colspan to keep it small.
function ccpChart(points, xlo, xhi, color, cols, rows) {
    cols = cols || 72; rows = rows || 14;
    var vlo = Infinity, vhi = -Infinity;
    for (var i = 0; i < points.length; i++) { vlo = Math.min(vlo, points[i].value); vhi = Math.max(vhi, points[i].value); }
    if (!points.length) { vlo = 0; vhi = 1; }
    if (vhi - vlo < 1e-9) { vlo -= 1; vhi += 1; }
    var bg = CCP_C.chartBg, fill = ccpMix(bg, color, 0.32);
    var heights = [];
    for (var c = 0; c < cols; c++) {
        var x = xlo + (xhi - xlo) * (c + 0.5) / cols;
        heights.push(Math.max(0.6, (ccpCurveAt(points, x) - vlo) / (vhi - vlo) * rows));
    }
    var html = "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" border=\"0\" bgcolor=\"" + bg + "\">";
    for (var r = rows; r >= 1; r--) {
        var cells = [];
        for (var k = 0; k < cols; k++) {
            var h = heights[k], top = Math.ceil(h - 1e-9);
            cells.push(r > top ? bg : r === top ? ccpMix(fill, color, 0.45 + 0.55 * (h - (r - 1))) : fill);
        }
        html += "<tr>";
        for (var a = 0; a < cols; ) {
            var b = a;
            while (b + 1 < cols && cells[b + 1] === cells[a]) b++;
            html += "<td" + (b > a ? " colspan=\"" + (b - a + 1) + "\"" : "") + " width=\"" + ((b - a + 1) * 100 / cols).toFixed(2) +
                    "%\" bgcolor=\"" + cells[a] + "\" style=\"font-size:3px;\">&nbsp;</td>";
            a = b + 1;
        }
        html += "</tr>";
    }
    return { html: html + "</table>", vlo: vlo, vhi: vhi };
}

// ── Chart images ─────────────────────────────────────────────────────────────
// Table cells can't draw a smooth curve, so charts are drawn pixel by pixel
// into a BMP in the project's .user folder and shown with <img> (Qt's rich
// text loads local image files). A 2× copy named "…@2x.bmp" is written too:
// Qt picks it on high-DPI screens. If a write fails (or the engine can't hand
// bytes to File.writeBinary), charts fall back to tables for the session.

// mode: null until the first chart, then "bmp" (form = the writeBinary
// argument that worked) or "xpm". ok goes false when nothing works.
var CCP_IMG = { ok: true, mode: null, form: "", whyBmp: "", written: {} };
var CCP_CHART_W = 360, CCP_CHART_H = 84;

function ccpUserDir() {
    var p = "";
    try { p = "" + (studio.project.filePath || ""); } catch (e) { p = ""; }
    var cut = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return cut < 0 ? "" : p.substring(0, cut + 1) + ".user/";
}

function ccpHash(str) {
    var h = 5381;
    for (var i = 0; i < str.length; i++) h = ((h * 33) ^ str.charCodeAt(i)) >>> 0;
    return h.toString(16);
}

function ccpRgb(hex) { return [parseInt(hex.substr(1, 2), 16), parseInt(hex.substr(3, 2), 16), parseInt(hex.substr(5, 2), 16)]; }

// 24-bit BMP from top-down RGB rows.
function ccpBmp(w, h, rgb) {
    var row = (w * 3 + 3) & ~3, size = 54 + row * h;
    var b = new Uint8Array(size);
    function u32(at, v) { b[at] = v & 255; b[at + 1] = (v >>> 8) & 255; b[at + 2] = (v >>> 16) & 255; b[at + 3] = (v >>> 24) & 255; }
    b[0] = 66; b[1] = 77; u32(2, size); u32(10, 54); u32(14, 40); u32(18, w); u32(22, h);
    b[26] = 1; b[28] = 24; u32(34, row * h); u32(38, 2835); u32(42, 2835);
    for (var y = 0; y < h; y++) {
        var src = (h - 1 - y) * w * 3, dst = 54 + y * row;
        for (var x = 0; x < w; x++) {
            b[dst + x * 3] = rgb[src + x * 3 + 2]; b[dst + x * 3 + 1] = rgb[src + x * 3 + 1]; b[dst + x * 3 + 2] = rgb[src + x * 3];
        }
    }
    return b;
}

// Draws the curve: dotted grid, a soft fill under the curve, an anti-aliased
// line (coverage of each pixel by the curve's span across that column, so
// steep drops stay solid), and a dot on each point.
function ccpRenderCurve(points, xlo, xhi, vlo, vhi, color, w, h, scale) {
    var bg = ccpRgb(CCP_C.chartBg), grid = ccpRgb(CCP_C.grid), line = ccpRgb(color);
    var fill = [0, 1, 2].map(function (i) { return Math.round(bg[i] + (line[i] - bg[i]) * 0.22); });
    var rgb = new Uint8Array(w * h * 3);
    var padX = 5 * scale, padY = 6 * scale, pw = w - 2 * padX, ph = h - 2 * padY;
    function X(px) { return xlo + (xhi - xlo) * (px - padX) / pw; }
    function Y(v) { return padY + (1 - (v - vlo) / (vhi - vlo)) * ph; }
    function put(x, y, c, a) {
        if (x < 0 || y < 0 || x >= w || y >= h || a <= 0) return;
        var i = (y * w + x) * 3;
        for (var k = 0; k < 3; k++) rgb[i + k] = Math.round(rgb[i + k] + (c[k] - rgb[i + k]) * Math.min(1, a));
    }
    for (var i = 0; i < w * h; i++) { rgb[i * 3] = bg[0]; rgb[i * 3 + 1] = bg[1]; rgb[i * 3 + 2] = bg[2]; }
    // Dotted grid: 5 columns, value at 0/50/100%.
    for (var g = 0; g <= 5; g++) {
        var gx = Math.round(padX + pw * g / 5);
        for (var gy = 0; gy < h; gy += 2 * scale) put(gx, gy, grid, 1);
    }
    for (var r = 0; r <= 2; r++) {
        var ry = Math.round(padY + ph * r / 2);
        for (var rx = 0; rx < w; rx += 2 * scale) put(rx, ry, grid, 1);
    }
    var half = 1.0 * scale;
    for (var px = 0; px < w; px++) {
        var ya = Y(ccpCurveAt(points, X(px))), yb = Y(ccpCurveAt(points, X(px + 1)));
        var ym = Y(ccpCurveAt(points, X(px + 0.5)));
        for (var py = Math.max(0, Math.floor(ym)); py < h; py++) {
            put(px, py, fill, Math.min(1, py + 1 - ym));               // fill below the curve
        }
        var lo = Math.min(ya, yb) - half, hi = Math.max(ya, yb) + half;
        for (var qy = Math.max(0, Math.floor(lo)); qy <= Math.min(h - 1, Math.ceil(hi)); qy++) {
            put(px, qy, line, Math.min(qy + 1, hi) - Math.max(qy, lo));  // line coverage
        }
    }
    var rad = 2.6 * scale;
    for (var p = 0; p < points.length; p++) {
        if (points[p].position < xlo || points[p].position > xhi) continue;
        var cx = padX + (points[p].position - xlo) / (xhi - xlo) * pw, cy = Y(points[p].value);
        for (var dy = Math.floor(cy - rad - 1); dy <= cy + rad + 1; dy++) {
            for (var dx = Math.floor(cx - rad - 1); dx <= cx + rad + 1; dx++) {
                var d = Math.sqrt((dx + 0.5 - cx) * (dx + 0.5 - cx) + (dy + 0.5 - cy) * (dy + 0.5 - cy));
                put(dx, dy, line, rad + 0.5 - d);
            }
        }
    }
    return rgb;
}

function ccpFileSize(path) {
    try {
        var f = studio.system.getFile(path);
        return f.exists() ? f.size() : -1;
    } catch (e) { return -1; }
}

// File.writeBinary's argument type isn't documented, so each likely form is
// tried until the file lands on disk at the right size (its return value
// isn't trusted). Returns { form } or { form: "", why }.
var CCP_BIN_FORMS = ["ArrayBuffer", "Uint8Array", "Array"];

function ccpWriteBytes(path, bytes, onlyForm) {
    var forms = onlyForm ? [onlyForm] : CCP_BIN_FORMS, why = [];
    for (var i = 0; i < forms.length; i++) {
        var fm = forms[i], ret;
        var arg = fm === "ArrayBuffer" ? bytes.buffer : fm === "Uint8Array" ? bytes : Array.prototype.slice.call(bytes);
        try {
            var f = studio.system.getFile(path);
            if (!f.open(studio.system.openMode.WriteOnly)) { why.push(fm + ": couldn't open " + path); continue; }
            ret = f.writeBinary(arg);
            f.close();
        } catch (e) { why.push(fm + ": " + e); continue; }
        var size = ccpFileSize(path);
        if (size === bytes.length) return { form: fm };
        why.push(fm + ": returned " + ret + ", file " + size + " of " + bytes.length + " bytes");
    }
    return { form: "", why: why.join("; ") };
}

// XPM: a plain-text image format Qt reads natively, written with writeText,
// for when binary writes don't work. Two characters per pixel.
function ccpXpm(w, h, rgb) {
    var A = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-";
    for (var q = 1; q <= 64; q *= 2) {          // coarser colours until they fit 64×64 codes
        var codes = {}, colours = [], rows = [], over = false;
        for (var y = 0; y < h && !over; y++) {
            var row = "";
            for (var x = 0; x < w; x++) {
                var i = (y * w + x) * 3;
                var r = Math.min(255, Math.round(rgb[i] / q) * q), g = Math.min(255, Math.round(rgb[i + 1] / q) * q),
                    b = Math.min(255, Math.round(rgb[i + 2] / q) * q);
                var key = (r << 16) | (g << 8) | b, code = codes[key];
                if (code === undefined) {
                    var n = colours.length;
                    if (n >= 4096) { over = true; break; }
                    code = codes[key] = A.charAt(n >> 6) + A.charAt(n & 63);
                    var hex = key.toString(16);
                    colours.push("\"" + code + " c #" + "000000".substring(hex.length) + hex + "\",");
                }
                row += code;
            }
            rows.push("\"" + row + "\"" + (y < h - 1 ? "," : ""));
        }
        if (!over) {
            return "/* XPM */\nstatic char *chart[] = {\n\"" + w + " " + h + " " + colours.length + " 2\",\n" +
                   colours.join("\n") + "\n" + rows.join("\n") + "\n};\n";
        }
    }
    return "";
}

function ccpWriteText(path, text) {
    try {
        var f = studio.system.getFile(path);
        if (!f.open(studio.system.openMode.WriteOnly)) return false;
        f.writeText(text);
        f.close();
    } catch (e) { return false; }
    return ccpFileSize(path) === text.length;
}

function ccpImgNote(text) {
    try { studio.system.print("[" + CCP_TITLE + "] " + text + "\n"); } catch (e) {}
}

// Writes one chart image as BMP, or XPM if binary writes don't work. Returns
// the extension written, or "" (then the session uses table charts). Says
// once in Script Output which route is used, and why, when it's not BMP.
function ccpWriteChart(path, w, h, rgb) {
    if (CCP_IMG.mode !== "xpm") {
        var r = ccpWriteBytes(path + ".bmp", ccpBmp(w, h, rgb), CCP_IMG.form);
        if (r.form) {
            if (CCP_IMG.mode !== "bmp") ccpImgNote("Chart images: BMP, written with File.writeBinary(" + r.form + ").");
            CCP_IMG.mode = "bmp";
            CCP_IMG.form = r.form;
            return ".bmp";
        }
        CCP_IMG.whyBmp = r.why;
        try { studio.system.getFile(path + ".bmp").remove(); } catch (e) {}
    }
    if (ccpWriteText(path + ".xpm", ccpXpm(w, h, rgb))) {
        if (CCP_IMG.mode !== "xpm") ccpImgNote("Chart images: XPM text (File.writeBinary didn't work: " + CCP_IMG.whyBmp + ").");
        CCP_IMG.mode = "xpm";
        return ".xpm";
    }
    ccpImgNote("Chart images unavailable, so charts are drawn as tables. File.writeBinary: " + CCP_IMG.whyBmp +
               ". XPM via writeText failed too.");
    CCP_IMG.ok = false;
    return "";
}

// Deletes this tool's chart images from .user, so they don't pile up. Called
// when the window opens; charts are written again as they're shown.
function ccpClearCharts() {
    CCP_IMG.written = {};
    CCP_IMG.ok = true;                  // try again each time the window opens
    var dir = ccpUserDir();
    if (!dir) return;
    var names = [];
    try { names = studio.system.readDir(dir, studio.system.readDirFilters.Files) || []; } catch (e) { names = []; }
    for (var i = 0; i < names.length; i++) {
        var n = "" + names[i];
        if (n.indexOf("ccp_chart_") !== 0) continue;
        try { studio.system.getFile(dir + n).remove(); } catch (e) {}
    }
}

// An <img> of the chart, or null (then the caller draws a table chart).
function ccpChartImage(points, xlo, xhi, vlo, vhi, color) {
    if (!CCP_IMG.ok) return null;
    var dir = ccpUserDir();
    if (!dir) return null;
    var key = ccpHash(JSON.stringify([points, xlo, xhi, vlo, vhi, color, CCP_CHART_W, CCP_CHART_H, CCP_C.chartBg]));
    var base = dir + "ccp_chart_" + key;
    if (!CCP_IMG.written[key]) {
        var ext = "";
        for (var s = 1; s <= 2; s++) {
            var w = CCP_CHART_W * s, h = CCP_CHART_H * s;
            ext = ccpWriteChart(base + (s === 2 ? "@2x" : ""), w, h, ccpRenderCurve(points, xlo, xhi, vlo, vhi, color, w, h, s));
            if (!ext) return null;
        }
        CCP_IMG.written[key] = ext;
    }
    return "<img src=\"" + base + CCP_IMG.written[key] + "\" width=\"" + CCP_CHART_W + "\" height=\"" + CCP_CHART_H + "\" />";
}

// Value range with a little headroom, like Studio's curve view.
function ccpValueRange(points) {
    var vlo = Infinity, vhi = -Infinity;
    for (var i = 0; i < points.length; i++) { vlo = Math.min(vlo, points[i].value); vhi = Math.max(vhi, points[i].value); }
    if (!points.length) { vlo = 0; vhi = 1; }
    if (vhi - vlo < 1e-9) { vlo -= 1; vhi += 1; }
    return { lo: vlo, hi: vhi };
}

// The chart: an image when possible, else the table chart.
function ccpChartHtml(points, xlo, xhi, color, cols, rows) {
    var vr = ccpValueRange(points);
    var img = ccpChartImage(points, xlo, xhi, vr.lo, vr.hi, color);
    if (img) return { html: img, vlo: vr.lo, vhi: vr.hi };
    return ccpChart(points, xlo, xhi, color, cols, rows);
}

function ccpCurveCard(property, curve, title) {
    var pts = curve.points;
    var xlo = 0, xhi = 1, axis = "";
    if (curve.param.kind === "game") {
        var gp = ccpLookup(curve.param.id);
        xlo = gp ? ccpGet(gp, "minimum") : 0; xhi = gp ? ccpGet(gp, "maximum") : 1;
        if (typeof xlo !== "number" || typeof xhi !== "number" || xhi <= xlo) {
            xlo = pts.length ? pts[0].position : 0; xhi = pts.length ? pts[pts.length - 1].position : 1;
            if (xhi <= xlo) xhi = xlo + 1;
        }
        axis = ccpEscape(curve.param.name) + " " + ccpFmt(xlo) + " → " + ccpFmt(xhi);
    } else {
        xhi = pts.length ? Math.max(pts[pts.length - 1].position * 1.1, 0.1) : 1;
        axis = "Timeline 0 → " + ccpFmt(xhi) + " s";
    }
    var ch = ccpChartHtml(pts, xlo, xhi, CCP_C.curve);
    return ccpCard("<b>" + ccpEscape(title || ccpPretty(property)) + "</b> " +
                   ccpSpan(CCP_C.dim, "automated by " + ccpEscape(curve.param.name) + " · " + ccpPlural(pts.length, "point"), "font-size:11px;") +
                   "<br/>" + ch.html +
                   "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"1\"><tr><td>" +
                   ccpSpan(CCP_C.dim, axis, "font-size:11px;") + "</td><td align=\"right\">" +
                   ccpSpan(CCP_C.dim, "value " + ccpFmt(ch.vlo) + " – " + ccpFmt(ch.vhi), "font-size:11px;") +
                   "</td></tr></table>" + ccpPointChips(pts, curve.param.kind === "timeline" ? " s" : ""));
}

// The curve's points under its chart, as Studio would show them on hover:
// "0.2 → 100 · bend -0.6". Up to 12, then "+N more".
function ccpPointChips(points, unit) {
    if (!points.length) return "";
    var chips = [];
    for (var i = 0; i < points.length && i < 12; i++) {
        var p = points[i];
        chips.push("<span style=\"background-color:#1d1d1d; color:" + CCP_C.text + "; font-size:11px;\">&nbsp;" +
                   ccpSpan(CCP_C.curve, "●", "font-size:9px;") + " " + ccpFmt(p.position) + unit + " → <b>" + ccpFmt(p.value) + "</b>" +
                   (p.isSCurve ? ccpSpan(CCP_C.dim, " · S-curve") : p.curveShape ? ccpSpan(CCP_C.dim, " · bend " + ccpFmt(p.curveShape)) : "") +
                   "&nbsp;</span>");
    }
    return "<div style=\"margin-top:3px; line-height:150%;\">" + chips.join("&nbsp; ") +
           (points.length > 12 ? ccpSpan(CCP_C.dim, "&nbsp; +" + (points.length - 12) + " more", "font-size:11px;") : "") + "</div>";
}

// AHDSR envelope shape from its settings (times in ms).
function ccpEnvelopePoints(p) {
    // Each segment's shape setting bends it, as curveShape does on a curve.
    var t = 0, pts = [{ position: 0, value: p.initialValue || 0, curveShape: p.attackShape || 0 }];
    t += p.attackTime || 0; pts.push({ position: t, value: p.peakValue || 0 });
    t += p.holdTime || 0;   pts.push({ position: t, value: p.peakValue || 0, curveShape: p.decayShape || 0 });
    t += p.decayTime || 0;  pts.push({ position: t, value: p.sustainValue || 0 });
    var sustain = Math.max(t * 0.3, 200);
    t += sustain;           pts.push({ position: t, value: p.sustainValue || 0, curveShape: p.releaseShape || 0 });
    t += p.releaseTime || 0; pts.push({ position: t, value: p.finalValue || 0 });
    return pts;
}

// A modulator setting as Studio shows it. Random's amount is stored as a
// percentage of the property's range: on pitch (±24 st, 48 st) 6.25 is 3 st,
// confirmed in Studio. Other ranges aren't
// known, so those amounts show as stored, in %.
function ccpModSettingText(md, k, v) {
    if (typeof v === "boolean") return v ? "on" : "off";
    if (md.type === "RandomizerModulator" && k === "amount") {
        return md.property === "pitch" ? ccpFmt(v * 48 / 100) + " st" : ccpFmt(v) + "%";
    }
    return ccpFmt(v);
}

// Instrument settings as a card: one row per group, label/value pairs.
function ccpSettingValue(row, v) {
    if (typeof v === "boolean") return v ? "on" : "off";
    if (row[3] === "enum") return "option " + v;
    return ccpFmt(v) + (row[3] ? (row[3] === "%" ? "%" : " " + row[3]) : "");
}

function ccpSettingsHtml(d) {
    var groups = [], byGroup = {};
    for (var i = 0; i < CCP_SETTINGS.length; i++) {
        var row = CCP_SETTINGS[i];
        if (!Object.prototype.hasOwnProperty.call(d.props, row[0])) continue;
        if (!byGroup[row[1]]) { byGroup[row[1]] = []; groups.push(row[1]); }
        byGroup[row[1]].push(row);
    }
    if (!groups.length) return ccpSpan(CCP_C.dim, "No instrument settings.");
    var out = "";
    for (var g = 0; g < groups.length; g++) {
        var cells = "", list = byGroup[groups[g]];
        for (var r = 0; r < list.length; r++) {
            cells += (r % 2 === 0 ? "<tr>" : "") +
                     "<td>" + ccpSpan(CCP_C.dim, ccpEscape(list[r][2]), "font-size:11px;") + "</td>" +
                     "<td align=\"right\"><b>" + ccpEscape(ccpSettingValue(list[r], d.props[list[r][0]])) + "</b>&nbsp;&nbsp;&nbsp;</td>" +
                     (r % 2 === 1 || r === list.length - 1 ? "</tr>" : "");
        }
        out += ccpCard("<b>" + ccpEscape(groups[g]) + "</b>" +
                       "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"2\">" + cells + "</table>");
    }
    return out;
}

function ccpModulatorCard(md, depth) {
    var name = CCP_MOD_NAMES[md.type] || md.type;
    var settings = [];
    for (var k in md.props) {
        if (!Object.prototype.hasOwnProperty.call(md.props, k) || k === "clockSource") continue;
        settings.push(ccpPretty(k) + " " + ccpModSettingText(md, k, md.props[k]));
    }
    var inner = ccpSpan(CCP_C.env, "◉ ") + "<b>" + ccpEscape(name) + "</b> " + ccpSpan(CCP_C.dim, "on", "font-size:11px;") + " <b>" +
                ccpEscape(ccpPretty(md.property)) + "</b>" +
                (ccpModelHas(md.type) ? "" : "&nbsp;&nbsp;" + ccpSpan(CCP_C.warn, "needs FMOD " + (CCP_MOD_SINCE[md.type] || "newer") +
                                                                  "+, won't paste here", "font-size:11px;")) + "<br/>" +
                ccpSpan(CCP_C.modText, ccpEscape(settings.join(" · ")), "font-size:11px;");
    if (md.type === "ADSRModulator") {
        var pts = ccpEnvelopePoints(md.props);
        inner += "<br/>" + ccpChartHtml(pts, 0, pts[pts.length - 1].position || 1, CCP_C.env, 72, 10).html;
    }
    var nested = ccpAutoModHtml(md.nested, depth + 1);
    if (nested) inner += "<br/>" + nested;
    return ccpCard(inner, CCP_C.mod);
}

function ccpAutoModHtml(d, depth) {
    var out = "";
    for (var a = 0; a < d.automators.length; a++) {
        for (var c = 0; c < d.automators[a].curves.length; c++) out += ccpCurveCard(d.automators[a].property, d.automators[a].curves[c]);
    }
    for (var m = 0; m < d.modulators.length; m++) out += ccpModulatorCard(d.modulators[m], depth || 0);
    return out;
}

function ccpAutomationHtml(d) {
    var out = ccpAutoModHtml(d.self, 0);
    for (var i = 0; i < d.eventParams.length; i++) {
        out += "<p><b>Event parameter \"" + ccpEscape(d.eventParams[i].name) + "\"</b></p>" + ccpAutoModHtml(d.eventParams[i].data, 0);
    }
    return out || ccpSpan(CCP_C.dim, "No automation or modulation.");
}

function ccpTracksHtml(d) {
    var out = "";
    for (var i = 0; i < d.lanes.length; i++) {
        var lane = d.lanes[i];
        for (var c = 0; c < lane.curves.length; c++) out += ccpCurveCard(lane.property, lane.curves[c], lane.label);
    }
    return out || ccpSpan(CCP_C.dim, "No track automation.");
}

// ── Saved profiles ───────────────────────────────────────────────────────────
// One JSON file per project, in its .user folder: per user, and kept out of
// source control by FMOD's own conventions. Loaded when the window opens for a
// project it hasn't been loaded for, saved after every Save as… and every
// change in Manage….

var CCP_PROFILES_FILE = "Copy-Paste Conditions & Automation profiles.json";

function ccpProjectPath() {
    try { return "" + (studio.project.filePath || ""); } catch (e) { return ""; }
}

function ccpProfilesPath(projectPath) {
    var cut = Math.max(projectPath.lastIndexOf("/"), projectPath.lastIndexOf("\\"));
    return cut < 0 ? "" : projectPath.substring(0, cut + 1) + ".user/" + CCP_PROFILES_FILE;
}

function ccpIsArray(v) { return Object.prototype.toString.call(v) === "[object Array]"; }

function ccpValidAutoMod(d, depth) {
    if (!d || !ccpIsArray(d.automators) || !ccpIsArray(d.modulators) || depth > 4) return false;
    for (var m = 0; m < d.modulators.length; m++) {
        if (!d.modulators[m] || !ccpValidAutoMod(d.modulators[m].nested, depth + 1)) return false;
    }
    return true;
}

// Only shapes this tool wrote; anything else is left out.
function ccpValidData(kind, d) {
    if (!d || typeof d !== "object") return false;
    if (kind === "conditions") return ccpIsArray(d.items);
    if (kind === "settings") return !!d.props && typeof d.props === "object";
    if (kind === "tracks") {
        if (!ccpIsArray(d.lanes)) return false;
        for (var l = 0; l < d.lanes.length; l++) {
            var lane = d.lanes[l];
            if (!lane || !lane.addr || typeof lane.addr.kind !== "string" ||
                typeof lane.property !== "string" || !ccpIsArray(lane.curves)) return false;
        }
        return true;
    }
    if (!ccpValidAutoMod(d.self, 0) || !ccpIsArray(d.eventParams)) return false;
    for (var i = 0; i < d.eventParams.length; i++) {
        if (!d.eventParams[i] || !ccpValidAutoMod(d.eventParams[i].data, 0)) return false;
    }
    return true;
}

// A clipboard / profile: each part valid or absent, at least one present.
function ccpValidClip(c) {
    if (!c || typeof c !== "object") return false;
    var any = false;
    for (var i = 0; i < CCP_PARTS.length; i++) {
        var d = c[CCP_PARTS[i]];
        if (d === null || d === undefined) continue;
        if (!ccpValidData(CCP_PARTS[i], d)) return false;
        any = true;
    }
    return any;
}

// Loads the open project's profiles, once per project. Returns "" or a
// message for the report.
function ccpLoadProfiles() {
    var project = ccpProjectPath();
    if (ccpProfilesFor === project) return "";
    ccpProfilesFor = project;
    ccpProfiles = [];
    var path = ccpProfilesPath(project);
    if (!path) return "";

    var raw = "";
    try {
        var f = studio.system.getFile(path);
        if (!f.exists()) return "";
        if (!f.open(studio.system.openMode.ReadOnly)) return "Couldn't open the saved profiles: " + path;
        raw = "" + f.readText(f.size() + 1024);
        f.close();
    } catch (e) { return "Couldn't read the saved profiles: " + e; }

    var saved = null;
    try { saved = JSON.parse(raw); } catch (e) { saved = null; }
    if (!saved || typeof saved !== "object") {
        return "The saved profiles file couldn't be read and was ignored (it is replaced on the next save): " + path;
    }
    // 1.4.0 keeps one list of two-part profiles. A file from the 1.3.0 draft
    // had a list per section; those load as one-part profiles.
    var entries = [];
    if (ccpIsArray(saved.profiles)) {
        entries = saved.profiles;
    } else {
        for (var k = 0; k < CCP_PARTS.length; k++) {
            var old = saved[CCP_PARTS[k]];
            for (var o = 0; ccpIsArray(old) && o < old.length; o++) {
                if (!old[o] || typeof old[o] !== "object" || !old[o].data) continue;
                var one = { conditions: null, automation: null };
                one[CCP_PARTS[k]] = old[o].data;
                entries.push({ name: old[o].name, data: one });
            }
        }
    }
    var dropped = 0;
    for (var i = 0; i < entries.length; i++) {
        var p = entries[i];
        if (!p || typeof p !== "object") continue;
        var name = (typeof p.name === "string") ? String.prototype.trim.call(p.name).substring(0, CCP_NAME_MAX) : "";
        if (!ccpValidClip(p.data) || ccpProfiles.length >= CCP_MAX_PROFILES) { dropped++; continue; }
        if (!name || ccpFindProfile(name) >= 0) name = ccpUniqueName(name || "Profile");
        ccpProfiles.push({ name: name,
                           data: { conditions: p.data.conditions || null, automation: p.data.automation || null,
                                   settings: p.data.settings || null, tracks: p.data.tracks || null } });
    }
    return dropped ? ccpPlural(dropped, "saved profile") + " couldn't be read and " +
                     (dropped === 1 ? "was" : "were") + " left out." : "";
}

// Index of the profile called `name` (ignoring case), or -1.
function ccpFindProfile(name, except) {
    var want = ("" + name).toLowerCase();
    for (var i = 0; i < ccpProfiles.length; i++) {
        if (i !== except && ccpProfiles[i].name.toLowerCase() === want) return i;
    }
    return -1;
}

function ccpUniqueName(base) {
    base = ("" + base).substring(0, CCP_NAME_MAX - 4);
    if (ccpFindProfile(base) < 0) return base;
    for (var n = 2; ; n++) {
        if (ccpFindProfile(base + " " + n) < 0) return base + " " + n;
    }
}

// Returns "" or a message for the report.
function ccpSaveProfiles() {
    var path = ccpProfilesPath(ccpProfilesFor || ccpProjectPath());
    if (!path) return "The project has no path on disk yet, so the profiles weren't saved.";
    var text = JSON.stringify({ tool: CCP_TITLE, version: CCP_VERSION, profiles: ccpProfiles });
    try {
        var f = studio.system.getFile(path);
        if (!f.open(studio.system.openMode.WriteOnly)) return "Couldn't save the profiles to " + path;
        var written = f.writeText(text);
        f.close();
        if (written < 0) return "Couldn't save the profiles to " + path;
    } catch (e) { return "Couldn't save the profiles: " + e; }
    return "";
}

// ── Menu item + window ───────────────────────────────────────────────────────

// "&&": Qt reads a single "&" in a menu name as a keyboard-mnemonic marker and
// hides it (the menu read "Copy-Paste Conditions  Automation").
studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Copy-Paste Conditions && Automation",
    execute: function () {

        var W = studio.ui.widgetType;
        var G = studio.ui.layoutType;
        // Track automation shares the Automation & Modulation checkbox.
        var PART_IDS   = { conditions: "ccp_incCond", automation: "ccp_incAuto", settings: "ccp_incSet",
                           tracks: "ccp_incAuto" };
        var PART_NAMES = { conditions: "Trigger Conditions", automation: "Automation & Modulation",
                           settings: "Instrument Settings", tracks: "Track Automation" };
        var PART_TEXT  = { conditions: "trigger conditions", automation: "automation & modulation",
                           settings: "instrument settings", tracks: "track automation" };
        var GREY = "<span style=\"color:#9a9a9a;\">";
        var state = { cache: {}, timerId: null, edSig: null, deckSig: null, preferDeck: false, clipItem: null };
        var loadMessage = ccpLoadProfiles();
        ccpClearCharts();

        function widget(w, id) {
            try { return w.findWidget(id); } catch (e) { return null; }
        }

        function isChecked(w, id) {
            var cb = widget(w, id);
            try { return !!(cb && cb.isChecked()); } catch (e) { return false; }
        }

        // The parts ticked under "Include", in order.
        function included(w) {
            var out = [];
            for (var i = 0; i < CCP_PARTS.length; i++) {
                if (isChecked(w, PART_IDS[CCP_PARTS[i]])) out.push(CCP_PARTS[i]);
            }
            return out;
        }

        function rememberOptions(w) {
            ccpOptions.conditions = isChecked(w, PART_IDS.conditions);
            ccpOptions.automation = isChecked(w, PART_IDS.automation);
            ccpOptions.settings = isChecked(w, PART_IDS.settings);
            ccpOptions.replace = isChecked(w, "ccp_replace");
            ccpOptions.sheets = isChecked(w, "ccp_addSheets");
        }

        function setLog(w, lines) {
            var text = lines.join("\n");
            var box = widget(w, "ccp_log");
            if (box) { try { box.setText(text); } catch (e) {} }
            studio.system.print("=== " + CCP_TITLE + " v" + CCP_VERSION + " ===\n" + text + "\n");
        }

        // Sets a widget's text / enabled state only when it changed.
        function setOnce(w, id, what, value) {
            var key = id + "|" + what;
            if (state.cache[key] === value) return;
            state.cache[key] = value;
            var x = widget(w, id);
            if (!x) return;
            try {
                if (what === "text") x.setText(value);
                else if (what === "html") x.setHtml(value);
                else x.setEnabled(value);
            } catch (e) {}
        }

        // "…/Folder/Event": keeps the end of a path, which says the most.
        function truncStart(s, n) {
            s = "" + s;
            return s.length > n ? "…" + s.substring(s.length - n + 1) : s;
        }

        function trunc(s, n) {
            s = "" + s;
            return s.length > n ? s.substring(0, n - 1) + "…" : s;
        }

        // ── Selection ──
        // The playlist (deck) selection wins only while it belongs to the
        // instrument selected in the event editor and was changed more
        // recently than the event editor's selection.
        function sig(arr) {
            var ids = [];
            for (var i = 0; i < arr.length; i++) ids.push(ccpId(arr[i]));
            return ids.join(",");
        }

        function observeSelection() {
            var ed = ccpEditorSelection();
            var dk = ccpDeckEntries(ed);
            var edSig = sig(ed), dkSig = sig(dk.entries);
            if (dkSig !== state.deckSig) state.preferDeck = dk.entries.length > 0;
            else if (edSig !== state.edSig) state.preferDeck = false;
            state.edSig = edSig;
            state.deckSig = dkSig;
            var fromDeck = state.preferDeck && dk.entries.length > 0;
            return { raw: fromDeck ? dk.entries : ed, fromDeck: fromDeck, parent: dk.parent };
        }

        // Targets for the given parts: instruments for automation, anything
        // triggerable (loop regions, transition markers too) for conditions,
        // tracks for track automation. Instruments win; tracks are used when
        // none are selected (`mode` says which).
        function selectionFor(parts) {
            var o = observeSelection();
            var inst = [];
            for (var i = 0; i < parts.length; i++) if (parts[i] !== "tracks") inst.push(parts[i]);
            var f = { targets: [], total: o.raw.length };
            if (inst.length) {
                // Only conditions also go on loop regions and transition markers.
                f = ccpFilterTargets(o.raw, inst.indexOf("conditions") >= 0 ? "conditions" : "automation");
            }
            if (!f.targets.length && parts.indexOf("tracks") >= 0 && !o.fromDeck) {
                var t = ccpTrackSelection(o.raw);
                if (t.targets.length) {
                    return { targets: t.targets, total: t.total, fromDeck: false, parent: null, mode: "tracks", picks: t.picks };
                }
            }
            return { targets: f.targets, total: f.total, fromDeck: o.fromDeck, parent: o.parent, mode: "instruments" };
        }

        // The one object to copy from. With several selected, the focused one.
        function pickSource(sel) {
            if (sel.targets.length === 1) return { obj: sel.targets[0] };
            if (sel.targets.length === 0) {
                return { error: "Select an instrument in the event editor to copy from." +
                                (sel.total ? "\n(" + ccpPlural(sel.total, "selected item") +
                                             ", none of them an instrument.)" : "") };
            }
            var cur = ccpCurrent(sel.fromDeck);
            for (var i = 0; i < sel.targets.length; i++) {
                if (cur && ccpId(sel.targets[i]) === ccpId(cur)) return { obj: sel.targets[i] };
            }
            return { error: sel.targets.length + " items are selected. Select just one to copy from." };
        }

        // The clipboard's source id (the same instrument for every part).
        function sourceId(c) {
            for (var i = 0; c && i < CCP_PARTS.length; i++) if (c[CCP_PARTS[i]]) return c[CCP_PARTS[i]].sourceId;
            return "";
        }

        function sourceName(c) {
            for (var i = 0; c && i < CCP_PARTS.length; i++) if (c[CCP_PARTS[i]]) return c[CCP_PARTS[i]].sourceName;
            return "";
        }

        function sourceDesc(c) {
            for (var i = 0; c && i < CCP_PARTS.length; i++) if (c[CCP_PARTS[i]]) return c[CCP_PARTS[i]].source;
            return "";
        }

        // Paste targets: the selection minus the instrument the clipboard came from.
        function pasteTargets(sel, clip) {
            var out = [];
            var id = sourceId(clip);
            for (var i = 0; i < sel.targets.length; i++) {
                if (ccpId(sel.targets[i]) !== id) out.push(sel.targets[i]);
            }
            return out;
        }

        // Parts that are both ticked and on the clipboard (or a profile).
        function pastable(w, c) {
            var parts = included(w), out = [];
            for (var i = 0; c && i < parts.length; i++) if (c[parts[i]]) out.push(parts[i]);
            return out;
        }

        // ── Describing the clipboard and profiles ──
        function partSummary(part, d) {
            if (part === "conditions") return ccpPlural(d.items.length, "condition");
            if (part === "settings") return ccpPlural(ccpSettingsCount(d), "setting");
            if (part === "tracks") return ccpPlural(d.lanes.length, "track automation lane");
            var c = ccpCountAutoMod(d.self);
            for (var i = 0; i < d.eventParams.length; i++) {
                var sub = ccpCountAutoMod(d.eventParams[i].data);
                c.automators += sub.automators;
                c.modulators += sub.modulators;
            }
            return ccpPlural(c.automators, "automation") + ", " + ccpPlural(c.modulators, "modulator");
        }

        function summary(c) {
            var parts = [];
            for (var i = 0; i < CCP_PARTS.length; i++) {
                if (c[CCP_PARTS[i]]) parts.push(partSummary(CCP_PARTS[i], c[CCP_PARTS[i]]));
            }
            return parts.join(" · ");
        }

        function contentLines(c) {
            var out = [];
            if (c.conditions) {
                out.push("Trigger conditions:");
                for (var i = 0; i < c.conditions.items.length; i++) out.push("    " + ccpConditionText(c.conditions.items[i]));
            }
            if (c.automation) {
                out.push("Automation & modulation:");
                var a = ccpInstrumentLines(c.automation);
                for (var k = 0; k < a.length; k++) out.push("    " + a[k]);
            }
            if (c.settings) {
                out.push("Instrument settings (" + ccpKind({ entity: c.settings.type }) + "):");
                var row = [];
                for (var r = 0; r < CCP_SETTINGS.length; r++) {
                    var key = CCP_SETTINGS[r][0];
                    if (Object.prototype.hasOwnProperty.call(c.settings.props, key)) {
                        row.push(CCP_SETTINGS[r][2] + " " + ccpSettingValue(CCP_SETTINGS[r], c.settings.props[key]));
                    }
                }
                for (var q = 0; q < row.length; q += 4) out.push("    " + row.slice(q, q + 4).join(" · "));
            }
            if (c.tracks) {
                out.push("Track automation:");
                for (var l = 0; l < c.tracks.lanes.length; l++) {
                    var lane = c.tracks.lanes[l], cs = [];
                    for (var cv = 0; cv < lane.curves.length; cv++) {
                        cs.push(lane.curves[cv].param.name + " (" + ccpPlural(lane.curves[cv].points.length, "point") + ")");
                    }
                    out.push("    " + lane.label + ": " + (cs.length ? cs.join(", ") : "no curves"));
                }
            }
            return out;
        }

        // Short counts for the clipboard line and the dropdown: "2 con · 1 auto
        // · 0 mod". Parts that weren't copied are left out.
        function counts(c) {
            var out = [];
            if (c.conditions) out.push({ n: c.conditions.items.length, label: "con", color: CCP_C.on });
            if (c.automation) {
                var k = ccpCountAutoMod(c.automation.self);
                for (var i = 0; i < c.automation.eventParams.length; i++) {
                    var sub = ccpCountAutoMod(c.automation.eventParams[i].data);
                    k.automators += sub.automators;
                    k.modulators += sub.modulators;
                }
                out.push({ n: k.automators, label: "auto", color: CCP_C.curve });
                out.push({ n: k.modulators, label: "mod", color: CCP_C.env });
            }
            if (c.settings) out.push({ n: ccpSettingsCount(c.settings), label: "set", color: CCP_C.settings });
            if (c.tracks) out.push({ n: c.tracks.lanes.length, label: "lane", color: CCP_C.curve });
            return out;
        }

        function shortSummary(c) {
            var parts = counts(c), out = [];
            for (var i = 0; i < parts.length; i++) out.push(parts[i].n + " " + parts[i].label);
            return out.join(" · ");
        }

        // The line beside Copy: coloured counts and where they came from.
        function clipHtml() {
            var c = ccpClipboard;
            if (!c) return GREY + "Clipboard empty</span>";
            var parts = counts(c), out = [];
            for (var i = 0; i < parts.length; i++) {
                out.push("<span style=\"color:" + (parts[i].n ? parts[i].color : "#7a7a7a") + ";\"><b>" + parts[i].n +
                         "</b> " + parts[i].label + "</span>");
            }
            return out.join(GREY + " · </span>") + GREY + " · from </span><span style=\"color:#ffd84d;\">" +
                   ccpEscape(trunc(sourceName(c), 34)) + "</span>";
        }

        // ── "Paste from" dropdown: the clipboard first, then the saved profiles ──
        function comboItems() {
            var items = [{ text: ccpClipboard ? "Clipboard  ·  " + shortSummary(ccpClipboard) : "Clipboard (empty)", userData: -1 }];
            for (var i = 0; i < ccpProfiles.length; i++) items.push({ text: ccpProfiles[i].name, userData: i });
            return items;
        }

        function profileItems() {
            var items = [];
            for (var i = 0; i < ccpProfiles.length; i++) items.push({ text: ccpProfiles[i].name, userData: i });
            return items.length ? items : [{ text: "No saved profiles", userData: -1 }];
        }

        // Index of the chosen profile, or -1 when the clipboard is chosen.
        function chosen(w) {
            var cb = widget(w, "ccp_profiles");
            var i = 0;
            try { i = cb ? cb.currentIndex() : 0; } catch (e) { i = 0; }
            return (typeof i === "number" && i >= 1 && i <= ccpProfiles.length) ? i - 1 : -1;
        }

        // ComboBox items are write-only: set them again, then choose profile
        // `index` (-1 = the clipboard).
        function updateCombo(w, index) {
            var cb = widget(w, "ccp_profiles");
            if (!cb) return;
            index = Math.max(-1, Math.min(index, ccpProfiles.length - 1));
            try { cb.setItems(comboItems()); } catch (e) {}
            try { cb.setCurrentIndex(index + 1); } catch (e) {}
        }

        // What Paste pastes: the chosen profile, or the clipboard. Null when
        // the clipboard is chosen and empty.
        function pasteSource(w) {
            var i = chosen(w);
            if (i >= 0) return { data: ccpProfiles[i].data, profile: ccpProfiles[i].name, title: ccpProfiles[i].name };
            return ccpClipboard ? { data: ccpClipboard, profile: null, title: "Clipboard" } : null;
        }

        // ── Preview (right pane) ──
        // Draws exactly what Paste will paste: the source chosen in "Paste
        // from" (the clipboard or a profile): condition cards with range bars,
        // a chart per automation curve, a tag per modulator.
        function previewSubject(w) {
            var src = pasteSource(w);
            return src ? { title: src.title, what: src.profile ? "Profile" : "", data: src.data } : null;
        }

        function previewTitleHtml(w) {
            var p = previewSubject(w);
            if (!p) return "<b>Preview</b><br/>" + GREY + "The clipboard is empty. Copy from an instrument, or pick a saved profile under Paste from.</span>";
            return "<span style=\"font-size:15px; font-weight:bold; color:#ffd84d;\">" + ccpEscape(p.title) + "</span><br/>" +
                   GREY + (p.what ? p.what + " · " : "") + ccpEscape(summary(p.data)) +
                   " · copied from " + ccpEscape(sourceName(p.data)) + "</span>";
        }

        function previewHtml(w) {
            var p = previewSubject(w);
            if (!p) return "";
            var parts = included(w), out = "";
            for (var i = 0; i < CCP_PARTS.length; i++) {
                var part = CCP_PARTS[i], d = p.data[part];
                if (!d) continue;
                var off = parts.indexOf(part) < 0;
                out += "<p style=\"margin-top:4px;\"><b>" + PART_NAMES[part] + "</b>" +
                       (off ? ccpSpan(CCP_C.warn, "&nbsp;&nbsp;not ticked under Include — won't paste", "font-size:11px;") : "") +
                       "</p>" + (part === "conditions" ? ccpConditionsHtml(d) :
                                 part === "settings" ? ccpSettingsHtml(d) :
                                 part === "tracks" ? ccpTracksHtml(d) : ccpAutomationHtml(d));
            }
            return "<body style=\"color:" + CCP_C.text + ";\">" + out + "</body>";
        }

        // ── Small modal dialogs ──
        function buttonRow(buttons) {
            return { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 6,
                     items: [{ widgetType: W.Spacer }].concat(buttons) };
        }

        // Returns the trimmed name entered, or null if cancelled.
        function askName(title, prompt, initial, okText) {
            var result = null;
            studio.ui.showModalDialog({
                windowTitle: title, windowWidth: 380, widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
                items: [
                    { widgetType: W.Label, wordWrap: true, text: prompt },
                    { widgetType: W.LineEdit, widgetId: "ccp_askName", text: initial },
                    buttonRow([
                        { widgetType: W.PushButton, widgetId: "ccp_askCancel", text: "Cancel",
                          onClicked: function () { this.closeDialog(); } },
                        { widgetType: W.PushButton, widgetId: "ccp_askOk", text: okText,
                          onClicked: function () {
                              var box = this.findWidget("ccp_askName");
                              result = String.prototype.trim.call(box ? "" + box.text() : "").substring(0, CCP_NAME_MAX);
                              this.closeDialog();
                          } }
                    ])
                ]
            });
            return result;
        }

        function confirmBox(title, text, okText) {
            var ok = false;
            studio.ui.showModalDialog({
                windowTitle: title, windowWidth: 380, widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
                items: [
                    { widgetType: W.Label, wordWrap: true, text: text },
                    buttonRow([
                        { widgetType: W.PushButton, widgetId: "ccp_confirmCancel", text: "Cancel",
                          onClicked: function () { this.closeDialog(); } },
                        { widgetType: W.PushButton, widgetId: "ccp_confirmOk", text: okText,
                          onClicked: function () { ok = true; this.closeDialog(); } }
                    ])
                ]
            });
            return ok;
        }

        function saveTo(lines) {
            var msg = ccpSaveProfiles();
            if (msg) lines.push("! " + msg);
            return lines;
        }

        // ── Actions ──
        // Copy replaces the whole clipboard: the ticked parts are captured,
        // the others cleared, so a paste never mixes two sources.
        function doCopy(w) {
            var parts = included(w);
            if (!parts.length) { setLog(w, ["Copy: tick at least one of Trigger Conditions, Automation & Modulation, Instrument Settings."]); return; }
            var sel = selectionFor(parts);
            var pick = pickSource(sel);
            if (pick.error) { setLog(w, ["Copy: " + pick.error]); return; }
            if (sel.mode === "tracks") { copyTrack(w, parts, ccpCaptureTrack(pick.obj, ccpPickedLanes(sel, pick.obj))); return; }
            parts = parts.filter(function (p) { return p !== "tracks"; });

            var c = { conditions: null, automation: null, settings: null, tracks: null };
            var lines = [], empties = [], notes = [], skipped = [], source = "";
            for (var i = 0; i < parts.length; i++) {
                var part = parts[i];
                if (part !== "conditions" && !ccpIsInstrument(pick.obj)) { empties.push(PART_TEXT[part] + " (not an instrument)"); continue; }
                var d = (part === "conditions") ? ccpCaptureConditions(pick.obj) :
                        (part === "settings") ? ccpCaptureSettings(pick.obj) : ccpCaptureInstrument(pick.obj);
                source = d.source;
                for (var n = 0; d.notes && n < d.notes.length; n++) notes.push(d.notes[n]);
                for (var k = 0; d.skipped && k < d.skipped.length; k++) skipped.push(d.skipped[k]);
                var empty = (part === "conditions") ? d.items.length === 0 :
                            (part === "settings") ? ccpSettingsCount(d) === 0 :
                            (ccpAutoModIsEmpty(d.self) && d.eventParams.length === 0);
                if (empty) empties.push(part === "automation" ? "automation or modulation" : PART_TEXT[part]);
                else c[part] = d;
            }
            if (!c.conditions && !c.automation && !c.settings) {
                lines.push("Copy: " + (source || ccpDescribe(pick.obj)) + " has no " + empties.join(" and no ") +
                           " — nothing copied.");
                for (var e = 0; e < notes.length; e++) lines.push(notes[e]);
                if (ccpClipboard) lines.push("The clipboard still holds its previous copy.");
                setLog(w, lines);
                return;
            }
            ccpClipboard = c;
            updateCombo(w, -1);                 // Paste from the clipboard now
            lines.push("Copied " + summary(c) + " from " + source + ":");
            var body = contentLines(c);
            for (var b = 0; b < body.length; b++) lines.push("    " + body[b]);
            for (var s = 0; s < skipped.length; s++) lines.push("    ! not copied: " + skipped[s]);
            if (empties.length) lines.push("    (no " + empties.join(", no ") + " to copy)");
            for (var x = 0; x < notes.length; x++) lines.push(notes[x]);
            lines.push("");
            lines.push("Now select the instrument(s) to paste onto and press Paste. " +
                       "Save as… keeps it as a profile.");
            setLog(w, lines);
        }

        // Copy from a track: only its automation lanes. Conditions and
        // settings belong to instruments, so a track has none to copy.
        function copyTrack(w, parts, d) {
            var lines = [];
            if (!d.lanes.length) {
                lines.push("Copy: " + d.source + " has no automation lanes" +
                           (d.skipped.length ? " this tool can copy" : "") + " — nothing copied.");
                for (var k = 0; k < d.skipped.length; k++) lines.push("    ! not copied: " + d.skipped[k]);
                if (ccpClipboard) lines.push("The clipboard still holds its previous copy.");
                setLog(w, lines);
                return;
            }
            ccpClipboard = { conditions: null, automation: null, settings: null, tracks: d };
            updateCombo(w, -1);
            lines.push("Copied " + summary(ccpClipboard) + " from " + d.source + ":");
            var body = contentLines(ccpClipboard);
            for (var b = 0; b < body.length; b++) lines.push("    " + body[b]);
            for (var s = 0; s < d.skipped.length; s++) lines.push("    ! not copied: " + d.skipped[s]);
            if (parts.indexOf("conditions") >= 0 || parts.indexOf("settings") >= 0) {
                lines.push("    (trigger conditions and instrument settings are copied from instruments, not tracks)");
            }
            lines.push("");
            lines.push("Now select the track(s) to paste onto and press Paste. Missing lanes are added; " +
                       "effects aren't, so a lane on an effect the target track lacks is skipped.");
            setLog(w, lines);
        }

        function doPaste(w) {
            var src = pasteSource(w);
            if (!src) { setLog(w, ["Paste: the clipboard is empty — Copy from an instrument, or pick a saved profile under Paste from."]); return; }
            var clip = src.data;
            var parts = pastable(w, clip);
            if (!parts.length) {
                setLog(w, ["Paste: " + (src.profile ? "profile \"" + src.profile + "\"" : "the clipboard") + " holds " +
                           summary(clip) + ", but that part isn't ticked under Include."]);
                return;
            }

            var sel = selectionFor(parts);
            if (sel.targets.length === 0 && clip.tracks && parts.indexOf("tracks") >= 0) {
                setLog(w, ["Paste: select one or more tracks (or a lane on each) in the event editor to paste the track automation onto."]);
                return;
            }
            if (sel.targets.length === 0) {
                setLog(w, ["Paste: select one or more instruments" +
                           (parts.indexOf("conditions") >= 0 ? " (or loop regions / transition markers)" : "") +
                           " in the event editor to paste onto."]);
                return;
            }

            var replace = isChecked(w, "ccp_replace");
            var addSheets = isChecked(w, "ccp_addSheets");
            var stats = { pasted: 0, skipped: 0, failed: 0, targets: 0, paramsDone: {}, addSheets: addSheets };
            var names = [];
            for (var p = 0; p < parts.length; p++) names.push(PART_TEXT[parts[p]]);
            var lines = ["Pasting " + names.join(" and ") + " (" + sourceDesc(clip) +
                         (src.profile ? ", profile \"" + src.profile + "\"" : ", clipboard") + ")" +
                         (replace ? "  [replace]" : "  [merge]") +
                         (addSheets ? "  [add missing parameter sheets]" : "")];

            for (var i = 0; i < sel.targets.length; i++) {
                var t = sel.targets[i];
                lines.push("");
                lines.push("→ " + ccpDescribe(t));
                if (ccpId(t) === sourceId(clip)) {
                    lines.push("    = skipped — this is the " + (sel.mode === "tracks" ? "track" : "instrument") +
                               " the data was copied from");
                    continue;
                }
                stats.targets++;
                for (var k = 0; k < parts.length; k++) {
                    var part = parts[k];
                    if (parts.length > 1) lines.push("  " + PART_NAMES[part] + ":");
                    if (part === "tracks") {
                        try { ccpPasteTrack(clip.tracks, t, replace, lines, stats); }
                        catch (e) { lines.push("    ! stopped on an error: " + e); stats.failed++; }
                        continue;
                    }
                    if (part !== "conditions" && !ccpIsInstrument(t)) {
                        lines.push("    = no " + (part === "settings" ? "settings" : "automation") + " pasted — " +
                                   ccpKind(t) + " isn't an instrument");
                        continue;
                    }
                    try {
                        if (part === "conditions")    ccpPasteConditions(clip.conditions, t, replace, addSheets, lines, stats);
                        else if (part === "settings") ccpPasteSettings(clip.settings, t, lines, stats);
                        else                          ccpPasteInstrument(clip.automation, t, replace, addSheets, lines, stats);
                    } catch (e) {
                        lines.push("    ! stopped on an error: " + e);
                        stats.failed++;
                    }
                }
            }

            lines.push("");
            lines.push("Done: " + ccpPlural(stats.pasted, "item") + " pasted onto " +
                       ccpPlural(stats.targets, "target") +
                       (stats.skipped ? ", " + stats.skipped + " already present" : "") +
                       (stats.failed ? ", " + stats.failed + " problem" + (stats.failed === 1 ? "" : "s") +
                                       " (marked !)" : "") + ".");
            var unticked = [];
            for (var u = 0; u < CCP_PARTS.length; u++) {
                if (clip[CCP_PARTS[u]] && parts.indexOf(CCP_PARTS[u]) < 0) unticked.push(PART_NAMES[CCP_PARTS[u]]);
            }
            if (unticked.length) lines.push("Not pasted (unticked under Include): " + unticked.join(", ") + ".");
            setLog(w, lines);
        }

        // Saves the clipboard as a named profile (the Copy zone's Save button).
        function doSaveAs(w) {
            var clip = ccpClipboard;
            if (!clip) { setLog(w, ["Save as profile: the clipboard is empty — Copy from an instrument first."]); return; }
            var name = askName("Save as Profile",
                               "Save the clipboard (" + ccpEscape(summary(clip)) + ", copied from " +
                               ccpEscape(sourceName(clip)) + ") as a profile named:",
                               ccpUniqueName(sourceName(clip) || "Profile"), "Save");
            if (name === null) return;
            if (!name) { setLog(w, ["Save as profile: a profile needs a name — nothing saved."]); return; }

            var at = ccpFindProfile(name);
            var replaced = at >= 0;
            if (replaced) {
                if (!confirmBox("Replace Profile",
                                "A profile called \"" + ccpEscape(ccpProfiles[at].name) +
                                "\" already exists. Replace it with the clipboard?", "Replace")) return;
                ccpProfiles[at] = { name: ccpProfiles[at].name, data: clip };
            } else {
                if (ccpProfiles.length >= CCP_MAX_PROFILES) {
                    setLog(w, ["Save as profile: there are already " + CCP_MAX_PROFILES + " profiles. Delete one in Manage… first."]);
                    return;
                }
                ccpProfiles.push({ name: name, data: clip });
                at = ccpProfiles.length - 1;
            }
            updateCombo(w, at);
            setLog(w, saveTo([(replaced ? "Replaced" : "Saved") + " profile \"" + ccpProfiles[at].name +
                              "\" (" + summary(clip) + "). It's chosen under Paste from."]));
        }

        // Rename, replace with the clipboard, or delete saved profiles.
        function doManage(w) {
            if (!ccpProfiles.length) { setLog(w, ["Manage: there are no saved profiles yet."]); return; }
            var report = [];
            var wasProfile = chosen(w);
            var start = Math.max(0, wasProfile);

            function mChosen(m) {
                var n = ccpProfiles.length;
                var cb = m.findWidget("ccp_mCombo");
                var i = 0;
                try { i = cb ? cb.currentIndex() : 0; } catch (e) { i = 0; }
                return n ? Math.max(0, Math.min(i, n - 1)) : -1;
            }

            function mShow(m, status) {
                var i = mChosen(m);
                var info = "", name = "";
                if (i >= 0) {
                    var p = ccpProfiles[i];
                    name = p.name;
                    var body = contentLines(p.data);
                    info = "<b>" + ccpEscape(summary(p.data)) + "</b> " + GREY + "· copied from " +
                           ccpEscape(sourceDesc(p.data)) + "</span><br/>" +
                           ccpEscape(body.slice(0, 14).join("\n")).split("\n").join("<br/>") +
                           (body.length > 14 ? "<br/>…" : "");
                } else {
                    info = GREY + "No profiles left.</span>";
                }
                try { m.findWidget("ccp_mInfo").setText(info); } catch (e) {}
                try { m.findWidget("ccp_mName").setText(name); } catch (e) {}
                try { m.findWidget("ccp_mStatus").setText(status || ""); } catch (e) {}
                var any = i >= 0;
                try { m.findWidget("ccp_mRename").setEnabled(any); } catch (e) {}
                try { m.findWidget("ccp_mDelete").setEnabled(any); } catch (e) {}
                try { m.findWidget("ccp_mReplace").setEnabled(any && !!ccpClipboard); } catch (e) {}
            }

            function mReload(m, index, status) {
                var cb = m.findWidget("ccp_mCombo");
                try { cb.setItems(profileItems()); } catch (e) {}
                try { cb.setCurrentIndex(Math.max(0, index)); } catch (e) {}
                mShow(m, status);
            }

            function mSave(line) {
                report.push(line);
                var msg = ccpSaveProfiles();
                if (msg) report.push("! " + msg);
            }

            studio.ui.showModalDialog({
                windowTitle: "Manage Profiles",
                windowWidth: 440, widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
                items: [
                    { widgetType: W.ComboBox, widgetId: "ccp_mCombo", items: profileItems(), currentIndex: start,
                      onCurrentIndexChanged: function () { mShow(this, ""); } },
                    { widgetType: W.Label, widgetId: "ccp_mInfo", wordWrap: true, text: "" },
                    { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 6, items: [
                        { widgetType: W.LineEdit, widgetId: "ccp_mName", text: "" },
                        { widgetType: W.PushButton, widgetId: "ccp_mRename", text: "Rename",
                          onClicked: function () {
                              var i = mChosen(this);
                              if (i < 0) return;
                              var box = this.findWidget("ccp_mName");
                              var name = String.prototype.trim.call(box ? "" + box.text() : "").substring(0, CCP_NAME_MAX);
                              if (!name) { mShow(this, "Type a name first."); return; }
                              if (ccpFindProfile(name, i) >= 0) { mShow(this, "Another profile is already called \"" + ccpEscape(name) + "\"."); return; }
                              var old = ccpProfiles[i].name;
                              ccpProfiles[i].name = name;
                              mSave("Renamed profile \"" + old + "\" to \"" + name + "\".");
                              mReload(this, i, "Renamed.");
                          } }
                    ] },
                    { widgetType: W.Label, widgetId: "ccp_mStatus", wordWrap: true, text: "" },
                    buttonRow([
                        { widgetType: W.PushButton, widgetId: "ccp_mReplace", text: "Replace with Clipboard",
                          onClicked: function () {
                              var i = mChosen(this);
                              var clip = ccpClipboard;
                              if (i < 0 || !clip) return;
                              var p = ccpProfiles[i];
                              if (!confirmBox("Replace Profile", "Replace profile \"" + ccpEscape(p.name) + "\" (" +
                                              ccpEscape(summary(p.data)) + ") with the clipboard (" +
                                              ccpEscape(summary(clip)) + ", copied from " +
                                              ccpEscape(sourceName(clip)) + ")?", "Replace")) return;
                              p.data = clip;
                              mSave("Replaced profile \"" + p.name + "\" with the clipboard (" + summary(clip) + ").");
                              mReload(this, i, "Replaced.");
                          } },
                        { widgetType: W.PushButton, widgetId: "ccp_mDelete", text: "Delete",
                          onClicked: function () {
                              var i = mChosen(this);
                              if (i < 0) return;
                              var p = ccpProfiles[i];
                              if (!confirmBox("Delete Profile", "Delete profile \"" + ccpEscape(p.name) +
                                              "\"? This can't be undone.", "Delete")) return;
                              ccpProfiles.splice(i, 1);
                              mSave("Deleted profile \"" + p.name + "\".");
                              mReload(this, Math.min(i, ccpProfiles.length - 1), "Deleted.");
                          } },
                        { widgetType: W.PushButton, widgetId: "ccp_mClose", text: "Close",
                          onClicked: function () { this.closeDialog(); } }
                    ])
                ],
                onConstructed: function () { mShow(this, ""); }
            });

            // Keep the main dropdown on what it showed (the clipboard stays chosen).
            updateCombo(w, wasProfile < 0 ? -1 : Math.min(wasProfile, ccpProfiles.length - 1));
            if (report.length) setLog(w, report);
        }

        // ── Live status, every 500 ms ──
        function refresh(w) {
            rememberOptions(w);
            var parts = included(w);
            var sel = selectionFor(parts.length ? parts : ["conditions"]);
            // Always three lines in a fixed-height box, with long names and
            // paths shortened, so selecting things never moves the window's
            // contents. The third line is the playlist note, or blank.
            var n = sel.targets.length;
            var big, small, third = "&nbsp;";
            if (n === 0) {
                big   = "<span style=\"font-size:15px; color:#9a9a9a;\">No instrument selected</span>";
                small = GREY + "Click one in the event editor" +
                        (parts.indexOf("tracks") >= 0 ? ", or a track's automation lane" : "") + "</span>";
            } else if (n === 1) {
                var t = sel.targets[0];
                var path = ccpEventPath(ccpEventOf(t));
                big   = "<span style=\"font-size:15px; font-weight:bold; color:#ffd84d;\">" +
                        ccpEscape(trunc(ccpShortName(t), 44)) + "</span>";
                small = GREY + ccpEscape(ccpKind(t) + (path ? "  ·  " + truncStart(path, 56) : "")) + "</span>";
            } else {
                var names = [];
                for (var k = 0; k < n && k < 3; k++) names.push(ccpShortName(sel.targets[k]));
                big   = "<span style=\"font-size:15px; font-weight:bold; color:#ffd84d;\">" +
                        n + " selected</span>";
                small = GREY + ccpEscape(trunc(names.join(", ") + (n > 3 ? ", …" : ""), 60)) +
                        "  ·  Paste applies to all</span>";
            }
            if (sel.mode === "tracks" && n === 1) {
                var picked = ccpPickedLanes(sel, sel.targets[0]);
                var all = ccpList(sel.targets[0], "automationTracks");
                var labels = [];
                for (var pl = 0; picked && pl < picked.length && pl < 4; pl++) {
                    var ld = ccpCaptureLane(picked[pl], sel.targets[0]);
                    labels.push(ld.error ? "?" : ld.label);
                }
                third = "<span style=\"color:#8fc7ff;\">↳ " + (picked
                        ? ccpEscape(trunc(ccpPlural(picked.length, "lane") + ": " + labels.join(", ") +
                                          (picked.length > 4 ? ", …" : ""), 62))
                        : "whole track · " + ccpPlural(all.length, "automation lane")) + "</span>";
            }
            if (sel.fromDeck && n > 0) {
                third = "<span style=\"color:#8fc7ff;\">↳ " + ccpPlural(n, "playlist entry") + " of " +
                        ccpEscape(trunc(ccpShortName(sel.parent), 30)) + "</span>" +
                        GREY + " · deselect to target the whole instrument</span>";
            }
            setOnce(w, "ccp_selection", "text",
                    "<table width=\"100%\" cellpadding=\"6\" bgcolor=\"#3b3a26\"><tr><td>" +
                    big + "<br/>" + small + "<br/>" + third + "</td></tr></table>");

            setOnce(w, "ccp_clip", "text", clipHtml());

            // The Copy button has a fixed width; its text is cut to fit.
            var pick = parts.length ? pickSource(sel) : { error: "none ticked" };
            setOnce(w, "ccp_copy", "text", !parts.length ? "Copy (tick Include)"
                    : pick.obj ? "Copy from " + trunc(ccpShortName(pick.obj), 18)
                    : (sel.targets.length > 1 ? "Copy (select one)" : "Copy"));
            setOnce(w, "ccp_copy", "enabled", !!pick.obj);

            // The dropdown's clipboard entry describes the clipboard.
            var clipItem = ccpClipboard ? shortSummary(ccpClipboard) : "";
            if (state.clipItem !== clipItem) {
                state.clipItem = clipItem;
                updateCombo(w, chosen(w));
            }

            var src = pasteSource(w);
            var clip = src ? src.data : null;
            var pp = pastable(w, clip);
            var targets = pasteTargets(pp.length ? selectionFor(pp) : sel, clip);
            setOnce(w, "ccp_paste", "text", !clip ? "Paste  (the clipboard is empty)"
                    : !pp.length ? "Paste  (nothing ticked is in " + (src.profile ? "this profile" : "the clipboard") + ")"
                    : targets.length === 1 ? "Paste onto " + trunc(ccpShortName(targets[0]), 32)
                    : targets.length > 1 ? "Paste onto " + targets.length + " selected"
                    : "Paste  (select a target)");
            setOnce(w, "ccp_paste", "enabled", pp.length > 0 && targets.length > 0);

            setOnce(w, "ccp_manage", "enabled", ccpProfiles.length > 0);
            setOnce(w, "ccp_save", "enabled", !!ccpClipboard);

            setOnce(w, "ccp_previewTitle", "text", previewTitleHtml(w));
            setOnce(w, "ccp_preview", "html", previewHtml(w));
        }

        function guarded(fn) {
            return function () {
                var w = this;
                try { fn(w); refresh(w); }
                catch (e) { setLog(w, ["Unexpected error: " + e]); }
            };
        }

        // A zone heading: "1  Copy  to the clipboard".
        function zone(n, title, sub) {
            return { widgetType: W.Label, text: "<table width=\"100%\" cellpadding=\"3\" bgcolor=\"#2a2a2a\"><tr><td>" +
                     "<span style=\"color:#ffd84d;\"><b>" + n + "</b></span>&nbsp;&nbsp;<b>" + title + "</b>&nbsp;&nbsp;" +
                     GREY + sub + "</span></td></tr></table>" };
        }

        var leftItems = [
                { widgetType: W.Label, widgetId: "ccp_selection", wordWrap: false, text: "",
                  minimumHeight: 74, maximumHeight: 74 },

                // ── 1  Copy: from an instrument to the clipboard, which can be saved.
                zone("1", "Copy", "from the selected instrument or track lanes to the clipboard"),
                { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 12, items: [
                    { widgetType: W.Label, text: "Include" },
                    { widgetType: W.CheckBox, widgetId: PART_IDS.conditions, isChecked: ccpOptions.conditions,
                      text: PART_NAMES.conditions, onToggled: guarded(function () {}) },
                    // "&&": Qt reads a single "&" in a CheckBox as a mnemonic
                    // and hides it ("Automation _Modulation").
                    { widgetType: W.CheckBox, widgetId: PART_IDS.automation, isChecked: ccpOptions.automation,
                      text: PART_NAMES.automation.replace("&", "&&"), onToggled: guarded(function () {}) },
                    { widgetType: W.CheckBox, widgetId: PART_IDS.settings, isChecked: ccpOptions.settings,
                      text: PART_NAMES.settings, onToggled: guarded(function () {}) },
                    { widgetType: W.Spacer }
                ] },
                // Copy and Save as profile… side by side at fixed widths, and the
                // clipboard's counts on one line below (shortened, never
                // wrapped), so nothing here changes height or position.
                { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
                    { widgetType: W.PushButton, widgetId: "ccp_copy", text: "Copy",
                      minimumWidth: 220, maximumWidth: 220,
                      sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed },
                      onClicked: guarded(function (w) { doCopy(w); }) },
                    { widgetType: W.PushButton, widgetId: "ccp_save", text: "Save as profile…",
                      minimumWidth: 140, maximumWidth: 140,
                      sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed },
                      onClicked: guarded(function (w) { doSaveAs(w); }) },
                    { widgetType: W.Spacer }
                ] },
                { widgetType: W.Label, widgetId: "ccp_clip", wordWrap: false, text: "",
                  minimumHeight: 18, maximumHeight: 18 },

                // ── 2  Paste: from the clipboard or a saved profile.
                zone("2", "Paste", "the clipboard or a saved profile onto the selection"),
                { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 6, items: [
                    { widgetType: W.Label, text: "Paste from" },
                    { widgetType: W.ComboBox, widgetId: "ccp_profiles", items: comboItems(), currentIndex: 0,
                      minimumWidth: 260, onCurrentIndexChanged: guarded(function () {}) },
                    { widgetType: W.PushButton, widgetId: "ccp_manage", text: "Manage…",
                      onClicked: guarded(function (w) { doManage(w); }) },
                    { widgetType: W.Spacer }
                ] },
                // Paste: the main action, full width and tall.
                { widgetType: W.PushButton, widgetId: "ccp_paste", text: "Paste", isEnabled: false,
                  minimumHeight: 44, onClicked: guarded(function (w) { doPaste(w); }) },

                { widgetType: W.Label, text: GREY + "Paste options</span>" },
                { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 12, items: [
                    { widgetType: W.CheckBox, widgetId: "ccp_replace", isChecked: ccpOptions.replace,
                      text: "Replace existing", onToggled: guarded(function () {}) },
                    { widgetType: W.CheckBox, widgetId: "ccp_addSheets", isChecked: ccpOptions.sheets,
                      text: "Add missing parameter sheets", onToggled: guarded(function () {}) },
                    { widgetType: W.Spacer }
                ] },

                { widgetType: W.Label, wordWrap: true,
                  text: GREY + "Paste uses what's chosen under Paste from (shown in the Preview). " +
                        "A modulator the target already has is updated, not doubled.</span>" },

                { widgetType: W.TextEdit, widgetId: "ccp_log", isReadOnly: true,
                  minimumHeight: 120, text: "" }
        ];

        // The Preview pane: what Paste will paste, drawn.
        var rightItems = [
                { widgetType: W.Label, widgetId: "ccp_previewTitle", wordWrap: true, text: "" },
                { widgetType: W.TextEdit, widgetId: "ccp_preview", isReadOnly: true,
                  minimumWidth: 380, minimumHeight: 420, html: "" }
        ];

        studio.ui.showModelessDialog({
            windowTitle:  CCP_TITLE + "  v" + CCP_VERSION,
            windowWidth:  960,
            windowHeight: 720,
            widgetType:   W.Layout,
            layout:       G.HBoxLayout,
            spacing:      16,
            items: [
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8, items: leftItems },
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 6, items: rightItems }
            ],
            onConstructed: function () {
                if (loadMessage) { try { setLog(this, ["! " + loadMessage]); } catch (e) {} }
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
