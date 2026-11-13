// FMOD Studio Script — Batch Event Editor
// Version: 0.10.1
// ---------------------------------------------------------------------------
// A NON-BLOCKING (modeless) window that follows the Event Browser selection
// and shows everything editable about the selected events, grouped so that
// patterns and outliers stand out:
//
//   Patterns      events grouped by their master-track effect chain
//   Differences   a written brief: what differs, the outliers, and which
//                 event differs the most and why
//   Effects       every insert on every track: shared or unique, bypass,
//                 values that differ from the default, automation/modulation
//   Spatializer   type, distance override, min/max, rolloff, 2D pan mix,
//                 envelopment — as distributions across the selection
//   Sheets        timeline / parameter sheet / action sheet, and which
//                 parameter sheets the events carry
//   Macros        the event macros (max instances, stealing, priority, …)
//   Tags & Notes
//   Per event     one compact block per event
//
// "Apply to" narrows every view (and the export) to one pattern group.
//
// Edits (Tags, Notes, Macros, Effects) act on the "Apply to" target. Each opens
// a modal with a live preview; every write is read back, and Undo reverts a
// batch. The inspector views themselves never write.
//
// Menu: Scripts > DAT-audio Utilities > Edit Events in Batch...
// Released in the DAT-audio Utilities pack as "Edit Events in Batch.js".
// Compatible with FMOD Studio 2.02.x and 2.03.x (2.03-only properties are
// read defensively).
// ---------------------------------------------------------------------------

var BEB_TITLE = "Batch Event Editor";
var BEB_VERSION = "0.10.1";
var BEB_PER_EVENT_CAP = 250;       // Per-event view: blocks shown before "… N more"

// Remembered while Studio runs (lost on Scripts > Reload).
var bebOptions = { recursive: true, view: 0 };

// ── Effect catalogue ────────────────────────────────────────────────────────
// Display names, and the properties worth showing, with their schema
// defaults (2.03.14 object model). Format: "name=default unit", where unit is
// optional and "#" marks an enum shown as its raw integer (mapping not
// verified). Values equal to the default are hidden in summaries.
var BEB_FX = {
    SpatialiserEffect:       { label: "Spatializer",        spatial: true },
    ObjectSpatialiserEffect: { label: "Object Spatializer", spatial: true },
    ThreeEQEffect:           { label: "3-EQ",               props: "lowGain=0 dB,midGain=0 dB,highGain=0 dB,lowCrossover=400 Hz,highCrossover=4000 Hz,crossoverSlope=1 #" },
    MultibandEqEffect:       { label: "Multiband EQ",       bands: true },
    GainEffect:              { label: "Gain",               props: "gain=0 dB" },
    CompressorEffect:        { label: "Compressor",         props: "threshold=0 dB,ratio=2.5 :1,attackTime=20 ms,releaseTime=200 ms,gain=0 dB,linkChannels=true" },
    LimiterEffect:           { label: "Limiter",            props: "boost=0 dB,ceiling=0 dB,release=10 ms,linkChannels=true" },
    MultibandDynamicsEffect: { label: "Multiband Dynamics", props: "lowerFrequency=400 Hz,upperFrequency=4000 Hz" },
    DelayEffect:             { label: "Delay",              props: "delay=500 ms,feedback=50 %,level=0 dB,dryLevel=0 dB" },
    SFXReverbEffect:         { label: "Reverb",             props: "decayTime=1500 ms,earlyDelay=20 ms,lateDelay=40 ms,HFReference=5000 Hz,HFDecayRatio=50 %,diffusion=100 %,density=100 %,lowShelfFrequency=250 Hz,lowShelfGain=0 dB,highCut=5000 Hz,earlyLateMix=50 %,wetLevel=-6 dB,dryLevel=0 dB" },
    ConvolutionReverbEffect: { label: "Convolution Reverb", props: "wetLevel=0 dB,dryLevel=-80 dB" },
    ChorusEffect:            { label: "Chorus",             props: "rate=0.8 Hz,depth=3 ms,mix=100 %" },
    FlangerEffect:           { label: "Flanger",            props: "rate=0.1 Hz,delay=1 ms,mix=50 %" },
    TremoloEffect:           { label: "Tremolo",            props: "frequency=5 Hz,depth=1,shape=0,skew=0,duty=0.5,square=0,phase=0,spread=0" },
    DistortionEffect:        { label: "Distortion",         props: "level=0.5" },
    PitchShifterEffect:      { label: "Pitch Shifter",      props: "pitch=0.5,fftSize=1024,overlap=4,maxChannels=0" },
    ITEchoEffect:            { label: "IT Echo",            props: "wetDryMix=50 %,feedback=50 %,leftDelay=500 ms,rightDelay=500 ms,panDelay=0" },
    ChannelMixEffect:        { label: "Channel Mix" },
    TransceiverEffect:       { label: "Transceiver",        props: "transmitMode=false,level=0 dB,channel=0" },
    LoudnessMeter:           { label: "Loudness Meter" },
    SpectrumAnalyzer:        { label: "Spectrum Analyzer" },
    SpectralSidechain:       { label: "Spectral Sidechain", props: "level=0 dB" },
    Sidechain:               { label: "Sidechain",          props: "level=0 dB" },
    MixerSend:               { label: "Send",               props: "level=-80 dB" },
    PannerEffect:            { label: "Panner" },
    HighpassEffect:          { label: "Highpass",           props: "cutoff=2000 Hz,resonance=1" },
    LowpassEffect:           { label: "Lowpass",            props: "cutoff=2000 Hz,resonance=1" },
    HighpassSimpleEffect:    { label: "Highpass Simple",    props: "cutoff=2000 Hz" },
    LowpassSimpleEffect:     { label: "Lowpass Simple",     props: "cutoff=2000 Hz" },
    ParamEqEffect:           { label: "Param EQ",           props: "centreFrequency=2000 Hz,gain=0 dB,bandwidth=1" },
    MixerBusFader:           { label: "Fader" },
    EffectChain:             { label: "Effect Chain" },
    PluginEffect:            { label: "Plugin" }
};

// Spatializer properties, shown in full on the Spatializer view.
// distanceRolloffType's labels are documented (project.distanceRolloffType);
// extentMode and panBlend were checked against the Studio UI:
// extentMode 0 = Auto, 1 = User (2 = Off, inferred); panBlend is stored inverted,
// 1 = 0% 2D pan mix (linear in between, inferred).
var BEB_SPATIAL_PROPS = "overrideRange=false,minimumDistance=1,maximumDistance=20,distanceRolloffType=0 #,extentMode=0 #,soundSize=0,minimumExtent=0";
var BEB_SPATIALISER_PROPS = "panBlend=1 raw,occlusionEnabled=true,dopplerMultiplier=100 %,userPanDirection=0,userPanExtent=360,userLFELevel=0 dB,LFEUpmixEnabled=false,stereoToSurroundUserPanMode=1 #,userStereoSeparation=60,userStereoAxis=0";
var BEB_ROLLOFF = ["Linear Squared", "Linear", "Inverse", "Inverse Tapered", "Custom"];
var BEB_EXTENT = ["Auto", "User", "Off"];
var BEB_PROP_LABEL = {
    overrideRange: "Distance override", minimumDistance: "Min distance", maximumDistance: "Max distance",
    distanceRolloffType: "Rolloff", extentMode: "Envelopment", soundSize: "Sound size",
    minimumExtent: "Min extent", panBlend: "2D pan mix", occlusionEnabled: "Occlusion",
    dopplerMultiplier: "Doppler", userPanDirection: "Pan direction", userPanExtent: "Pan extent",
    userLFELevel: "LFE level", LFEUpmixEnabled: "LFE upmix", stereoToSurroundUserPanMode: "Stereo→surround mode",
    userStereoSeparation: "Stereo separation", userStereoAxis: "Stereo axis",
    gain: "Gain", level: "Level", volume: "Volume", pitch: "Pitch"
};

// Event macros (on event.automatableProperties, except Pitch: see below).
// Stealing order confirmed in Studio (0 = Oldest, 2 = Virtualize), which rules
// out the other candidate orders.
var BEB_MACROS = [
    { prop: "maxVoices",       label: "Max instances", kind: "int",   min: 1, max: 65, hint: "65 = ∞",
      fmt: function (v) { return v >= 65 ? "∞" : "" + v; } },
    { prop: "voiceStealing",   label: "Stealing",      kind: "enum",  opts: ["Oldest", "Quietest", "Virtualize", "None", "Furthest"] },
    { prop: "priority",        label: "Priority",      kind: "enum",  opts: ["Lowest", "Low", "Medium", "High", "Highest"] },
    { prop: "triggerCooldown", label: "Cooldown",      kind: "float", min: 0, max: 60000, unit: "ms", hint: "ms, 0–60000" },
    { prop: "isPersistent",    label: "Persistent",    kind: "bool" },
    { prop: "dopplerEnabled",  label: "Doppler",       kind: "bool" },
    { prop: "dopplerScale",    label: "Doppler scale", kind: "float", min: 0, max: 500, unit: "%", hint: "%, needs Doppler on" },
    // Studio's drawer shows Pitch with the macros, but it isn't on the macro object:
    // it's the event's master bus pitch (MixerGroup.pitch, semitones). Unverified
    // that the drawer edits exactly this.
    { prop: "pitch",           label: "Pitch",         kind: "float", min: -24, max: 24, unit: "st", hint: "semitones", on: "master" },
    { prop: "minimumDistance", label: "Min distance",  kind: "float", min: 0, max: 10000, hint: "≤ max distance" },
    { prop: "maximumDistance", label: "Max distance",  kind: "float", min: 0, max: 10000, hint: "≥ min distance" }
];

var BEB_VIEWS = ["Patterns", "Differences", "Effects", "Spatializer", "Sheets", "Macros", "Tags & Notes", "Per event"];

// ── Small helpers ──────────────────────────────────────────────────────────
function bebGet(o, k) { try { return o[k]; } catch (e) { return undefined; } }

function bebList(v) {
    var out = [];
    try { if (v && typeof v.length === "number") { for (var i = 0; i < v.length; i++) { out.push(v[i]); } } } catch (e) {}
    return out;
}

function bebIs(o, t) { try { return !!o && o.isOfExactType(t); } catch (e) { return false; } }
function bebIsA(o, t) { try { return !!o && o.isOfType(t); } catch (e) { return false; } }

// Reads through the ManagedProperty first (the authoritative path), then
// the plain property.
function bebProp(o, name) {
    try { var mp = o.properties[name]; if (mp) { return mp.value; } } catch (e) {}
    return bebGet(o, name);
}

function bebEsc(s) {
    return ("" + s).split("&").join("&amp;").split("<").join("&lt;").split(">").join("&gt;");
}

function bebPlural(n, word, many) { return n + " " + (n === 1 ? word : (many || word + "s")); }

function bebNum(v) {
    if (typeof v !== "number") { return "" + v; }
    var r = Math.round(v * 100) / 100;
    return "" + r;
}

function bebTrunc(s, n) { s = "" + s; return s.length > n ? s.substring(0, n - 1) + "…" : s; }
function bebTruncStart(s, n) { s = "" + s; return s.length > n ? "…" + s.substring(s.length - n + 1) : s; }

function bebEventPath(ev) {
    var p = "";
    try { p = ev.getPath(); } catch (e) {}
    if (!p) { p = "" + (bebGet(ev, "name") || "<unnamed>"); }
    return p.indexOf("event:/") === 0 ? p.substring(7) : p;
}

function bebParamName(p) {
    if (!p) { return "?"; }
    if (bebIs(p, "Timeline")) { return "Timeline"; }
    var po = bebGet(p, "presetOwner");
    if (po && bebGet(po, "name")) { return "" + po.name; }
    var pr = bebGet(p, "preset");
    if (pr && bebGet(pr, "presetOwner")) { return "" + pr.presetOwner.name; }
    return "" + (bebGet(p, "name") || "?");
}

// "name=default unit" list → [{ name, def, unit, raw }]
var bebSpecCache = {};
function bebSpec(str) {
    if (!str) { return []; }
    if (bebSpecCache[str]) { return bebSpecCache[str]; }
    var out = [];
    var parts = str.split(",");
    for (var i = 0; i < parts.length; i++) {
        var eq = parts[i].indexOf("=");
        var name = parts[i].substring(0, eq);
        var rest = parts[i].substring(eq + 1).split(" ");
        var d = rest[0];
        var def = d === "true" ? true : d === "false" ? false : Number(d);
        var unit = rest.length > 1 ? rest[1] : "";
        out.push({ name: name, def: def, unit: unit === "#" || unit === "raw" ? "" : unit, raw: unit === "#" });
    }
    bebSpecCache[str] = out;
    return out;
}

function bebSame(a, b) {
    if (typeof a === "number" && typeof b === "number") { return Math.abs(a - b) < 1e-4; }
    return a === b;
}

function bebFmt(spec, v) {
    if (v === undefined || v === null) { return "—"; }
    if (typeof v === "boolean") { return v ? "On" : "Off"; }
    if (spec.name === "distanceRolloffType" && BEB_ROLLOFF[v] !== undefined) { return BEB_ROLLOFF[v]; }
    if (spec.name === "extentMode" && BEB_EXTENT[v] !== undefined) { return BEB_EXTENT[v]; }
    if (spec.name === "panBlend" && typeof v === "number") { return bebNum(Math.round((1 - v) * 1000) / 10) + "%"; }
    if (spec.raw) { return "#" + v; }
    return bebNum(v) + (spec.unit ? (spec.unit === "%" || spec.unit === ":1" ? "" : " ") + spec.unit : "");
}

// ── Effects ────────────────────────────────────────────────────────────────
function bebFxLabel(entity, real) {
    if (entity === "PluginEffect") {
        var id = "";
        try { id = "" + real.plugin.identifier; } catch (e) {}
        return id ? id : "Plugin";
    }
    return BEB_FX[entity] ? BEB_FX[entity].label : entity;
}

// Who drives which property of obj: [{ prop, kind: "auto"|"mod", by }]
function bebDriven(obj) {
    var out = [];
    var autos = bebList(bebGet(obj, "automators"));
    for (var a = 0; a < autos.length; a++) {
        var names = [];
        var curves = bebList(bebGet(autos[a], "automationCurves"));
        for (var c = 0; c < curves.length; c++) { names.push(bebParamName(bebGet(curves[c], "parameter"))); }
        out.push({ prop: "" + bebGet(autos[a], "nameOfPropertyBeingAutomated"), kind: "auto", by: names.join(", ") || "?" });
    }
    var mods = bebList(bebGet(obj, "modulators"));
    for (var m = 0; m < mods.length; m++) {
        var ent = "" + bebGet(mods[m], "entity");
        out.push({ prop: "" + bebGet(mods[m], "nameOfPropertyBeingModulated"), kind: "mod",
                   by: ent.replace("Modulator", "") });
    }
    return out;
}

// "highGain" → "High gain", "frequencyA" → "Frequency A", unless BEB_PROP_LABEL names it.
function bebPropLabel(name) {
    if (BEB_PROP_LABEL[name]) { return BEB_PROP_LABEL[name]; }
    var out = "";
    for (var i = 0; i < name.length; i++) {
        var c = name.charAt(i);
        out += (i > 0 && c >= "A" && c <= "Z") ? " " + c.toLowerCase() : c;
    }
    out = out.replace(/ ([a-e])$/, function (m, l) { return " " + l.toUpperCase(); });   // band letter
    return out.charAt(0).toUpperCase() + out.substring(1);
}

// "auto Min extent ← Distance" (verb: "auto"/"automated").
function bebDrivenText(dr, verbose) {
    var verb = dr.kind === "auto" ? (verbose ? "automated " : "auto ") : (verbose ? "modulated " : "mod ");
    return verb + bebPropLabel(dr.prop) + " ← " + dr.by;
}

// Long names with underscores wrap at the underscores, not mid-word.
function bebBreakable(html) { return html.split("_").join("_&#8203;"); }

function bebFxSpecs(entity) {
    var d = BEB_FX[entity];
    if (d && d.spatial) {
        var s = bebSpec(BEB_SPATIAL_PROPS);
        return entity === "SpatialiserEffect" ? s.concat(bebSpec(BEB_SPATIALISER_PROPS)) : s;
    }
    if (d && d.bands) {
        var b = [];
        var L = ["A", "B", "C", "D", "E"];
        var dflt = { A: [1, 660], B: [0, 40], C: [0, 11000], D: [0, 160], E: [0, 2700] };
        for (var i = 0; i < L.length; i++) {
            b.push({ name: "filterType" + L[i], def: dflt[L[i]][0], unit: "", raw: true });
            b.push({ name: "frequency" + L[i], def: dflt[L[i]][1], unit: "Hz" });
            b.push({ name: "quality" + L[i], def: 0.707, unit: "" });
            b.push({ name: "gain" + L[i], def: 0, unit: "dB" });
        }
        return b;
    }
    return bebSpec(d ? d.props : "");
}

function bebScanEffect(fx, trackKind, index) {
    var entity = "" + bebGet(fx, "entity");
    var info = { entity: entity, real: entity, shared: false, presetName: "", bypass: false,
                 values: [], driven: [], track: trackKind, index: index };
    var target = fx;
    if (entity === "ProxyEffect") {
        var pr = bebGet(fx, "preset");
        var real = bebGet(pr, "effect");
        info.shared = true;
        info.presetName = "" + (bebGet(pr, "name") || "?");
        info.real = "" + (bebGet(real, "entity") || "?");
        info.uses = bebList(bebGet(pr, "proxies")).length;
        target = real || fx;
        info.driven = bebDriven(fx);       // automation on this instance
    }
    info.label = bebFxLabel(info.real, target);
    info.isFader = info.real === "MixerBusFader";
    info.spatial = !!(BEB_FX[info.real] && BEB_FX[info.real].spatial) ||
                   (info.real === "PluginEffect" && info.label.toLowerCase().indexOf("source") >= 0);
    info.bypass = bebBypassed(fx);
    info.sharedBypass = info.shared && !!bebProp(target, "bypass");
    var specs = bebFxSpecs(info.real);
    for (var i = 0; i < specs.length; i++) {
        var v = bebProp(target, specs[i].name);
        if (v === undefined) { continue; }
        info.values.push({ spec: specs[i], v: v, changed: !bebSame(v, specs[i].def) });
    }
    // A shared effect chain: scan what's inside it. Its spatializer counts
    // as the event's spatializer.
    if (info.shared && info.real === "EffectChain") {
        info.inner = [];
        bebCollectChain(target, trackKind, info.inner);
        info.contains = [];
        for (var q = 0; q < info.inner.length; q++) {
            var inn = info.inner[q];
            inn.shared = true;
            inn.inChain = true;
            inn.presetName = info.presetName + " › " + inn.label;
            info.contains.push(inn.label);
        }
    }
    if (target !== fx) { info.driven = info.driven.concat(bebDriven(target)); }
    else { info.driven = bebDriven(fx); }
    // Multiband EQ: hide bands that are switched off.
    if (BEB_FX[info.real] && BEB_FX[info.real].bands) {
        var keep = [], off = {};
        for (var j = 0; j < info.values.length; j++) {
            var n = info.values[j].spec.name;
            if (n.indexOf("filterType") === 0 && info.values[j].v === 0) { off[n.charAt(n.length - 1)] = true; }
        }
        for (var k = 0; k < info.values.length; k++) {
            var nm = info.values[k].spec.name;
            if (!off[nm.charAt(nm.length - 1)]) { keep.push(info.values[k]); }
        }
        info.values = keep;
    }
    return info;
}

// Bypass lives on the event's own copy (ProxyEffect, from MixerEffect) AND on the
// shared effect itself. Studio's bypass button on a shared effect appears to
// switch the shared one, which affects every event using it,
// so either counts as bypassed.
function bebBypassed(fx) {
    if (bebProp(fx, "bypass")) { return true; }
    if (bebIs(fx, "ProxyEffect")) { return !!bebProp(bebGet(bebGet(fx, "preset"), "effect"), "bypass"); }
    return false;
}

// ── Naming standard ────────────────────────────────────────────────────────
// Dropdowns can't be coloured, so plain text says what each thing is in words.
//   List row : "[Shared]  s_map_emitter_medium — Spatializer", "[Unique]  Gain",
//              "[Chain]  g_plots_special › Gain" (an effect inside a shared chain)
//   Inline   : inside a chain ("Gain › [Shared] send_rev › Fader") shared effects
//              keep their tag and unique ones are just their type.
//   Usage    : "on 7 of 7 events", "(5 copies)" when an event has it twice.
// HTML views carry the same meaning in colour: blue chip = shared, amber chip =
// unique, and event paths always in BEB_C.event. Guide: "How things are labelled".
function bebFxRowText(kind, name, type) {
    if (kind === "shared") { return "[Shared]  " + name + (type ? " — " + type : ""); }
    return (kind === "chain" ? "[Chain]  " : "[Unique]  ") + name;
}
function bebFxKind(fx) { return fx.inChain ? "chain" : fx.shared ? "shared" : "unique"; }
function bebFxRow(fx) { var k = bebFxKind(fx); return bebFxRowText(k, k === "unique" ? fx.label : fx.presetName, k === "shared" ? fx.label : ""); }
function bebFxInline(fx) { return fx.inChain ? "[Chain] " + fx.presetName : fx.shared ? "[Shared] " + fx.presetName : fx.label; }
function bebUsageText(n, total, copies) {
    return "on " + n + " of " + bebPlural(total, "event") + (copies > n ? "  (" + copies + " copies)" : "");
}
function bebEventHtml(path) { return "<span style=\"color:" + BEB_C.event + ";\">" + bebEsc(path) + "</span>"; }

// Chip text: colour says shared/unique, so the chip holds just the name.
function bebFxChipText(fx) { return fx.shared ? fx.presetName : fx.label; }

// ── Event scan ─────────────────────────────────────────────────────────────
function bebTracksOf(ev) {
    var out = [];
    var mt = bebGet(ev, "masterTrack");
    if (mt) { out.push({ kind: "Master", name: "Master", bus: bebGet(mt, "mixerGroup") }); }
    var gts = bebList(bebGet(ev, "groupTracks"));
    for (var g = 0; g < gts.length; g++) {
        var b = bebGet(gts[g], "mixerGroup");
        out.push({ kind: "Group", name: "" + (bebGet(b, "name") || "Group " + (g + 1)), bus: b });
    }
    var rts = bebList(bebGet(ev, "returnTracks"));
    for (var r = 0; r < rts.length; r++) {
        var rb = bebGet(rts[r], "mixerReturn");
        out.push({ kind: "Return", name: "" + (bebGet(rb, "name") || "Return " + (r + 1)), bus: rb });
    }
    return out;
}

function bebCollectChain(chain, kind, out) {
    var fxs = bebList(bebGet(chain, "effects"));
    for (var i = 0; i < fxs.length; i++) {
        if (bebIs(fxs[i], "EffectChain")) {
            out.push(bebScanEffect(fxs[i], kind, out.length));
            bebCollectChain(fxs[i], kind, out);           // nested chain
        } else {
            out.push(bebScanEffect(fxs[i], kind, out.length));
        }
    }
}

function bebScanEvent(ev) {
    var info = { ev: ev, id: "" + ev.id, path: bebEventPath(ev), note: "", tags: [], tracks: [],
                 sheets: [], actionSheets: 0, actionSounds: 0, timelineModules: 0, markers: 0,
                 macros: {}, macroDriven: [], spatial: [], effects: [], otherFx: [], master: null,
                 sig: "(unreadable)", drive: "?", error: null };
    try {
        info.name = "" + (bebGet(ev, "name") || "");
        info.note = "" + (bebProp(ev, "note") || "");
        var tags = bebList(bebGet(ev, "tags"));
        for (var t = 0; t < tags.length; t++) { info.tags.push("" + bebGet(tags[t], "name")); }

        // Sheets: where the instruments live decides what drives the event.
        var tl = bebGet(ev, "timeline");
        info.timelineModules = bebList(bebGet(tl, "modules")).length;
        info.markers = bebList(bebGet(tl, "markers")).length;
        var params = bebList(bebGet(ev, "parameters"));
        for (var p = 0; p < params.length; p++) {
            var pp = params[p];
            if (bebIs(pp, "ParameterProxy")) {
                var gp = bebGet(pp, "preset");
                var labels = bebGet(gp, "enumerationLabels");
                var kind = bebGet(gp, "isReadOnly") ? "built-in"
                         : (labels && labels.length) ? "labeled"
                         : (bebGet(gp, "parameterType") === 1 ? "discrete" : "continuous");
                info.sheets.push({ name: bebParamName(gp), global: !!bebGet(gp, "isGlobal"), kind: kind,
                                   modules: bebList(bebGet(pp, "modules")).length });
            } else if (bebIs(pp, "ActionSheet")) {
                info.actionSheets++;
                info.actionSounds += bebList(bebGet(bebGet(pp, "modules"), "sounds")).length;
            }
        }
        var sheetModules = 0;
        for (var s = 0; s < info.sheets.length; s++) { sheetModules += info.sheets[s].modules; }
        var drivers = [];
        if (info.timelineModules) { drivers.push("Timeline"); }
        if (sheetModules) { drivers.push("Parameter sheet"); }
        if (info.actionSheets) { drivers.push("Action sheet"); }
        info.drive = drivers.length ? drivers.join(" + ") : "Empty";

        // Tracks and effects
        var tracks = bebTracksOf(ev);
        for (var k = 0; k < tracks.length; k++) {
            var tr = tracks[k];
            var fx = [];
            bebCollectChain(bebGet(tr.bus, "effectChain"), tr.kind, fx);
            var volDriven = bebDriven(tr.bus);
            var t2 = { kind: tr.kind, name: tr.name, volume: bebProp(tr.bus, "volume"), driven: volDriven, effects: fx };
            info.tracks.push(t2);
            for (var f = 0; f < fx.length; f++) {
                fx[f].trackName = tr.name;
                if (!fx[f].isFader) { info.effects.push(fx[f]); }
                if (fx[f].spatial) { info.spatial.push(fx[f]); }
                var inner = fx[f].inner || [];
                for (var fi = 0; fi < inner.length; fi++) {
                    inner[fi].trackName = tr.name;
                    if (!inner[fi].isFader) { info.effects.push(inner[fi]); }
                    if (inner[fi].spatial) { info.spatial.push(inner[fi]); }
                }
            }
        }
        info.master = info.tracks.length && info.tracks[0].kind === "Master" ? info.tracks[0] : null;
        info.sig = bebChainSig(info.master);
        var others = [];
        for (var o = 1; o < info.tracks.length; o++) {
            var labelsO = [];
            for (var q = 0; q < info.tracks[o].effects.length; q++) {
                if (!info.tracks[o].effects[q].isFader) { labelsO.push(bebFxInline(info.tracks[o].effects[q])); }
            }
            if (labelsO.length) { others.push(info.tracks[o].kind + " “" + info.tracks[o].name + "”: " + labelsO.join(" › ")); }
        }
        info.otherFx = others;

        // Macros
        var ap = bebGet(ev, "automatableProperties");
        for (var m = 0; m < BEB_MACROS.length; m++) { info.macros[BEB_MACROS[m].prop] = bebProp(bebMacroObj(ev, BEB_MACROS[m]), BEB_MACROS[m].prop); }
        info.macroDriven = bebDriven(ap);
    } catch (e) {
        info.error = "" + e;
    }
    return info;
}

function bebChainSig(track) {
    if (!track) { return "(no master track)"; }
    var parts = [];
    for (var i = 0; i < track.effects.length; i++) {
        var fx = track.effects[i];
        parts.push(fx.isFader ? "Fader" : bebFxInline(fx) + (fx.bypass ? " (bypassed)" : ""));
    }
    return parts.join(" › ") || "(empty)";
}

// ── Selection ──────────────────────────────────────────────────────────────
// browserSelection() reads the active browser tab of the last active window.
// Selected EventFolders are walked when "Include events inside folders" is
// ticked; the walk recurses on EventFolder only (Event derives from Folder).
function bebRawSelection() {
    var out = [];
    var sel = [];
    try { sel = bebList(studio.window.browserSelection()); } catch (e) { sel = []; }
    for (var i = 0; i < sel.length; i++) {
        if (bebIs(sel[i], "Event") || bebIs(sel[i], "EventFolder")) { out.push(sel[i]); }
    }
    return out;
}

function bebResolve(items, recursive) {
    var res = { events: [], direct: 0, folders: 0, folderEvents: 0, folderNames: [] };
    var seen = {};
    function push(ev, fromFolder) {
        var k = "" + ev.id;
        if (seen[k]) { return; }
        seen[k] = true;
        res.events.push(ev);
        if (fromFolder) { res.folderEvents++; } else { res.direct++; }
    }
    function walk(folder, depth) {
        if (depth > 32) { return; }
        var its = bebList(bebGet(folder, "items"));
        for (var i = 0; i < its.length; i++) {
            if (bebIs(its[i], "Event")) { push(its[i], true); }
            else if (bebIs(its[i], "EventFolder")) { walk(its[i], depth + 1); }
        }
    }
    for (var j = 0; j < items.length; j++) {
        if (bebIs(items[j], "Event")) { push(items[j], false); }
        else {
            res.folders++;
            res.folderNames.push("" + (bebGet(items[j], "name") || "?"));
            if (recursive) { walk(items[j], 0); }
        }
    }
    return res;
}

// ── Grouping ───────────────────────────────────────────────────────────────
function bebGroups(infos) {
    var by = {}, order = [];
    for (var i = 0; i < infos.length; i++) {
        var s = infos[i].sig;
        if (!by[s]) { by[s] = { sig: s, events: [] }; order.push(by[s]); }
        by[s].events.push(infos[i]);
    }
    order.sort(function (a, b) { return b.events.length - a.events.length || (a.sig < b.sig ? -1 : 1); });
    var letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for (var j = 0; j < order.length; j++) {
        order[j].letter = j < 26 ? letters.charAt(j) : "Z" + (j - 25);
    }
    return order;
}

// Value distribution: [{ v, n }] sorted by count.
function bebDist(values) {
    var by = {}, order = [];
    for (var i = 0; i < values.length; i++) {
        var k = "#" + values[i];
        if (!by[k]) { by[k] = { v: values[i], n: 0 }; order.push(by[k]); }
        by[k].n++;
    }
    order.sort(function (a, b) { return b.n - a.n; });
    return order;
}

// ── HTML rendering (Qt rich text) ──────────────────────────────────────────
var BEB_C = { text: "#dddddd", grey: "#9a9a9a", accent: "#ffd84d", card: "#323232", warn: "#ff9a6a", event: "#7fd4c1",
              unique: "#6a4b2a", uniqueText: "#ffd7a8", shared: "#2a4f6a", sharedText: "#bfe3ff",
              fader: "#444444", faderText: "#cccccc", auto: "#5a2a5a", autoText: "#f5c6f5", bar: "#5b8def" };

// Edit previews: what's written (green), what it replaces (red, struck through).
var BEB_ADD = "<span style=\"background-color:#24452a; color:#b8f0c0;\">";
var BEB_DEL = "<span style=\"background-color:#4a2626; color:#ffb3b3; text-decoration:line-through;\">";

function bebChip(text, bg, fg) {
    return "<span style=\"background-color:" + bg + "; color:" + fg + ";\">&nbsp;" + bebEsc(text) + "&nbsp;</span>";
}

function bebFxChip(fx) {
    if (fx.isFader) { return bebChip("Fader", BEB_C.fader, BEB_C.faderText); }
    var t = bebFxChipText(fx) + (fx.bypass ? " ⊘" : "");
    var c = fx.shared ? bebChip(t, BEB_C.shared, BEB_C.sharedText) : bebChip(t, BEB_C.unique, BEB_C.uniqueText);
    if (fx.driven.length) { c += bebChip("auto", BEB_C.auto, BEB_C.autoText); }
    return c;
}

function bebGrey(s) { return "<span style=\"color:" + BEB_C.grey + ";\">" + s + "</span>"; }
function bebWarn(s) { return "<span style=\"color:" + BEB_C.warn + ";\">" + s + "</span>"; }

function bebCard(title, sub, rowsHtml, titleColor) {
    return "<table width=\"100%\" cellpadding=\"7\" cellspacing=\"0\" bgcolor=\"" + BEB_C.card + "\"><tr><td style=\"color:" + BEB_C.text + ";\">" +
           "<span style=\"color:" + (titleColor || BEB_C.accent) + "; font-weight:bold;\">" + bebEsc(title) + "</span>" +
           (sub ? "&nbsp;&nbsp;" + bebGrey(sub) : "") + "<br/>" + rowsHtml + "</td></tr></table><br/>";
}

// Two-column key/value rows.
function bebRows(rows) {
    var h = "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"2\">";
    for (var i = 0; i < rows.length; i++) {
        h += "<tr><td width=\"170\" valign=\"top\" style=\"color:" + BEB_C.text + ";\">" + rows[i][0] + "</td><td valign=\"top\" style=\"color:" + BEB_C.text + ";\">" + rows[i][1] + "</td></tr>";
    }
    return h + "</table>";
}

// "a ×3 · b ×1  mixed". noMixed: for lists where several values are normal
// (tags, track kinds).
function bebDistHtml(dist, total, fmt, noMixed) {
    var parts = [];
    for (var i = 0; i < dist.length && i < 8; i++) {
        parts.push(bebEsc(fmt ? fmt(dist[i].v) : dist[i].v) + " " + bebGrey("×" + dist[i].n));
    }
    if (dist.length > 8) { parts.push(bebGrey("+" + (dist.length - 8) + " more")); }
    var h = parts.join(" · ");
    if (dist.length > 1 && !noMixed) { h += "&nbsp;&nbsp;" + bebWarn("mixed"); }
    return h || bebGrey("—");
}

function bebBar(pct) {
    pct = Math.max(2, Math.min(100, Math.round(pct)));
    return "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr>" +
           "<td width=\"" + pct + "%\" bgcolor=\"" + BEB_C.bar + "\" style=\"font-size:4px;\">&nbsp;</td>" +
           (pct < 100 ? "<td width=\"" + (100 - pct) + "%\" style=\"font-size:4px;\">&nbsp;</td>" : "") + "</tr></table>";
}

function bebWrap(body) {
    return "<html><body style=\"color:" + BEB_C.text + "; font-size:12px;\">" + body + "</body></html>";
}

// ── Views ──────────────────────────────────────────────────────────────────
function bebViewPatterns(infos, groups) {
    if (!infos.length) { return bebGrey("Nothing to show."); }
    var h = "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"3\">";
    var max = groups.length ? groups[0].events.length : 1;
    for (var i = 0; i < groups.length; i++) {
        var g = groups[i];
        var chips = [];
        var fxs = g.events[0].master ? g.events[0].master.effects : [];
        for (var j = 0; j < fxs.length; j++) { chips.push(bebFxChip(fxs[j])); }
        h += "<tr><td width=\"36\" align=\"right\" style=\"color:" + BEB_C.accent + "; font-weight:bold;\">" + g.events.length + "</td>" +
             "<td width=\"18\" style=\"color:" + BEB_C.text + "; font-weight:bold;\">" + g.letter + "</td>" +
             "<td style=\"color:" + BEB_C.text + ";\">" + (chips.join(" ") || bebGrey("(empty chain)")) + "</td>" +
             "<td width=\"120\">" + bebBar(100 * g.events.length / max) + "</td></tr>";
    }
    h += "</table>";
    var out = bebCard("Master track effect chains", bebPlural(groups.length, "pattern") + " across " + bebPlural(infos.length, "event"), h);

    // Other tracks
    var other = [];
    for (var k = 0; k < infos.length; k++) { other = other.concat(infos[k].otherFx); }
    var od = bebDist(other);
    var withOther = 0;
    for (var m = 0; m < infos.length; m++) { if (infos[m].otherFx.length) { withOther++; } }
    out += bebCard("Group & return track effects", withOther + " of " + infos.length + " events have any",
                   od.length ? bebRows(bebListRows(od)) : bebGrey("None."));

    out += bebSpatialSummary(infos, true);
    out += bebUniqueSummary(infos);
    out += bebSheetsSummary(infos);
    out += bebTagsSummary(infos);
    return out;
}

function bebListRows(dist) {
    var rows = [];
    for (var i = 0; i < dist.length && i < 30; i++) {
        rows.push(["<span style=\"color:" + BEB_C.accent + ";\">×" + dist[i].n + "</span>", bebEsc(dist[i].v)]);
    }
    return rows;
}

function bebSpatialSummary(infos, compact) {
    var withSp = [], types = [], shared = 0;
    for (var i = 0; i < infos.length; i++) {
        if (!infos[i].spatial.length) { types.push("None (2D)"); continue; }
        var sp = infos[i].spatial[0];
        withSp.push(sp);
        types.push(sp.label + (infos[i].spatial.length > 1 ? " (+" + (infos[i].spatial.length - 1) + " more)" : ""));
        if (sp.shared) { shared++; }
    }
    var rows = [["Type", bebDistHtml(bebDist(types), infos.length)]];
    // Effective range: the effect's own min/max when it overrides, else the event macro.
    var eff = [];
    for (var x = 0; x < infos.length; x++) { if (infos[x].spatial.length) { eff.push(bebEffectiveRange(infos[x])); } }
    if (eff.length) { rows.push(["Effective min / max", bebDistHtml(bebDist(eff), eff.length)]); }
    var specs = bebSpec(BEB_SPATIAL_PROPS).concat(compact ? [] : bebSpec(BEB_SPATIALISER_PROPS));
    if (compact) { specs = bebSpec("overrideRange=false,distanceRolloffType=0 #,panBlend=1 raw,extentMode=0 #"); }
    for (var s = 0; s < specs.length; s++) {
        var vals = [];
        for (var j = 0; j < withSp.length; j++) {
            for (var k = 0; k < withSp[j].values.length; k++) {
                if (withSp[j].values[k].spec.name === specs[s].name) { vals.push(withSp[j].values[k].v); }
            }
        }
        if (!vals.length) { continue; }
        var spec = specs[s];
        rows.push([bebPropLabel(spec.name), bebDistHtml(bebDist(vals), vals.length, function (v) { return bebFmt(spec, v); })]);
    }
    var drivenRows = [];
    var dv = [];
    for (var d = 0; d < withSp.length; d++) {
        for (var e = 0; e < withSp[d].driven.length; e++) {
            var dr = withSp[d].driven[e];
            dv.push(bebDrivenText(dr, true));
        }
    }
    if (dv.length) { rows.push(["Automation", bebDistHtml(bebDist(dv), dv.length)]); }
    return bebCard("Spatializer", withSp.length + " with a spatializer · " + shared + " via a shared effect", bebRows(rows.concat(drivenRows)));
}

// Every non-spatial, non-fader effect type: count, shared/unique, changed values, automation.
function bebUniqueSummary(infos, full) {
    var by = {}, order = [];
    for (var i = 0; i < infos.length; i++) {
        for (var j = 0; j < infos[i].effects.length; j++) {
            var fx = infos[i].effects[j];
            if (fx.spatial && !full) { continue; }
            var key = bebFxRow(fx);
            if (!by[key]) { by[key] = { key: key, fx: [], events: {} }; order.push(by[key]); }
            by[key].fx.push(fx);
            by[key].events[infos[i].id] = true;
        }
    }
    order.sort(function (a, b) { return b.fx.length - a.fx.length; });
    if (!order.length) { return bebCard("Other effects", "", bebGrey("No inserts besides the fader" + (full ? "." : " and spatializers."))); }
    var rows = [];
    for (var k = 0; k < order.length; k++) {
        var o = order[k];
        var nEv = 0;
        for (var x in o.events) { if (o.events.hasOwnProperty(x)) { nEv++; } }
        var tracks = [], bypassed = 0, changed = {}, changedOrder = [], drv = [];
        for (var f = 0; f < o.fx.length; f++) {
            var e = o.fx[f];
            tracks.push(e.track);
            if (e.bypass) { bypassed++; }
            for (var v = 0; v < e.values.length; v++) {
                var val = e.values[v];
                if (!val.changed && !full) { continue; }
                var nm = val.spec.name;
                if (!changed[nm]) { changed[nm] = { spec: val.spec, vals: [] }; changedOrder.push(nm); }
                changed[nm].vals.push(val.v);
            }
            for (var dd = 0; dd < e.driven.length; dd++) {
                drv.push(bebDrivenText(e.driven[dd]));
            }
        }
        var lines = [];
        lines.push(bebGrey("on ") + bebDistHtml(bebDist(tracks), tracks.length, null, true) +
                   (bypassed ? bebGrey(" · ") + bebWarn(bypassed + " bypassed") : ""));
        for (var c = 0; c < changedOrder.length && c < 12; c++) {
            var ch = changed[changedOrder[c]];
            var unchanged = o.fx.length - ch.vals.length;
            lines.push(bebEsc(bebPropLabel(ch.spec.name)) + ": " + bebDistHtml(bebDist(ch.vals), ch.vals.length, (function (sp) {
                return function (vv) { return bebFmt(sp, vv); };
            })(ch.spec)) + (unchanged > 0 && !full ? bebGrey(" · default ×" + unchanged) : ""));
        }
        if (drv.length) { lines.push(bebChip("automation", BEB_C.auto, BEB_C.autoText) + " " + bebDistHtml(bebDist(drv), drv.length)); }
        if (o.fx[0].contains) { lines.push(bebGrey("contains: ") + bebEsc(o.fx[0].contains.join(" › ") || "nothing")); }
        rows.push([bebBreakable(bebEsc(o.key)) + " " + bebGrey("(" + nEv + ")"), lines.join("<br/>")]);
    }
    return bebCard(full ? "All effects" : "Other effects — values & automation",
                   full ? "every insert on every track · values listed in full" : "values that differ from the default",
                   bebRows(rows));
}

function bebSheetsSummary(infos, full) {
    var drive = [], sheets = [], groupCounts = [], returnCounts = [];
    var action = 0;
    for (var i = 0; i < infos.length; i++) {
        drive.push(infos[i].drive);
        for (var s = 0; s < infos[i].sheets.length; s++) {
            var sh = infos[i].sheets[s];
            sheets.push(sh.name + " (" + (sh.global ? "global, " : "") + sh.kind + (sh.modules ? "" : ", no instruments") + ")");
        }
        var g = 0, r = 0;
        for (var t = 0; t < infos[i].tracks.length; t++) {
            if (infos[i].tracks[t].kind === "Group") { g++; }
            if (infos[i].tracks[t].kind === "Return") { r++; }
        }
        groupCounts.push(g);
        returnCounts.push(r);
        if (infos[i].actionSheets) { action++; }
    }
    var rows = [
        ["Driven by", bebDistHtml(bebDist(drive), infos.length)],
        ["Parameter sheets", sheets.length ? bebDistHtml(bebDist(sheets), sheets.length) : bebGrey("none")],
        ["Group tracks", bebDistHtml(bebDist(groupCounts), infos.length)],
        ["Return tracks", bebDistHtml(bebDist(returnCounts), infos.length)]
    ];
    if (action) { rows.push(["Action sheets", action + " event" + (action === 1 ? "" : "s")]); }
    if (full) {
        var lists = {};
        var order = [];
        for (var e = 0; e < infos.length; e++) {
            var k = infos[e].drive;
            if (!lists[k]) { lists[k] = []; order.push(k); }
            lists[k].push(infos[e].path);
        }
        for (var o = 0; o < order.length; o++) {
            var paths = lists[order[o]];
            rows.push([bebEsc(order[o]) + " " + bebGrey("(" + paths.length + ")"),
                       bebEventHtml(paths.slice(0, 40).join("\n")).split("\n").join("<br/>") +
                       (paths.length > 40 ? "<br/>" + bebGrey("… " + (paths.length - 40) + " more") : "")]);
        }
    }
    return bebCard("Structure & sheets", "", bebRows(rows));
}

function bebMacroFmt(def, v) {
    if (v === undefined || v === null) { return "—"; }
    if (def.fmt) { return def.fmt(v); }
    if (def.opts) { return (def.opts[v] !== undefined ? def.opts[v] : "?") + " [" + v + "]"; }
    if (typeof v === "boolean") { return v ? "On" : "Off"; }
    return bebNum(v) + (def.unit ? " " + def.unit : "");
}

function bebMacrosView(infos) {
    var rows = [];
    for (var m = 0; m < BEB_MACROS.length; m++) {
        var def = BEB_MACROS[m];
        var vals = [];
        for (var i = 0; i < infos.length; i++) { vals.push(infos[i].macros[def.prop]); }
        rows.push([def.label, bebDistHtml(bebDist(vals), vals.length, function (d) { return function (v) { return bebMacroFmt(d, v); }; }(def))]);
    }
    var dv = [];
    for (var j = 0; j < infos.length; j++) {
        for (var k = 0; k < infos[j].macroDriven.length; k++) {
            var d2 = infos[j].macroDriven[k];
            dv.push(bebDrivenText(d2, true));
        }
    }
    if (dv.length) { rows.push(["Automation", bebDistHtml(bebDist(dv), dv.length)]); }
    return bebCard("Event macros", "", bebRows(rows));
}

function bebTagsSummary(infos, full) {
    var tags = [], noted = [];
    for (var i = 0; i < infos.length; i++) {
        tags = tags.concat(infos[i].tags);
        if (infos[i].note) { noted.push(infos[i]); }
    }
    var untagged = 0;
    for (var u = 0; u < infos.length; u++) { if (!infos[u].tags.length) { untagged++; } }
    var rows = [
        ["Tags", tags.length ? bebDistHtml(bebDist(tags), tags.length, null, true) : bebGrey("none")],
        ["Untagged", "" + untagged],
        ["Notes", noted.length + " of " + infos.length + " events have a note"]
    ];
    if (full) {
        for (var n = 0; n < noted.length && n < 120; n++) {
            rows.push([bebEsc(bebTruncStart(noted[n].path, 34)), bebEsc(noted[n].note).split("\n").join("<br/>")]);
        }
        if (noted.length > 120) { rows.push(["", bebGrey("… " + (noted.length - 120) + " more")]); }
    }
    return bebCard("Tags & notes", "", bebRows(rows));
}

function bebPerEventView(infos) {
    var h = "";
    var n = Math.min(infos.length, BEB_PER_EVENT_CAP);
    for (var i = 0; i < n; i++) {
        var e = infos[i];
        var lines = [];
        if (e.error) { lines.push(bebWarn("Couldn't read everything: " + bebEsc(e.error))); }
        for (var t = 0; t < e.tracks.length; t++) {
            var tr = e.tracks[t];
            var chips = [];
            for (var f = 0; f < tr.effects.length; f++) { chips.push(bebFxChip(tr.effects[f])); }
            var vol = typeof tr.volume === "number" && Math.abs(tr.volume) > 1e-4 ? " " + bebGrey(bebNum(tr.volume) + " dB") : "";
            lines.push(bebGrey(bebEsc(tr.kind === "Master" ? "Master" : tr.kind + " “" + tr.name + "”") + ":") + " " + chips.join(" ") + vol);
        }
        var sh = [];
        for (var s = 0; s < e.sheets.length; s++) { sh.push(e.sheets[s].name); }
        lines.push(bebGrey("Driven by: ") + bebEsc(e.drive) + (sh.length ? bebGrey(" · sheets: ") + bebEsc(sh.join(", ")) : ""));
        if (e.tags.length) { lines.push(bebGrey("Tags: ") + bebEsc(e.tags.join(", "))); }
        if (e.note) { lines.push(bebGrey("Note: ") + bebEsc(bebTrunc(e.note.split("\n").join(" "), 140))); }
        h += bebCard(e.path, "", lines.join("<br/>"), BEB_C.event);
    }
    if (infos.length > n) { h += bebGrey("… " + (infos.length - n) + " more events not shown. Narrow “Apply to”, or use Export TSV."); }
    return h || bebGrey("Nothing to show.");
}

// ── Differences view ───────────────────────────────────────────────────────
// Each event is boiled down to "facets": one plain value per aspect (master
// chain, spatializer, each macro, tags…). A facet every event agrees on is
// common ground; one that varies is a difference. When most events share a
// value, events holding a rare one (a quarter of them or fewer) are outliers,
// and the event that is off the majority most often differs the most.
function bebMacroPlain(def, v) {
    if (def.opts && typeof v === "number") { return def.opts[v] !== undefined ? def.opts[v] : "#" + v; }
    return bebMacroFmt(def, v);
}

function bebFacets(info) {
    var out = [];
    function add(key, label, value) { out.push({ key: key, label: label, value: "" + value }); }
    add("chain", "Master chain", info.sig);
    add("other", "Group & return effects", info.otherFx.length ? info.otherFx.join(" | ") : "none");
    var groups = 0, returns = 0;
    for (var t = 0; t < info.tracks.length; t++) {
        if (info.tracks[t].kind === "Group") { groups++; }
        if (info.tracks[t].kind === "Return") { returns++; }
    }
    add("tracks", "Group / return tracks", !groups && !returns ? "none" : !returns ? bebPlural(groups, "group track")
        : !groups ? bebPlural(returns, "return track") : groups + " group + " + bebPlural(returns, "return track"));
    add("drive", "Driven by", info.drive);
    var sh = [];
    for (var s = 0; s < info.sheets.length; s++) { sh.push(info.sheets[s].name); }
    sh.sort();
    add("sheets", "Parameter sheets", sh.length ? sh.join(", ") : "none");
    var sp = info.spatial[0];
    add("spatial", "Spatializer", sp ? bebFxRow(sp) : "none (2D)");
    if (sp) {
        add("range", "Min / max distance", bebEffectiveRange(info));
        for (var v = 0; v < sp.values.length; v++) {
            var spec = sp.values[v].spec;
            if (spec.name === "minimumDistance" || spec.name === "maximumDistance" || spec.name === "overrideRange") { continue; }
            add("sp:" + spec.name, "Spatializer · " + bebPropLabel(spec.name), bebFmt(spec, sp.values[v].v));
        }
    }
    for (var m = 0; m < BEB_MACROS.length; m++) {
        var def = BEB_MACROS[m];
        if (def.prop === "minimumDistance" || def.prop === "maximumDistance") { continue; }   // in Min / max distance
        add("m:" + def.prop, def.label, bebMacroPlain(def, info.macros[def.prop]));
    }
    var tags = info.tags.slice().sort();
    add("tags", "Tags", tags.length ? tags.join(", ") : "none");
    add("note", "Note", info.note ? "has a note" : "no note");
    return out;
}

function bebDiffAnalyse(infos, groups) {
    var by = {}, order = [], score = [], reasons = [];
    for (var i = 0; i < infos.length; i++) {
        score.push(0); reasons.push([]);
        var fs = bebFacets(infos[i]);
        for (var f = 0; f < fs.length; f++) {
            var k = fs[f].key;
            if (!by[k]) { by[k] = { key: k, label: fs[f].label, vals: [], who: [], byId: {} }; order.push(by[k]); }
            by[k].vals.push(fs[f].value);
            by[k].who.push(i);
            by[k].byId[infos[i].id] = fs[f].value;
        }
    }
    var common = [], diffs = [], outliers = [];
    for (var o = 0; o < order.length; o++) {
        var F = order[o];
        F.present = F.vals.length;
        F.dist = bebDist(F.vals);
        if (F.dist.length === 1) { common.push(F); continue; }
        F.mode = F.dist[0].v;
        F.majority = F.dist[0].n * 2 > F.present;
        F.off = F.present - F.dist[0].n;
        F.rare = {};
        diffs.push(F);
        if (F.majority) {
            var lim = Math.max(1, Math.floor(F.present / 4));
            for (var d = 1; d < F.dist.length; d++) {
                if (F.dist[d].n > lim) { continue; }
                F.rare["#" + F.dist[d].v] = true;
                var evs = [];
                for (var w = 0; w < F.who.length; w++) { if (F.vals[w] === F.dist[d].v) { evs.push(infos[F.who[w]]); } }
                outliers.push({ facet: F, value: F.dist[d].v, events: evs });
            }
            for (var x = 0; x < F.who.length; x++) {
                if (F.vals[x] !== F.mode) { score[F.who[x]]++; reasons[F.who[x]].push({ facet: F, value: F.vals[x] }); }
            }
        }
        // Does this difference line up with the pattern groups (uniform inside each)?
        if (groups.length > 1 && F.key !== "chain") {
            var gOf = {};
            for (var g = 0; g < groups.length; g++) { for (var e = 0; e < groups[g].events.length; e++) { gOf[groups[g].events[e].id] = g; } }
            var seen = {}, follows = true, backed = false;
            for (var y = 0; y < F.who.length && follows; y++) {
                var gi = gOf[infos[F.who[y]].id];
                if (seen[gi] === undefined) { seen[gi] = F.vals[y]; }
                else if (seen[gi] !== F.vals[y]) { follows = false; }
                else { backed = true; }                      // two events in one group agree
            }
            F.followsGroups = follows && backed;
        }
    }
    diffs.sort(function (a, b) { return b.off - a.off || b.dist.length - a.dist.length; });
    outliers.sort(function (a, b) { return a.events.length - b.events.length; });
    var ranked = [];
    for (var r = 0; r < infos.length; r++) { if (score[r] > 0) { ranked.push({ info: infos[r], score: score[r], reasons: reasons[r] }); } }
    ranked.sort(function (a, b) { return b.score - a.score || (a.info.path < b.info.path ? -1 : 1); });
    return { facets: order, common: common, diffs: diffs, outliers: outliers, ranked: ranked };
}

function bebDiffVal(s) { return bebBreakable(bebEsc(bebTrunc(s, 90))); }

function bebDiffBrief(infos, groups, a) {
    var n = infos.length, ps = [];
    if (!a.diffs.length) {
        return "<p>All <b>" + bebPlural(n, "event") + "</b> are set up the same way on all " + a.facets.length +
               " things this view compares.</p>";
    }
    ps.push("<b>" + bebPlural(n, "event") + "</b> agree on <b>" + a.common.length + "</b> of the " + a.facets.length +
            " things compared and differ on <b>" + a.diffs.length + "</b>.");
    var top = [];
    for (var t = 0; t < a.diffs.length && t < 3; t++) {
        top.push("<b>" + bebEsc(a.diffs[t].label) + "</b> " + bebGrey("(" + a.diffs[t].dist.length + " versions)"));
    }
    ps.push("The biggest differences are in " + top.join(", ") + ".");
    if (groups.length > 1) {
        var follow = [], inside = [];
        for (var d = 0; d < a.diffs.length; d++) {
            if (a.diffs[d].key === "chain") { continue; }
            (a.diffs[d].followsGroups ? follow : inside).push(bebEsc(a.diffs[d].label));
        }
        var s = "They fall into <b>" + groups.length + " groups</b> by master chain.";
        if (follow.length) { s += " " + follow.slice(0, 4).join(", ") + (follow.length > 4 ? " and " + (follow.length - 4) + " more" : "") + (follow.length === 1 ? " also follows" : " also follow") + " the groups."; }
        if (inside.length) { s += " " + inside.slice(0, 4).join(", ") + (inside.length > 4 ? " and " + (inside.length - 4) + " more" : "") + (inside.length === 1 ? " varies" : " vary") + " inside groups."; }
        ps.push(s);
    } else {
        ps.push("They all share one master chain.");
    }
    if (a.outliers.length) {
        // One-event outliers are summed up per event; shared rare values get a line each.
        var lines = [], perEv = {}, evOrder = [];
        for (var o = 0; o < a.outliers.length; o++) {
            var ol = a.outliers[o];
            if (ol.events.length === 1) {
                var id = ol.events[0].id;
                if (!perEv[id]) { perEv[id] = { info: ol.events[0], what: [] }; evOrder.push(perEv[id]); }
                perEv[id].what.push(bebEsc(ol.facet.label));
            } else {
                lines.push(ol.events.length + " events have " + bebEsc(ol.facet.label) + " <b>" + bebDiffVal(ol.value) + "</b>" +
                           bebGrey(" (most: " + bebDiffVal(ol.facet.mode) + ")"));
            }
        }
        evOrder.sort(function (x, y) { return y.what.length - x.what.length; });
        for (var q = 0; q < evOrder.length; q++) {
            lines.push(bebEventHtml(evOrder[q].info.path) + " is the odd one out on " + evOrder[q].what.join(", "));
        }
        ps.push("<b>Outliers</b> " + bebGrey("(" + bebPlural(a.outliers.length, "rare value") + ", details below):") + "<br/>" +
                lines.slice(0, 6).join("<br/>") + (lines.length > 6 ? "<br/>" + bebGrey("… " + (lines.length - 6) + " more below") : ""));
    } else {
        ps.push("<b>No outliers:</b> where they differ, it's a broad split rather than one or two odd events.");
    }
    if (a.ranked.length) {
        var best = a.ranked[0], tied = [];
        for (var r = 1; r < a.ranked.length; r++) { if (a.ranked[r].score === best.score) { tied.push(a.ranked[r]); } }
        var why = [];
        for (var w = 0; w < best.reasons.length && w < 4; w++) {
            why.push(bebEsc(best.reasons[w].facet.label) + " <b>" + bebDiffVal(best.reasons[w].value) + "</b>" +
                     bebGrey(" (most: " + bebDiffVal(best.reasons[w].facet.mode) + ")"));
        }
        ps.push("<b>Differs the most:</b> " + bebEventHtml(best.info.path) + ", off the majority on " +
                bebPlural(best.score, "thing") + (tied.length ? bebGrey(" (tied with " + bebPlural(tied.length, "other event") + ")") : "") +
                ":<br/>" + why.join("<br/>") + (best.reasons.length > 4 ? "<br/>" + bebGrey("… " + (best.reasons.length - 4) + " more") : ""));
    } else {
        ps.push("<b>No event stands out:</b> every difference is an even split, with no majority to differ from.");
    }
    return "<p>" + ps.join("</p><p>") + "</p>";
}

function bebViewDifferences(infos, groups) {
    if (infos.length < 2) { return bebGrey("Select two or more events to compare them."); }
    var a = bebDiffAnalyse(infos, groups);
    var out = bebCard("Summary", "how these events differ", bebDiffBrief(infos, groups, a));
    if (!a.diffs.length) { return out; }

    var rows = [];
    for (var d = 0; d < a.diffs.length; d++) {
        var F = a.diffs[d], parts = [];
        for (var v = 0; v < F.dist.length && v < 8; v++) {
            var t = bebDiffVal(F.dist[v].v) + " " + bebGrey("×" + F.dist[v].n);
            parts.push(F.rare["#" + F.dist[v].v] ? bebWarn(t) : t);
        }
        if (F.dist.length > 8) { parts.push(bebGrey("+" + (F.dist.length - 8) + " more")); }
        var note = F.key === "chain" ? "defines the groups" : F.followsGroups ? "follows the groups" : "";
        rows.push([bebEsc(F.label) + (note ? "<br/>" + bebGrey(note) : "") +
                   (F.present < infos.length ? "<br/>" + bebGrey(F.present + " of " + infos.length + " have it") : ""),
                   parts.join("<br/>")]);
    }
    out += bebCard("Differences", "biggest split first · " + bebWarn("orange") + bebGrey(" = outlier value"), bebRows(rows));

    if (a.outliers.length) {
        var orows = [];
        for (var o = 0; o < a.outliers.length && o < 15; o++) {
            var ol = a.outliers[o], ev = [];
            for (var e = 0; e < ol.events.length && e < 6; e++) { ev.push(bebEventHtml(ol.events[e].path)); }
            if (ol.events.length > 6) { ev.push(bebGrey("… " + (ol.events.length - 6) + " more")); }
            orows.push([bebEsc(ol.facet.label) + "<br/>" + bebWarn(bebDiffVal(ol.value)) + bebGrey(" vs " + bebDiffVal(ol.facet.mode)),
                        ev.join("<br/>")]);
        }
        out += bebCard("Outliers", "rare values where most events agree", bebRows(orows));
    }

    if (a.ranked.length) {
        var rrows = [];
        for (var r = 0; r < a.ranked.length && r < 5; r++) {
            var rk = a.ranked[r], why = [];
            for (var w = 0; w < rk.reasons.length; w++) {
                why.push(bebEsc(rk.reasons[w].facet.label) + " " + bebDiffVal(rk.reasons[w].value) + bebGrey(" (most: " + bebDiffVal(rk.reasons[w].facet.mode) + ")"));
            }
            rrows.push([bebEventHtml(rk.info.path) + "<br/>" + bebGrey(bebPlural(rk.score, "difference")), why.join("<br/>")]);
        }
        out += bebCard("Most different events", "off the majority most often", bebRows(rrows));
    }

    if (groups.length > 1) {
        var grows = [];
        for (var g = 0; g < groups.length && g < 12; g++) {
            var G = groups[g], marks = [];
            for (var q = 0; q < a.diffs.length; q++) {
                var D = a.diffs[q];
                if (D.key === "chain" || !D.followsGroups || !D.majority) { continue; }
                var gv = undefined;
                for (var z = 0; z < G.events.length && gv === undefined; z++) { gv = D.byId[G.events[z].id]; }
                if (gv !== undefined && gv !== D.mode) { marks.push(bebEsc(D.label) + " " + bebDiffVal(gv)); }
            }
            grows.push(["<b>" + G.letter + "</b> " + bebGrey("· " + bebPlural(G.events.length, "event")),
                        bebGrey(bebDiffVal(G.sig)) + (marks.length ? "<br/>" + marks.join("<br/>") : "")]);
        }
        out += bebCard("Groups compared", "each group's chain, and where the whole group departs from the majority", bebRows(grows));
    }

    var crows = [];
    for (var c = 0; c < a.common.length; c++) {
        var C = a.common[c];
        crows.push([bebGrey(bebEsc(C.label)), bebDiffVal(C.dist[0].v) +
                    (C.present < infos.length ? " " + bebGrey("(the " + C.present + " that have it)") : "")]);
    }
    if (crows.length) { out += bebCard("In common", bebPlural(crows.length, "thing") + " every event shares", bebRows(crows)); }
    return out;
}

function bebRenderView(view, infos, groups) {
    if (!infos.length) {
        return bebWrap(bebGrey("Select events or event folders in the Event Browser (Events tab)."));
    }
    var body;
    switch (BEB_VIEWS[view]) {
        case "Differences":  body = bebViewDifferences(infos, groups); break;
        case "Effects":      body = bebUniqueSummary(infos, true); break;
        case "Spatializer":  body = bebSpatialSummary(infos, false) + bebSpatialOutliers(infos); break;
        case "Sheets":       body = bebSheetsSummary(infos, true); break;
        case "Macros":       body = bebMacrosView(infos); break;
        case "Tags & Notes": body = bebTagsSummary(infos, true); break;
        case "Per event":    body = bebPerEventView(infos); break;
        default:             body = bebViewPatterns(infos, groups);
    }
    return bebWrap(body);
}

// "1 / 20 (macro)" or "2 / 40 (override)" for an event's first spatializer.
function bebEffectiveRange(info) {
    var sp = info.spatial[0];
    var mn, mx, ov = false;
    for (var k = 0; k < sp.values.length; k++) {
        var n = sp.values[k].spec.name;
        if (n === "minimumDistance") { mn = sp.values[k].v; }
        if (n === "maximumDistance") { mx = sp.values[k].v; }
        if (n === "overrideRange") { ov = !!sp.values[k].v; }
    }
    if (!ov) {
        mn = info.macros.minimumDistance;
        mx = info.macros.maximumDistance;
    }
    return bebNum(mn) + " / " + bebNum(mx) + (ov ? " (override)" : " (macro)");
}

// Events whose effective min/max differ from the most common pair.
function bebSpatialOutliers(infos) {
    var pairs = [], who = [];
    for (var i = 0; i < infos.length; i++) {
        if (!infos[i].spatial.length) { continue; }
        var key = bebEffectiveRange(infos[i]);
        pairs.push(key);
        who.push({ key: key, path: infos[i].path });
    }
    var dist = bebDist(pairs);
    if (dist.length < 2) { return ""; }
    var rows = [];
    for (var d = 1; d < dist.length; d++) {
        var paths = [];
        for (var w = 0; w < who.length; w++) { if (who[w].key === dist[d].v) { paths.push(who[w].path); } }
        rows.push([bebEsc(dist[d].v) + " " + bebGrey("×" + dist[d].n),
                   bebEventHtml(paths.slice(0, 12).join("\n")).split("\n").join("<br/>") + (paths.length > 12 ? "<br/>" + bebGrey("…") : "")]);
    }
    return bebCard("Effective min / max distance — outliers", "most common: " + bebEsc(dist[0].v) + " ×" + dist[0].n, bebRows(rows));
}

// ── Export ─────────────────────────────────────────────────────────────────
function bebTsv(infos) {
    var head = ["Event", "Driven by", "Parameter sheets", "Master chain", "Other track effects",
                "Spatializer", "Distance override", "Min", "Max", "Rolloff", "2D pan mix", "Envelopment", "Tags", "Note"];
    for (var m = 0; m < BEB_MACROS.length; m++) { head.push(BEB_MACROS[m].label); }
    var lines = [head.join("\t")];
    function clean(s) {
        return ("" + s).split("\t").join(" ").split("\r").join(" ").split("\n").join(" ")
                       .split(" › ").join(" > ").split("«").join("").split("»").join("");
    }
    function sv(spv, n) { return spv[n] ? bebFmt(spv[n].spec, spv[n].v) : ""; }
    for (var i = 0; i < infos.length; i++) {
        var e = infos[i];
        var sp = e.spatial[0];
        var spv = {};
        if (sp) { for (var k = 0; k < sp.values.length; k++) { spv[sp.values[k].spec.name] = sp.values[k]; } }
        var sheets = [];
        for (var s = 0; s < e.sheets.length; s++) { sheets.push(e.sheets[s].name); }
        var row = [e.path, e.drive, sheets.join(", "), e.sig, e.otherFx.join(" | "),
                   sp ? bebFxRow(sp) : "",
                   sv(spv, "overrideRange"), sv(spv, "minimumDistance"), sv(spv, "maximumDistance"),
                   sv(spv, "distanceRolloffType"), sv(spv, "panBlend"), sv(spv, "extentMode"),
                   e.tags.join(", "), e.note];
        for (var mm = 0; mm < BEB_MACROS.length; mm++) { row.push(bebMacroFmt(BEB_MACROS[mm], e.macros[BEB_MACROS[mm].prop])); }
        for (var c = 0; c < row.length; c++) { row[c] = clean(row[c]); }
        lines.push(row.join("\t"));
    }
    return lines;
}

function bebCopyToClipboard(lines) {
    var isWindows = false;
    try { isWindows = ("" + studio.os.platform) === "win"; } catch (e) {}
    var payload = lines.join(isWindows ? "\r\n" : "\n");
    try {
        // standardInput: undocumented, from FMOD's own SVN.sc.js (as in Copy Selected Event Names).
        // pbcopy decodes stdin by LANG, which Studio doesn't set: without it,
        // UTF-8 text lands on the clipboard as Mac Roman ("«" → "¬´").
        var r = isWindows ? studio.system.start("clip", { standardInput: payload, timeout: 10000 })
                          : studio.system.start("/usr/bin/env", { args: ["LANG=en_US.UTF-8", "LC_ALL=en_US.UTF-8", "/usr/bin/pbcopy"],
                                                                  standardInput: payload, timeout: 10000 });
        if (r && typeof r.exitCode === "number" && r.exitCode !== 0) { return "the clipboard helper exited with code " + r.exitCode; }
        return null;
    } catch (e2) { return "" + e2; }
}

// ── Editing ────────────────────────────────────────────────────────────────
// Each edit domain has a plan function (what would change; reads only) and an
// apply function (writes, then reads every value back). The dialogs show the
// plan as a live preview, so Apply never surprises. Objects are matched by
// name or .id, never ===: Studio hands out a new wrapper on every read.

var BEB_LOG_CAP = 200;               // per-event lines kept in "Last result"

function bebWrite(obj, name, value) {
    var res = { ok: false, before: undefined, after: undefined, error: null };
    var mp = null;
    try { mp = obj.properties[name]; } catch (e) {}
    if (!mp) { res.error = "no property \"" + name + "\""; return res; }
    try { res.before = mp.value; } catch (e1) {}
    try { mp.setValue(value); } catch (e2) { res.error = "" + e2; return res; }
    try { res.after = mp.value; } catch (e3) {}
    res.ok = bebSame(res.after, value);
    if (!res.ok) { res.error = "didn't stick (wanted " + value + ", read back " + res.after + ")"; }
    return res;
}

function bebTrim(s) {
    s = "" + s;
    var a = 0, b = s.length;
    while (a < b && " \t\r\n".indexOf(s.charAt(a)) >= 0) { a++; }
    while (b > a && " \t\r\n".indexOf(s.charAt(b - 1)) >= 0) { b--; }
    return s.substring(a, b);
}

// Per-event lines, capped, after a summary.
function bebResultLines(summary, items) {
    var out = summary.slice();
    for (var i = 0; i < items.length && i < BEB_LOG_CAP; i++) { out.push(items[i]); }
    if (items.length > BEB_LOG_CAP) { out.push("  … " + (items.length - BEB_LOG_CAP) + " more (see the console)"); }
    return out;
}

function bebW(w, id) { try { return w.findWidget(id); } catch (e) { return null; } }
function bebWText(w, id) { var x = bebW(w, id); try { return x ? "" + x.text() : ""; } catch (e) { return ""; } }
function bebWIndex(w, id) { var x = bebW(w, id); try { return x ? x.currentIndex() : 0; } catch (e) { return 0; } }
function bebWChecked(w, id) { var x = bebW(w, id); try { return !!(x && x.isChecked()); } catch (e) { return false; } }

// Preview box + Apply/Cancel, shared by the three dialogs.
function bebDialogFooter(W, G, prefix, onApply) {
    return [
        { widgetType: W.TextEdit, widgetId: prefix + "preview", isReadOnly: true, html: "", minimumHeight: 190 },
        { widgetType: W.Label, wordWrap: true,
          text: "<span style=\"color:#9a9a9a;\">The main window's Undo button reverts the whole batch.</span>" },
        { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
            { widgetType: W.Spacer },
            { widgetType: W.PushButton, widgetId: prefix + "cancel", text: "Cancel", minimumWidth: 100,
              onClicked: function () { this.closeDialog(); } },
            { widgetType: W.PushButton, widgetId: prefix + "apply", text: "Apply", minimumWidth: 160, isEnabled: false,
              onClicked: onApply }
        ] }
    ];
}

function bebSetPreview(w, prefix, html, applyText, canApply) {
    var pv = bebW(w, prefix + "preview");
    if (pv) { try { pv.setHtml("<html><body style=\"color:#dddddd;\">" + html + "</body></html>"); } catch (e) {} }
    var ap = bebW(w, prefix + "apply");
    if (ap) { try { ap.setText(applyText); ap.setEnabled(!!canApply); } catch (e2) {} }
}

function bebPreviewList(title, rows, cap) {
    if (!rows.length) { return ""; }
    var h = "<p><b>" + bebEsc(title) + "</b><br/>";
    for (var i = 0; i < rows.length && i < cap; i++) { h += rows[i] + "<br/>"; }
    if (rows.length > cap) { h += bebGrey("… " + (rows.length - cap) + " more") + "<br/>"; }
    return h + "</p>";
}

// ── Tags ───────────────────────────────────────────────────────────────────
function bebWalkTags(items, out, depth) {
    var l = bebList(items);
    for (var i = 0; i < l.length; i++) {
        if (bebIs(l[i], "Tag")) { out.push(l[i]); }
        else if (depth < 12) { bebWalkTags(bebGet(l[i], "items"), out, depth + 1); }
    }
}

function bebProjectTags() {
    var out = [];
    try { bebWalkTags(bebGet(studio.project.workspace.masterTagFolder, "items"), out, 0); } catch (e) {}
    return out;
}

function bebFindTag(name) {
    var t = bebProjectTags();
    for (var i = 0; i < t.length; i++) { if ("" + bebGet(t[i], "name") === name) { return t[i]; } }
    return null;
}

function bebEventTag(ev, name) {
    var t = bebList(bebGet(ev, "tags"));
    for (var i = 0; i < t.length; i++) { if ("" + bebGet(t[i], "name") === name) { return t[i]; } }
    return null;
}

// mode "add" | "remove" → { name, change: [info], skip: [info], create }
function bebTagPlan(infos, mode, name) {
    var plan = { name: name, change: [], skip: [], create: false };
    if (!name) { return plan; }
    for (var i = 0; i < infos.length; i++) {
        var has = !!bebEventTag(infos[i].ev, name);
        if ((mode === "add") !== has) { plan.change.push(infos[i]); } else { plan.skip.push(infos[i]); }
    }
    plan.create = mode === "add" && plan.change.length > 0 && !bebFindTag(name);
    return plan;
}

function bebApplyTags(plan, mode) {
    var items = [], ok = 0, bad = 0, head = [];
    var tag = null;
    if (mode === "add" && plan.change.length) {
        tag = bebFindTag(plan.name);
        if (!tag) {
            try { tag = studio.project.workspace.addTag(plan.name); } catch (e) {}
            if (!tag || !bebFindTag(plan.name)) {
                return { ok: 0, bad: plan.change.length, lines: ["! Couldn't create the tag \"" + plan.name + "\". Nothing changed."] };
            }
            head.push("+ Created the tag \"" + plan.name + "\"");
        }
    }
    var done = [];
    for (var i = 0; i < plan.change.length; i++) {
        var info = plan.change[i], err = null;
        try {
            if (mode === "add") { info.ev.relationships.tags.add(tag); }
            else { info.ev.relationships.tags.remove(bebEventTag(info.ev, plan.name)); }
        } catch (e2) { err = "" + e2; }
        var has = !!bebEventTag(info.ev, plan.name);
        if (!err && has === (mode === "add")) { ok++; done.push({ ev: info.ev, path: info.path }); items.push((mode === "add" ? "+ " : "- ") + info.path); }
        else { bad++; items.push("! " + info.path + "  " + (err || "didn't stick")); }
    }
    var verb = mode === "add" ? "Added" : "Removed";
    var summary = [(bad ? "! " : "+ ") + verb + " tag \"" + plan.name + "\" " + (mode === "add" ? "to " : "from ") + bebPlural(ok, "event") +
                   (plan.skip.length ? " · " + plan.skip.length + " already " + (mode === "add" ? "had it" : "without it") : "") +
                   (bad ? " · " + bad + " failed" : "")];
    return { ok: ok, bad: bad, lines: bebResultLines(summary.concat(head), items),
             undo: { kind: "tags", mode: mode, name: plan.name, created: head.length > 0, events: done,
                     label: "Tags: " + (mode === "add" ? "added “" + plan.name + "” to " : "removed “" + plan.name + "” from ") + bebPlural(ok, "event") } };
}

// Returns { lines } after applying, or null when cancelled.
function bebTagsDialog(infos) {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    var result = null;
    var pickNames = [];
    var MODES = ["add", "remove"];

    function pickItems(mode) {
        pickNames = [];
        var items = [{ text: mode === "add" ? "(pick an existing tag, or type a new one)" : "(pick a tag on these events)" }];
        if (mode === "add") {
            var all = bebProjectTags(), names = [];
            for (var i = 0; i < all.length; i++) { names.push("" + bebGet(all[i], "name")); }
            names.sort();
            for (var j = 0; j < names.length; j++) { pickNames.push(names[j]); items.push({ text: names[j] }); }
        } else {
            var on = [];
            for (var k = 0; k < infos.length; k++) { on = on.concat(infos[k].tags); }
            var d = bebDist(on);
            for (var m = 0; m < d.length; m++) { pickNames.push(d[m].v); items.push({ text: d[m].v + "  ·  " + bebUsageText(d[m].n, infos.length) }); }
        }
        return items;
    }

    function update(w) {
        var mode = MODES[bebWIndex(w, "bebt_mode")];
        var name = bebTrim(bebWText(w, "bebt_name"));
        if (!name) { bebSetPreview(w, "bebt_", bebGrey("Type or pick a tag."), "Apply", false); return; }
        var plan = bebTagPlan(infos, mode, name);
        var h = "<p><b>" + (mode === "add" ? "Add" : "Remove") + " “" + bebEsc(name) + "”</b> " +
                (mode === "add" ? "to " : "from ") + bebPlural(plan.change.length, "event") +
                (plan.skip.length ? bebGrey(" · " + plan.skip.length + (mode === "add" ? " already have it" : " don't have it")) : "") + "</p>";
        if (plan.create) { h += "<p><span style=\"color:#8fc7ff;\">Creates a new project tag “" + bebEsc(name) + "”.</span></p>"; }
        var rows = [];
        for (var i = 0; i < plan.change.length; i++) { rows.push((mode === "add" ? "+ " : "- ") + bebEventHtml(plan.change[i].path)); }
        h += bebPreviewList("Events", rows, 40);
        bebSetPreview(w, "bebt_", h, plan.change.length ? (mode === "add" ? "Add to " : "Remove from ") + bebPlural(plan.change.length, "event") : "Nothing to change",
                      plan.change.length > 0);
    }

    studio.ui.showModalDialog({
        windowTitle: "Tags — " + bebPlural(infos.length, "event"),
        windowWidth: 560, windowHeight: 440,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [
            { widgetType: W.Layout, layout: G.GridLayout, spacing: 6, items: [
                { widgetType: W.Label, text: "Action", row: 0, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebt_mode", row: 0, column: 1, currentIndex: 0,
                  items: [{ text: "Add a tag to every event" }, { text: "Remove a tag from every event" }],
                  onCurrentIndexChanged: function () {
                      var p = bebW(this, "bebt_pick");
                      if (p) { try { p.setItems(pickItems(MODES[this.currentIndex()])); p.setCurrentIndex(0); } catch (e) {} }
                      update(this);
                  } },
                { widgetType: W.Label, text: "Tag", row: 1, column: 0 },
                { widgetType: W.LineEdit, widgetId: "bebt_name", row: 1, column: 1, text: "",
                  onTextEdited: function () { update(this); } },
                { widgetType: W.Label, text: "Pick", row: 2, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebt_pick", row: 2, column: 1, currentIndex: 0, items: pickItems("add"),
                  onCurrentIndexChanged: function () {
                      var i = this.currentIndex();
                      if (i > 0 && pickNames[i - 1] !== undefined) {
                          var n = bebW(this, "bebt_name");
                          if (n) { try { n.setText(pickNames[i - 1]); } catch (e) {} }
                      }
                      update(this);
                  } }
            ] }
        ].concat(bebDialogFooter(W, G, "bebt_", function () {
            var mode = MODES[bebWIndex(this, "bebt_mode")];
            var plan = bebTagPlan(infos, mode, bebTrim(bebWText(this, "bebt_name")));
            if (!plan.change.length) { return; }
            result = bebApplyTags(plan, mode);
            this.closeDialog();
        })),
        onConstructed: function () { update(this); }
    });
    return result;
}

// ── Notes ──────────────────────────────────────────────────────────────────
// mode "append" | "remove" | "clear". The new note, or null when unchanged.
function bebNoteAfter(note, mode, text, opts) {
    note = "" + (note || "");
    if (mode === "clear") { return note ? "" : null; }
    if (!text) { return null; }
    if (mode === "append") {
        if (opts.skipIfPresent && note.indexOf(text) >= 0) { return null; }
        return note ? note + (opts.newLine ? "\n" : " ") + text : text;
    }
    if (note.indexOf(text) < 0) { return null; }        // remove: case-sensitive, every occurrence
    return bebTrim(note.split(text).join(""));
}

function bebNotePlan(infos, mode, text, opts) {
    var plan = { change: [], skip: 0 };
    for (var i = 0; i < infos.length; i++) {
        var before = "" + (bebProp(infos[i].ev, "note") || "");
        var after = bebNoteAfter(before, mode, text, opts);
        if (after === null) { plan.skip++; } else { plan.change.push({ info: infos[i], before: before, after: after }); }
    }
    return plan;
}

function bebApplyNotes(plan, mode) {
    var items = [], ok = 0, bad = 0, entries = [];
    for (var i = 0; i < plan.change.length; i++) {
        var c = plan.change[i];
        var r = bebWrite(c.info.ev, "note", c.after);
        if (r.ok) { ok++; entries.push({ obj: c.info.ev, prop: "note", path: c.info.path, label: "Note", before: c.before, after: r.after });
                    items.push((mode === "clear" ? "- " : "~ ") + c.info.path); }
        else { bad++; items.push("! " + c.info.path + "  " + r.error); }
    }
    var verb = mode === "append" ? "Appended to" : mode === "clear" ? "Cleared" : "Removed text from";
    return { ok: ok, bad: bad, lines: bebResultLines([(bad ? "! " : "+ ") + verb + " " + bebPlural(ok, "note") +
             (plan.skip ? " · " + plan.skip + " unchanged" : "") + (bad ? " · " + bad + " failed" : "")], items),
             undo: { kind: "values", entries: entries, label: "Notes: " + verb.toLowerCase() + " " + bebPlural(ok, "note") } };
}

// Note text as HTML: line breaks kept, long text cut from the given side.
function bebNoteHtml(s, cap, fromEnd) {
    s = "" + s;
    if (s.length > cap) { s = fromEnd ? "…" + s.substring(s.length - cap + 1) : s.substring(0, cap - 1) + "…"; }
    return bebEsc(s).split("\n").join("<br/>");
}

// One card per event, showing only what changes: appended text in green after
// the end of the current note, removed text struck through in red.
function bebNoteChangeHtml(c, mode, text, opts) {
    var ADD = BEB_ADD, DEL = BEB_DEL;
    var body;
    if (mode === "append") {
        var sep = c.before ? (opts.newLine ? "<br/>" : " ") : "";
        body = (c.before ? bebGrey(bebNoteHtml(c.before, 160, true)) : bebGrey("(empty note)") + "<br/>") +
               sep + ADD + bebNoteHtml(text, 400) + "</span>";
    } else if (mode === "clear") {
        body = DEL + bebNoteHtml(c.before, 400) + "</span>";
    } else {
        var parts = c.before.split(text), segs = [];
        for (var i = 0; i < parts.length; i++) {
            var seg = parts[i];
            if (seg.length > 120) { seg = i === 0 ? "…" + seg.substring(seg.length - 119) : i === parts.length - 1 ? seg.substring(0, 119) + "…" : seg.substring(0, 58) + " … " + seg.substring(seg.length - 58); }
            segs.push(bebGrey(bebEsc(seg).split("\n").join("<br/>")));
        }
        body = segs.join(DEL + bebNoteHtml(text, 200) + "</span>");
    }
    return bebCard(c.info.path, "", body, BEB_C.event);
}

function bebNotesDialog(infos) {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    var result = null;
    var MODES = ["append", "remove", "clear"];
    var lastSig = null, timerId = null;

    // The text box is a multi-line TextEdit (Enter makes a new line). Studio gives
    // it no change callback, so the preview polls it on a timer. The ends are
    // trimmed: a stray trailing space or newline is never what's meant.
    function read(w) {
        return { mode: MODES[bebWIndex(w, "bebn_mode")], text: bebTrim(bebWText(w, "bebn_text")),
                 opts: { newLine: bebWChecked(w, "bebn_newline"), skipIfPresent: bebWChecked(w, "bebn_skip") } };
    }

    function update(w) {
        var o = read(w);
        var sig = o.mode + "|" + o.opts.newLine + "|" + o.opts.skipIfPresent + "|" + o.text;
        if (sig === lastSig) { return; }
        lastSig = sig;
        var t = bebW(w, "bebn_text");
        if (t) { try { t.setEnabled(o.mode !== "clear"); } catch (e) {} }
        var nl = bebW(w, "bebn_newline"), sk = bebW(w, "bebn_skip");
        try { if (nl) { nl.setEnabled(o.mode === "append"); } if (sk) { sk.setEnabled(o.mode === "append"); } } catch (e2) {}
        if (o.mode !== "clear" && !o.text) {
            bebSetPreview(w, "bebn_", bebGrey("Type the text to " + (o.mode === "append" ? "append. Enter starts a new line." : "find and remove (case-sensitive).")), "Apply", false);
            return;
        }
        var plan = bebNotePlan(infos, o.mode, o.text, o.opts);
        var h = "<p><b>" + bebPlural(plan.change.length, "note") + " will change</b>" + (plan.skip ? bebGrey(" · " + plan.skip + " unchanged") : "") + "</p>";
        if (o.mode === "clear" && plan.change.length) {
            h += "<p><span style=\"color:#ff9a6a;\">Clearing deletes these notes' text. Only Undo or source control brings it back.</span></p>";
        }
        for (var i = 0; i < plan.change.length && i < 40; i++) { h += bebNoteChangeHtml(plan.change[i], o.mode, o.text, o.opts); }
        if (plan.change.length > 40) { h += bebGrey("… " + (plan.change.length - 40) + " more"); }
        var verb = o.mode === "append" ? "Append to " : o.mode === "clear" ? "Clear " : "Remove from ";
        bebSetPreview(w, "bebn_", h, plan.change.length ? verb + bebPlural(plan.change.length, "note") : "Nothing to change", plan.change.length > 0);
    }

    studio.ui.showModalDialog({
        windowTitle: "Notes — " + bebPlural(infos.length, "event"),
        windowWidth: 640, windowHeight: 620,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [
            { widgetType: W.Layout, layout: G.GridLayout, spacing: 6, items: [
                { widgetType: W.Label, text: "Action", row: 0, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebn_mode", row: 0, column: 1, currentIndex: 0,
                  items: [{ text: "Append text" }, { text: "Find & remove text" }, { text: "Clear the note" }],
                  onCurrentIndexChanged: function () { update(this); } },
                { widgetType: W.Label, text: "Text", row: 1, column: 0, alignment: studio.ui.alignment.AlignTop },
                { widgetType: W.TextEdit, widgetId: "bebn_text", row: 1, column: 1, text: "", isReadOnly: false,
                  minimumHeight: 80, maximumHeight: 110 },
                { widgetType: W.CheckBox, widgetId: "bebn_newline", row: 2, column: 1, isChecked: true,
                  text: "Append on a new line", onToggled: function () { update(this); } },
                { widgetType: W.CheckBox, widgetId: "bebn_skip", row: 3, column: 1, isChecked: true,
                  text: "Skip notes that already contain the text", onToggled: function () { update(this); } }
            ] }
        ].concat(bebDialogFooter(W, G, "bebn_", function () {
            var o = read(this);
            var plan = bebNotePlan(infos, o.mode, o.text, o.opts);
            if (!plan.change.length) { return; }
            result = bebApplyNotes(plan, o.mode);
            this.closeDialog();
        })),
        onConstructed: function () {
            update(this);
            try { timerId = this.startTimer(300); } catch (e) {}
        },
        onTimerEvent: function () { try { update(this); } catch (e) {} },
        onClose: function () { try { if (timerId !== null) { this.stopTimer(timerId); } } catch (e) {} }
    });
    return result;
}

// ── Macros ─────────────────────────────────────────────────────────────────
// The object a macro lives on: the macro object, or the master bus for Pitch.
function bebMacroObj(ev, def) {
    if (def.on === "master") { return bebGet(bebGet(ev, "masterTrack"), "mixerGroup"); }
    return bebGet(ev, "automatableProperties");
}

function bebMacroDef(prop) {
    for (var i = 0; i < BEB_MACROS.length; i++) { if (BEB_MACROS[i].prop === prop) { return BEB_MACROS[i]; } }
    return null;
}

// jobs: [{ def, value }] → { rows: [{ info, ap, writes: [{ def, from, to }] }], changes, same, warn: [] }
// Checks per event: the min/max pair each event would end
// up with, Doppler scale while Doppler stays off, distance on events whose
// spatializer ignores the macro, and automated/modulated macros.
function bebMacroPlan(infos, jobs) {
    var plan = { rows: [], changes: 0, same: 0, warn: [] };
    var job = {}, wantsDist = false;
    for (var j = 0; j < jobs.length; j++) {
        job[jobs[j].def.prop] = jobs[j].value;
        if (jobs[j].def.prop === "minimumDistance" || jobs[j].def.prop === "maximumDistance") { wantsDist = true; }
    }
    var badPair = [], noMacros = [], notSpatial = [], overridden = [], dopplerOff = [], driven = [];
    for (var i = 0; i < infos.length; i++) {
        var info = infos[i];
        var ap = bebGet(info.ev, "automatableProperties");
        if (!ap) { noMacros.push(info.path); continue; }
        var cur = {}, objs = {};
        for (var m = 0; m < BEB_MACROS.length; m++) {
            objs[BEB_MACROS[m].prop] = bebMacroObj(info.ev, BEB_MACROS[m]);
            cur[BEB_MACROS[m].prop] = bebProp(objs[BEB_MACROS[m].prop], BEB_MACROS[m].prop);
        }
        var skipDist = false;
        if (wantsDist) {
            var mn = "minimumDistance" in job ? job.minimumDistance : cur.minimumDistance;
            var mx = "maximumDistance" in job ? job.maximumDistance : cur.maximumDistance;
            if (mn > mx) { skipDist = true; badPair.push(info.path + " (min " + bebNum(mn) + " > max " + bebNum(mx) + ")"); }
            if (!info.spatial.length) { notSpatial.push(info.path); }
            else if (bebSpatialOverrides(info)) { overridden.push(info.path); }
        }
        if ("dopplerScale" in job && !("dopplerEnabled" in job ? job.dopplerEnabled : cur.dopplerEnabled)) { dopplerOff.push(info.path); }
        for (var d = 0; d < info.macroDriven.length; d++) {
            if (info.macroDriven[d].prop in job) { driven.push(info.path + " → " + bebMacroDef(info.macroDriven[d].prop).label); }
        }
        var writes = [];
        for (var k = 0; k < jobs.length; k++) {
            var p = jobs[k].def.prop;
            if (skipDist && (p === "minimumDistance" || p === "maximumDistance")) { continue; }
            if (bebSame(cur[p], jobs[k].value)) { plan.same++; continue; }
            if (!objs[p]) { continue; }
            writes.push({ def: jobs[k].def, from: cur[p], to: jobs[k].value, obj: objs[p] });
        }
        // Raising min above the current max: write max first so the pair stays valid.
        if (job.minimumDistance !== undefined && job.minimumDistance > cur.maximumDistance) {
            writes.sort(function (a, b) { return (a.def.prop === "maximumDistance" ? 0 : 1) - (b.def.prop === "maximumDistance" ? 0 : 1); });
        }
        plan.changes += writes.length;
        if (writes.length) { plan.rows.push({ info: info, ap: ap, writes: writes }); }
    }
    function warn(list, text) { if (list.length) { plan.warn.push({ text: list.length + " " + text, events: list }); } }
    warn(badPair, "would end up with min > max distance: distance skipped for them");
    warn(notSpatial, "have no spatializer: min/max distance does nothing there");
    warn(overridden, "have a spatializer that overrides the range: the macro min/max is ignored there");
    warn(dopplerOff, "keep Doppler off: Doppler scale does nothing there");
    warn(driven, "automated or modulated: the base value is overridden at runtime");
    warn(noMacros, "have no macro object: skipped");
    return plan;
}

function bebSpatialOverrides(info) {
    var vals = info.spatial[0].values;
    for (var i = 0; i < vals.length; i++) { if (vals[i].spec.name === "overrideRange") { return !!vals[i].v; } }
    return false;
}

function bebApplyMacros(plan) {
    var items = [], ok = 0, bad = 0, entries = [];
    for (var i = 0; i < plan.rows.length; i++) {
        var row = plan.rows[i], parts = [], failed = [];
        for (var j = 0; j < row.writes.length; j++) {
            var wr = row.writes[j];
            var r = bebWrite(wr.obj, wr.def.prop, wr.to);
            if (r.ok) { ok++; entries.push({ obj: wr.obj, prop: wr.def.prop, path: row.info.path, label: wr.def.label, def: wr.def, before: wr.from, after: r.after });
                        parts.push(wr.def.label + " " + bebMacroFmt(wr.def, wr.from) + " → " + bebMacroFmt(wr.def, wr.to)); }
            else { bad++; failed.push(wr.def.label + ": " + r.error); }
        }
        if (parts.length) { items.push("~ " + row.info.path + "  " + parts.join(", ")); }
        if (failed.length) { items.push("! " + row.info.path + "  " + failed.join("; ")); }
    }
    var summary = [(bad ? "! " : "+ ") + "Macros: " + bebPlural(ok, "value") + " changed on " + bebPlural(plan.rows.length, "event") +
                   (plan.same ? " · " + plan.same + " already matched" : "") + (bad ? " · " + bad + " failed" : "")];
    for (var w = 0; w < plan.warn.length; w++) { summary.push("! " + plan.warn[w].text); }
    return { ok: ok, bad: bad, lines: bebResultLines(summary, items),
             undo: { kind: "values", entries: entries, label: "Macros: " + bebPlural(ok, "value") + " on " + bebPlural(plan.rows.length, "event") } };
}

// The value a row's widget shows when the dialog opens: the most common one.
function bebMacroPrefill(infos, def) {
    var vals = [];
    for (var i = 0; i < infos.length; i++) { vals.push(infos[i].macros[def.prop]); }
    var d = bebDist(vals);
    return { value: d.length ? d[0].v : undefined, mixed: d.length > 1, kinds: d.length,
             html: bebDistHtml(d, vals.length, function (v) { return bebMacroFmt(def, v); }) };
}

// ── Macros drawer (preview laid out like Studio's Event Macros drawer) ─────
// Each control shows the value after Apply: unchanged values as they are (or
// "mixed"), changed ones in green with the value they replace struck in red.
function bebDrawRange(oldMn, oldMx, newMn, newMx, changed) {
    return function (w, h, s) {
        var cv = bebCanvas(w, h, "#2f2f2f");
        var lo = -0.5, hi = 4.5, pad = 6 * s, tw = w - 2 * pad;
        function X(v) { return pad + (Math.log(Math.max(v, 0.0316)) / Math.LN10 - lo) / (hi - lo) * tw; }
        var trackTop = Math.round(h * 0.3), trackBot = Math.round(h * 0.7);
        var track = bebRgb("#1c1c1c"), rim = bebRgb("#555555"), sel = bebRgb("#8c8c8c"), green = bebRgb("#5cc06a"), red = bebRgb("#e05a5a"), white = [235, 235, 235];
        for (var y = trackTop; y <= trackBot; y++) { for (var x = pad; x < w - pad; x++) { bebPut(cv, x, y, y === trackTop || y === trackBot ? rim : track, 1); } }
        for (var t = 0; t <= 4; t++) {                                  // ticks at 1, 10, 100, 1k, 10k
            var tx = Math.round(X(Math.pow(10, t)));
            for (var ty = trackBot + 2 * s; ty < h; ty++) { bebPut(cv, tx, ty, rim, 1); }
        }
        function bar(mn, mx, col, y0, y1) {
            for (var y2 = y0; y2 <= y1; y2++) { for (var x2 = Math.round(X(mn)); x2 <= Math.round(X(mx)); x2++) { bebPut(cv, x2, y2, col, 1); } }
        }
        function handle(v, col) {
            var hx = Math.round(X(v));
            for (var y3 = trackTop - 3 * s; y3 <= trackBot + 3 * s; y3++) { for (var d = -s; d <= s; d++) { bebPut(cv, hx + d, y3, col, 1); } }
        }
        if (changed) {
            bar(oldMn, oldMx, red, trackBot - Math.round(2.5 * s), trackBot - 1);      // the range being replaced, thin, red
            bar(newMn, newMx, green, trackTop + 1, trackBot - Math.round(3 * s));
            handle(newMn, white); handle(newMx, white);
        } else {
            bar(newMn, newMx, sel, trackTop + 1, trackBot - 1);
            handle(newMn, white); handle(newMx, white);
        }
        return cv.rgb;
    };
}

function bebMacroDrawerHtml(infos, pre, jobs) {
    var job = {};
    for (var j = 0; j < jobs.length; j++) { job[jobs[j].def.prop] = jobs[j].value; }
    var Y = "#e8c14a", G = "#5cc06a";
    function state(prop) {
        var def = bebMacroDef(prop), p = pre[prop];
        var changing = prop in job && (p.mixed || !bebSame(job[prop], p.value));
        return { def: def, now: p.value, mixed: p.mixed, kinds: p.kinds, changing: changing, value: changing ? job[prop] : p.value };
    }
    function was(st) {
        return st.changing ? "<span style=\"color:#ff8a8a; font-size:9px; text-decoration:line-through;\">" +
               (st.mixed ? "mixed (" + st.kinds + ")" : bebEsc(bebMacroFmt(st.def, st.now))) + "</span>" : "";
    }
    function label(t) { return "<span style=\"color:#ffffff; font-weight:bold; font-size:11px;\">" + t + "</span><br/>"; }
    function toggle(prop) {
        var st = state(prop), segs = "";
        var opts = [false, true];
        for (var i = 0; i < 2; i++) {
            var active = !(st.mixed && !st.changing) && st.value === opts[i];
            var bg = active ? (st.changing ? G : Y) : "#4a4a4a", fg = active ? "#111111" : "#cfcfcf";
            segs += "<td bgcolor=\"" + bg + "\" style=\"color:" + fg + "; font-size:10px;\" align=\"center\">&nbsp;" + (i ? "On" : "Off") + "&nbsp;</td>";
        }
        return "<td align=\"center\" valign=\"top\">" + label(st.def.label) + "<table cellspacing=\"1\" cellpadding=\"2\" bgcolor=\"#222222\"><tr>" + segs + "</tr></table>" +
               (st.mixed && !st.changing ? bebGrey("mixed") : "") + was(st) + "</td>";
    }
    function field(prop, arrow) {
        var st = state(prop);
        var txt = st.mixed && !st.changing ? "mixed" : bebMacroFmt(st.def, st.value);
        if (txt.indexOf(" [") > 0) { txt = txt.substring(0, txt.indexOf(" [")); }      // "Medium [2]" → "Medium"
        var fg = st.changing ? G : st.mixed ? "#9a9a9a" : "#ffffff";
        return "<td align=\"center\" valign=\"top\">" + label(st.def.label) +
               "<table cellspacing=\"0\" cellpadding=\"2\" width=\"96\" bgcolor=\"#1e1e1e\"><tr><td style=\"color:" + fg + "; font-size:10px;\">" +
               (arrow ? "▸ " : "") + bebEsc(txt) + (arrow ? "" : "<span style=\"color:#9a9a9a;\">&nbsp;▾</span>") +
               (st.changing ? "&nbsp;&nbsp;" + was(st) : "") + "</td></tr></table></td>";
    }
    var mn = state("minimumDistance"), mx = state("maximumDistance");
    var distChanged = mn.changing || mx.changing;
    var img = bebImage("range|" + mn.now + "|" + mx.now + "|" + mn.value + "|" + mx.value + "|" + distChanged, 300, 26,
                       bebDrawRange(mn.now, mx.now, mn.value, mx.value, distChanged));
    var ticks = "<table width=\"300\" cellspacing=\"0\" cellpadding=\"0\"><tr>";
    var T = ["1.00", "10.0", "100", "1.00k", "10.0k"];
    for (var t = 0; t < T.length; t++) { ticks += "<td width=\"20%\" align=\"center\" style=\"color:#8a8a8a; font-size:9px;\">" + T[t] + "</td>"; }
    ticks += "</tr></table>";
    var distText = (mn.mixed && !mn.changing ? "mixed" : bebNum(mn.value)) + " – " + (mx.mixed && !mx.changing ? "mixed" : bebNum(mx.value));
    var distWas = distChanged ? " <span style=\"color:#ff8a8a; font-size:9px; text-decoration:line-through;\">" +
                  (mn.mixed ? "mixed" : bebNum(mn.now)) + " – " + (mx.mixed ? "mixed" : bebNum(mx.now)) + "</span>" : "";
    var h = "<table width=\"330\" cellspacing=\"6\" cellpadding=\"2\" bgcolor=\"#2f2f2f\">" +
        "<tr>" + toggle("isPersistent") + toggle("dopplerEnabled") + field("maxVoices", true) + "</tr>" +
        "<tr>" + field("pitch", true) + field("dopplerScale", true) + field("voiceStealing", false) + "</tr>" +
        "<tr>" + field("priority", false) + field("triggerCooldown", true) + "<td></td></tr>" +
        "<tr><td colspan=\"3\" align=\"center\">" + label("Min &amp; Max Distance") + (img || "") + "<br/>" + ticks +
        "<span style=\"color:" + (distChanged ? G : "#ffffff") + "; font-size:10px;\">" + distText + "</span>" + distWas + "</td></tr>" +
        "<tr><td colspan=\"3\" align=\"center\" style=\"color:#dddddd; font-weight:bold;\">Event Macros</td></tr></table>";
    var n = 0;
    for (var k in job) { if (job.hasOwnProperty(k) && state(k).changing) { n++; } }
    return "<html><body style=\"color:#dddddd;\"><p>" + bebGrey("After Apply, across " + bebPlural(infos.length, "event") +
           (n ? " · " : "") ) + (n ? "<span style=\"color:" + G + ";\">" + bebPlural(n, "setting") + " changing</span>" : "") + "</p>" + h + "</body></html>";
}

function bebMacrosDialog(infos) {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    var result = null;
    var pre = {};
    var grid = [];

    // Read the ticked rows. → { jobs, errors }
    function collect(w) {
        var jobs = [], errors = [];
        for (var i = 0; i < BEB_MACROS.length; i++) {
            var def = BEB_MACROS[i];
            if (!bebWChecked(w, "bebm_t_" + def.prop)) { continue; }
            var v = bebMacroWidgetValue(w, def);
            if (v.error) { errors.push(v.error); continue; }
            jobs.push({ def: def, value: v.value });
        }
        return { jobs: jobs, errors: errors };
    }

    var lastDrawer = null;
    function updateDrawer(w, jobs) {
        var html = bebMacroDrawerHtml(infos, pre, jobs);
        if (html === lastDrawer) { return; }
        lastDrawer = html;
        var d = bebW(w, "bebm_drawer");
        if (d) { try { d.setHtml(html); } catch (e) {} }
    }

    function update(w) {
        var c = collect(w);
        updateDrawer(w, c.jobs);
        var h = "";
        for (var e = 0; e < c.errors.length; e++) { h += "<p><span style=\"color:#ff9a6a;\">" + bebEsc(c.errors[e]) + "</span></p>"; }
        if (!c.jobs.length) {
            bebSetPreview(w, "bebm_", h + bebGrey("Change a value (its row ticks itself), or tick a row to apply the value shown to every event."), "Apply", false);
            return;
        }
        var plan = bebMacroPlan(infos, c.jobs);
        h += "<p><b>" + bebPlural(plan.changes, "value") + " change on " + bebPlural(plan.rows.length, "event") + "</b>" +
             (plan.same ? bebGrey(" · " + plan.same + " already match") : "") + "</p>";
        for (var x = 0; x < plan.warn.length; x++) {
            var wn = plan.warn[x];
            h += "<p><span style=\"color:#ff9a6a;\">" + bebEsc(wn.text) + "</span><br/>" +
                 bebGrey(bebEsc(wn.events.slice(0, 4).join(", ")) + (wn.events.length > 4 ? " …" : "")) + "</p>";
        }
        var rows = [];
        for (var r = 0; r < plan.rows.length; r++) {
            var parts = [];
            for (var k = 0; k < plan.rows[r].writes.length; k++) {
                var wr = plan.rows[r].writes[k];
                parts.push(bebEsc(wr.def.label) + " " + BEB_DEL + "&nbsp;" + bebEsc(bebMacroFmt(wr.def, wr.from)) + "&nbsp;</span> " +
                           bebGrey("→") + " " + BEB_ADD + "&nbsp;" + bebEsc(bebMacroFmt(wr.def, wr.to)) + "&nbsp;</span>");
            }
            rows.push(bebEventHtml(plan.rows[r].info.path) + "<br/>&nbsp;&nbsp;" + parts.join(" · "));
        }
        h += bebPreviewList("Changes", rows, 30);
        bebSetPreview(w, "bebm_", h, plan.changes ? "Apply to " + bebPlural(plan.rows.length, "event") : "Nothing to change",
                      plan.changes > 0 && !c.errors.length);
    }

    // Editing a value ticks its row when it differs from what was shown, or
    // isn't a valid number (so the error shows and blocks Apply).
    function edited(w, def) {
        var v = bebMacroWidgetValue(w, def);
        if (v.error || !bebSame(v.value, pre[def.prop].value)) {
            var t = bebW(w, "bebm_t_" + def.prop);
            if (t) { try { if (!t.isChecked()) { t.setChecked(true); } } catch (e) {} }
        }
        update(w);
    }

    grid.push({ widgetType: W.Label, text: "<b>Set</b>", row: 0, column: 0 });
    grid.push({ widgetType: W.Label, text: "<b>Value</b>", row: 0, column: 2 });
    grid.push({ widgetType: W.Label, text: "<b>Now</b>", row: 0, column: 3 });
    for (var i = 0; i < BEB_MACROS.length; i++) {
        var def = BEB_MACROS[i];
        pre[def.prop] = bebMacroPrefill(infos, def);
        var row = i + 1;
        grid.push({ widgetType: W.CheckBox, widgetId: "bebm_t_" + def.prop, row: row, column: 0, isChecked: false, text: "",
                    onToggled: function () { update(this); } });
        grid.push({ widgetType: W.Label, row: row, column: 1, text: def.label + (def.hint ? " <span style=\"color:#9a9a9a;\">(" + def.hint + ")</span>" : "") });
        grid.push(bebMacroWidget(W, def, pre[def.prop].value, edited, row));
        grid.push({ widgetType: W.Label, row: row, column: 3, text: pre[def.prop].html });
    }

    studio.ui.showModalDialog({
        windowTitle: "Macros — " + bebPlural(infos.length, "event"),
        windowWidth: 1100, windowHeight: 700,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [{ widgetType: W.Layout, layout: G.HBoxLayout, spacing: 12, items: [
            { widgetType: W.Layout, layout: G.VBoxLayout, items: [{ widgetType: W.Layout, layout: G.GridLayout, spacing: 6, items: grid }, { widgetType: W.Spacer }] },
            { widgetType: W.TextEdit, widgetId: "bebm_drawer", isReadOnly: true, html: "", minimumWidth: 370, minimumHeight: 380 }
        ] }].concat(bebDialogFooter(W, G, "bebm_", function () {
            var c = collect(this);
            if (c.errors.length || !c.jobs.length) { return; }
            var plan = bebMacroPlan(infos, c.jobs);
            if (!plan.changes) { return; }
            result = bebApplyMacros(plan);
            this.closeDialog();
        })),
        onConstructed: function () { update(this); }
    });
    return result;
}

function bebMacroWidget(W, def, value, edited, row) {
    var id = "bebm_v_" + def.prop;
    var base = { widgetId: id, row: row, column: 2, minimumWidth: 130 };
    var on = function () { edited(this, def); };
    if (def.kind === "int") {
        base.widgetType = W.SpinBox; base.range = { minimum: def.min, maximum: def.max };
        base.value = typeof value === "number" ? value : def.max; base.onValueChanged = on;
    } else if (def.kind === "enum") {
        var items = [];
        for (var i = 0; i < def.opts.length; i++) { items.push({ text: def.opts[i] }); }
        base.widgetType = W.ComboBox; base.items = items; base.currentIndex = typeof value === "number" ? value : 0;
        base.onCurrentIndexChanged = on;
    } else if (def.kind === "bool") {
        base.widgetType = W.CheckBox; base.text = "On"; base.isChecked = !!value; base.onToggled = on;
    } else {
        base.widgetType = W.LineEdit; base.text = typeof value === "number" ? bebNum(value) : ""; base.onTextEdited = on;
    }
    return base;
}

// → { value } or { error }
function bebMacroWidgetValue(w, def) {
    var x = bebW(w, "bebm_v_" + def.prop);
    if (!x) { return { error: def.label + ": widget missing" }; }
    try {
        if (def.kind === "int") { return { value: x.value() }; }
        if (def.kind === "enum") { return { value: x.currentIndex() }; }
        if (def.kind === "bool") { return { value: !!x.isChecked() }; }
        var t = bebTrim(x.text());
        var v = parseFloat(t);
        if (t === "" || isNaN(v) || ("" + Number(t)) === "NaN") { return { error: def.label + ": “" + t + "” is not a number" }; }
        if (v < def.min || v > def.max) { return { error: def.label + ": " + t + " is outside " + def.min + "–" + def.max }; }
        return { value: v };
    } catch (e) { return { error: def.label + ": " + e }; }
}

// ── Effects ────────────────────────────────────────────────────────────────
// Remove / add shared / replace with shared, on the top level of each track's
// effect chain. (No bypass: the tool is for standardising on shared effects,
// not for per-event switches.) Rules (verified in Studio):
//   - Removing a shared effect deletes the ProxyEffect only. NEVER deleteObject
//     an EffectPreset or preset.effect: that removes it project-wide.
//   - Effects inside a shared effect chain are never touched (that would edit
//     the shared chain for every event using it).
//   - addEffect() appends at the very end; relationships.effects.insert(i, fx)
//     then moves it (pre-fader = the fader's index).
//   - The fader is never removed or replaced.
var BEB_FX_ACTIONS = ["remove", "add", "replace"];
var BEB_FX_POSITIONS = [{ key: "prefader", text: "Before the fader" }, { key: "start", text: "Start of the chain" },
                        { key: "end", text: "End of the chain (after sends)" }];
var BEB_PROP_SKIP = { parent: 1, toString: 1, dump: 1, bypass: 1 };

// "S:{preset id}" for a shared effect, "P:identifier" for a plugin, "U:Entity" otherwise; null for the fader.
function bebFxKey(fx) {
    var ent = "" + bebGet(fx, "entity");
    if (ent === "MixerBusFader") { return null; }
    if (ent === "ProxyEffect") { return "S:" + bebGet(bebGet(fx, "preset"), "id"); }
    if (ent === "PluginEffect") { return "P:" + bebFxLabel("PluginEffect", fx); }
    return "U:" + ent;
}

function bebFxName(fx) {
    var ent = "" + bebGet(fx, "entity");
    if (ent === "ProxyEffect") {
        var pr = bebGet(fx, "preset");
        return bebFxRowText("shared", "" + bebGet(pr, "name"), bebFxLabel("" + bebGet(bebGet(pr, "effect"), "entity"), bebGet(pr, "effect")));
    }
    if (ent === "MixerBusFader") { return "Fader"; }
    return bebFxRowText("unique", bebFxLabel(ent, fx));
}

function bebFxChipName(fx) {
    var ent = "" + bebGet(fx, "entity");
    if (ent === "ProxyEffect") { return "" + bebGet(bebGet(fx, "preset"), "name"); }
    return ent === "MixerBusFader" ? "Fader" : bebFxLabel(ent, fx);
}

function bebFxIsSpatial(fx) {
    var ent = "" + bebGet(fx, "entity");
    if (ent === "ProxyEffect") { ent = "" + bebGet(bebGet(bebGet(fx, "preset"), "effect"), "entity"); }
    if (BEB_FX[ent] && BEB_FX[ent].spatial) { return true; }
    return ent === "PluginEffect" && bebFxLabel(ent, fx).toLowerCase().indexOf("source") >= 0;
}

// Chains in scope, live: [{ kind, name, chain }]. scope = { master, group, ret },
// or { track: "Kind|Name" } for one named track (Tracks… → Effects).
function bebTrackKey(t) { return t.kind + "|" + t.name; }
function bebScopeChains(ev, scope) {
    var out = [], tracks = bebTracksOf(ev);
    for (var i = 0; i < tracks.length; i++) {
        var t = tracks[i];
        if (scope.track ? bebTrackKey(t) === scope.track :
            (t.kind === "Master" && scope.master) || (t.kind === "Group" && scope.group) || (t.kind === "Return" && scope.ret)) {
            var ch = bebGet(t.bus, "effectChain");
            if (ch) { out.push({ kind: t.kind, name: t.name, chain: ch, label: t.kind === "Master" ? "Master" : t.kind + " “" + t.name + "”" }); }
        }
    }
    return out;
}

function bebChainFx(chain) { return bebList(bebGet(chain, "effects")); }

function bebIndexOfId(list, id) {
    for (var i = 0; i < list.length; i++) { if ("" + bebGet(list[i], "id") === "" + id) { return i; } }
    return -1;
}

function bebFaderIndex(chain) {
    var l = bebChainFx(chain);
    for (var i = 0; i < l.length; i++) { if ("" + bebGet(l[i], "entity") === "MixerBusFader") { return i; } }
    return -1;
}

// Everything removable in the target: [{ key, name, events, count, spatial }], most common first.
function bebFxCatalog(infos, scope) {
    var by = {}, order = [];
    for (var i = 0; i < infos.length; i++) {
        var chains = bebScopeChains(infos[i].ev, scope), seen = {};
        for (var c = 0; c < chains.length; c++) {
            var fxs = bebChainFx(chains[c].chain);
            for (var f = 0; f < fxs.length; f++) {
                var key = bebFxKey(fxs[f]);
                if (!key) { continue; }
                if (!by[key]) { by[key] = { key: key, name: bebFxName(fxs[f]), events: 0, count: 0, spatial: bebFxIsSpatial(fxs[f]) }; order.push(by[key]); }
                by[key].count++;
                if (!seen[key]) { seen[key] = true; by[key].events++; }
            }
        }
    }
    order.sort(function (a, b) { return b.events - a.events || (a.name < b.name ? -1 : 1); });
    return order;
}

function bebWalkPresets(items, out, depth, path) {
    var l = bebList(items);
    for (var i = 0; i < l.length; i++) {
        if (bebIs(l[i], "EffectPreset")) { out.push({ preset: l[i], name: "" + bebGet(l[i], "name"), folder: path }); }
        else if (depth < 12) { bebWalkPresets(bebGet(l[i], "items"), out, depth + 1, path + bebGet(l[i], "name") + "/"); }
    }
}

// The project's shared effects: [{ preset, name, folder, type, uses }], by name.
function bebSharedEffects() {
    var out = [];
    try { bebWalkPresets(bebGet(studio.project.workspace.masterEffectPresetFolder, "items"), out, 0, ""); } catch (e) {}
    for (var i = 0; i < out.length; i++) {
        var eff = bebGet(out[i].preset, "effect");
        out[i].type = bebFxLabel("" + bebGet(eff, "entity"), eff);
        out[i].uses = bebList(bebGet(out[i].preset, "proxies")).length;
    }
    out.sort(function (a, b) { return a.name.toLowerCase() < b.name.toLowerCase() ? -1 : 1; });
    return out;
}

// op = { action, key, preset, position, scope, skipExisting, bypassOn }
// → { rows: [{ info, chains: [{ label, chain, before: [fx], steps }] }], changes, skipped, warn: [] }
function bebFxPlan(infos, op) {
    var plan = { rows: [], changes: 0, skipped: 0, warn: [] };
    var spatialGone = [], automated = [], plugin = [], multi = [], already = [], missing = [];
    var presetKey = op.preset ? "S:" + bebGet(op.preset, "id") : null;
    var addScope = op.scope && op.scope.track ? op.scope : { master: true, group: false, ret: false };
    var addWhere = op.scope && op.scope.track ? "this track" : "the master track";
    var presetSpatial = false;
    if (op.preset) { var pe = bebGet(op.preset, "effect"), pent = "" + bebGet(pe, "entity"); presetSpatial = !!(BEB_FX[pent] && BEB_FX[pent].spatial); }
    for (var i = 0; i < infos.length; i++) {
        var info = infos[i], rowChains = [], n = 0;
        if (op.action === "add") {
            var mc = bebScopeChains(info.ev, addScope)[0];
            if (!mc) { missing.push(info.path); plan.skipped++; continue; }
            var mfx = bebChainFx(mc.chain), has = false;
            for (var a = 0; a < mfx.length; a++) { if (bebFxKey(mfx[a]) === presetKey) { has = true; } }
            if (has && op.skipExisting) { already.push(info.path); plan.skipped++; continue; }
            rowChains.push({ label: mc.label, chain: mc.chain, before: mfx, steps: [{ type: "add" }] });
            n++;
        } else {
            var chains = bebScopeChains(info.ev, op.scope), found = 0;
            for (var c = 0; c < chains.length; c++) {
                var fxs = bebChainFx(chains[c].chain), steps = [];
                for (var f = 0; f < fxs.length; f++) {
                    if (bebFxKey(fxs[f]) !== op.key) { continue; }
                    found++;
                    var driven = bebDriven(fxs[f]).length > 0;
                    if (driven) { automated.push(info.path); }
                    if (bebFxIsSpatial(fxs[f])) { spatialGone.push(info.path); }
                    if ("" + bebGet(fxs[f], "entity") === "PluginEffect") { plugin.push(info.path); }
                    steps.push({ type: op.action, fx: fxs[f], index: f, driven: driven });
                }
                if (steps.length) { rowChains.push({ label: chains[c].label, chain: chains[c].chain, before: fxs, steps: steps }); n += steps.length; }
            }
            if (found > 1) { multi.push(info.path + " (" + found + ")"); }
            if (!n) { plan.skipped++; continue; }
        }
        plan.changes += n;
        plan.rows.push({ info: info, chains: rowChains });
    }
    function warn(list, text) { if (list.length) { plan.warn.push({ text: list.length + " " + text, events: list }); } }
    if (op.action === "remove" || (op.action === "replace" && !presetSpatial)) {
        warn(spatialGone, "lose their spatializer: they become 2D unless something else spatializes them");
    }
    warn(automated, "have this effect automated or modulated: the automation is deleted with it, and Undo can't bring it back");
    warn(plugin, "have it as a plugin effect: Undo can't recreate plugin effects");
    warn(multi, "have it more than once: every instance in scope is affected");
    warn(already, "already have it on " + addWhere + ": skipped");
    warn(missing, "don't have " + addWhere + ": skipped");
    return plan;
}

// The chain as chips, with this plan's changes marked.
function bebFxChainPreview(ch, op) {
    var parts = [], stepAt = {};
    for (var s = 0; s < ch.steps.length; s++) { if (ch.steps[s].index !== undefined) { stepAt[ch.steps[s].index] = ch.steps[s]; } }
    var newChip = op.preset ? BEB_ADD + "&nbsp;+ [Shared] " + bebEsc(bebGet(op.preset, "name")) + "&nbsp;</span>" : "";
    var fader = bebFaderIndex(ch.chain);
    var insertAt = op.action !== "add" ? -1 : op.position === "start" ? 0 : op.position === "prefader" && fader >= 0 ? fader : ch.before.length;
    for (var i = 0; i <= ch.before.length; i++) {
        if (i === insertAt) { parts.push(newChip); }
        if (i === ch.before.length) { break; }
        var fx = ch.before[i], st = stepAt[i];
        var isFader = "" + bebGet(fx, "entity") === "MixerBusFader";
        var name = bebEsc(bebFxChipName(fx));
        if (!st) {
            var off = bebBypassed(fx);
            parts.push(isFader ? bebChip("Fader", BEB_C.fader, BEB_C.faderText)
                               : bebKey(fx).charAt(0) === "S" ? bebChipHtml(name + (off ? " ⊘" : ""), BEB_C.shared, BEB_C.sharedText)
                                                              : bebChipHtml(name + (off ? " ⊘" : ""), BEB_C.unique, BEB_C.uniqueText));
        } else if (st.type === "remove") {
            parts.push(BEB_DEL + "&nbsp;" + name + "&nbsp;</span>");
        } else {
            parts.push(BEB_DEL + "&nbsp;" + name + "&nbsp;</span>" + newChip);
        }
    }
    return bebGrey(bebEsc(ch.label) + ":") + " " + parts.join(" ");
}

function bebKey(fx) { return bebFxKey(fx) || ""; }
function bebChipHtml(html, bg, fg) { return "<span style=\"background-color:" + bg + "; color:" + fg + ";\">&nbsp;" + html + "&nbsp;</span>"; }

// Everything needed to put a removed effect back.
function bebFxSnapshot(fx, chain, index) {
    var ent = "" + bebGet(fx, "entity");
    var snap = { chain: chain, index: index, entity: ent, bypass: !!bebProp(fx, "bypass"), name: bebFxName(fx) };
    // A shared instance is put back as a new instance of the same shared effect. Its
    // settings are NOT copied: a ProxyEffect exposes the shared effect's own
    // properties, so writing them would change the shared effect for every event.
    if (ent === "ProxyEffect") { snap.preset = bebGet(fx, "preset"); return snap; }
    snap.props = {};
    try {
        for (var k in fx.properties) {
            if (BEB_PROP_SKIP[k] || k.indexOf("ui") === 0 || k.indexOf("derived") === 0) { continue; }
            var v = bebProp(fx, k);
            if (typeof v === "number" || typeof v === "boolean" || typeof v === "string") { snap.props[k] = v; }
        }
    } catch (e) {}
    return snap;
}

// Adds a shared effect (or re-creates a unique one) at a position. → { fx } or { error }
function bebFxInsert(chain, what, where) {
    var fx = null;
    try { fx = chain.addEffect(what); } catch (e) { return { error: "addEffect failed: " + e }; }
    if (!fx) { return { error: "addEffect returned nothing" }; }
    var id = "" + bebGet(fx, "id");
    var target = where === "prefader" ? bebFaderIndex(chain) : where === "start" ? 0 : typeof where === "number" ? where : -1;
    var len = bebChainFx(chain).length;
    if (target >= 0 && target < len - 1) {
        try { chain.relationships.effects.insert(target, fx); } catch (e2) { return { fx: fx, error: "added at the end; moving it failed: " + e2 }; }
        if (bebIndexOfId(bebChainFx(chain), id) !== target) { return { fx: fx, error: "added at the end; it didn't move to position " + (target + 1) }; }
    }
    return { fx: fx };
}

function bebFxDelete(chain, fx) {
    var id = "" + bebGet(fx, "id");
    try { studio.project.deleteObject(fx); } catch (e) { return "" + e; }
    return bebIndexOfId(bebChainFx(chain), id) < 0 ? null : "still there after delete";
}

function bebApplyFx(plan, op) {
    var items = [], ok = 0, bad = 0, undoOps = [];
    var pname = op.preset ? "[Shared] " + bebGet(op.preset, "name") : "";
    for (var r = 0; r < plan.rows.length; r++) {
        var row = plan.rows[r], parts = [], fails = [];
        for (var c = 0; c < row.chains.length; c++) {
            var ch = row.chains[c];
            // Work from the last index back, so earlier indices stay valid.
            var steps = ch.steps.slice().sort(function (a, b) { return (b.index || 0) - (a.index || 0); });
            for (var s = 0; s < steps.length; s++) {
                var st = steps[s], err = null;
                if (st.type === "add") {
                    var res = bebFxInsert(ch.chain, op.preset, op.position);
                    if (res.fx) { undoOps.push({ op: "added", chain: ch.chain, id: "" + bebGet(res.fx, "id"), name: pname, path: row.info.path }); }
                    err = res.error;
                    if (!err) { parts.push("+ " + pname + " on " + ch.label); }
                } else {
                    var snap = bebFxSnapshot(st.fx, ch.chain, st.index);
                    var nm = bebFxName(st.fx);
                    err = bebFxDelete(ch.chain, st.fx);
                    if (!err) {
                        undoOps.push({ op: "removed", snap: snap, driven: st.driven, path: row.info.path });
                        if (st.type === "replace") {
                            var rr = bebFxInsert(ch.chain, op.preset, st.index);
                            if (rr.fx) { undoOps.push({ op: "added", chain: ch.chain, id: "" + bebGet(rr.fx, "id"), name: pname, path: row.info.path }); }
                            err = rr.error;
                            if (!err) { parts.push("~ " + nm + " → " + pname + " on " + ch.label); }
                        } else {
                            parts.push("- " + nm + " from " + ch.label);
                        }
                    }
                }
                if (err) { bad++; fails.push(err); } else { ok++; }
            }
        }
        if (parts.length) { items.push("~ " + row.info.path + "  " + parts.join(", ")); }
        if (fails.length) { items.push("! " + row.info.path + "  " + fails.join("; ")); }
    }
    var what = op.action === "add" ? "Added " + pname + " to" : op.action === "remove" ? "Removed it from" : "Replaced it with " + pname + " on";
    var summary = [(bad ? "! " : "+ ") + "Effects: " + what + " " + bebPlural(plan.rows.length, "event") + " (" + bebPlural(ok, "change") + ")" +
                   (plan.skipped ? " · " + plan.skipped + " skipped" : "") + (bad ? " · " + bad + " failed" : "")];
    for (var x = 0; x < plan.warn.length; x++) { summary.push("! " + plan.warn[x].text); }
    var label = op.action === "add" ? "Effects: added " + pname + " to " + bebPlural(plan.rows.length, "event")
              : "Effects: " + op.action + " on " + bebPlural(plan.rows.length, "event");
    return { ok: ok, bad: bad, lines: bebResultLines(summary, items), undo: { kind: "fx", ops: undoOps, label: label } };
}

// Undo an effects batch, newest operation first.
function bebUndoFx(rec, items) {
    var n = { ok: 0, skipped: 0, bad: 0 };
    for (var i = rec.ops.length - 1; i >= 0; i--) {
        var o = rec.ops[i], err = null;
        if (o.op === "added") {
            var l = bebChainFx(o.chain), at = bebIndexOfId(l, o.id);
            if (at < 0) { n.skipped++; items.push("= " + o.path + "  " + o.name + " already gone"); continue; }
            err = bebFxDelete(o.chain, l[at]);
            if (!err) { items.push("- " + o.path + "  " + o.name); }
        } else if (o.op === "autoRemoved") {
            var ra = bebRestoreAutomation(o);
            if (ra.skip) { n.skipped++; items.push("= " + o.path + "  " + o.name + " " + ra.skip); continue; }
            err = ra.error || null;
            if (!err) { items.push("+ " + o.path + "  " + o.name + " " + ra.text); }
        } else if (o.op === "value") {
            if (!bebSame(bebProp(o.obj, o.prop), o.after)) { n.skipped++; items.push("= " + o.path + "  " + o.name + " changed since: left as it is"); continue; }
            var w = bebWrite(o.obj, o.prop, o.before);
            err = w.ok ? null : w.error;
            if (!err) { items.push("~ " + o.path + "  " + o.name + (o.what ? " " + o.what : o.before ? " bypassed again" : " active again")); }
        } else {
            var sn = o.snap;
            if (sn.entity === "PluginEffect") { n.bad++; items.push("! " + o.path + "  " + sn.name + ": plugin effects can't be re-created by a script"); continue; }
            var res = bebFxInsert(sn.chain, sn.preset || sn.entity, sn.index);
            err = res.error || null;
            if (res.fx) {
                var miss = 0;
                if (sn.props) { for (var k in sn.props) { if (sn.props.hasOwnProperty(k) && !bebWrite(res.fx, k, sn.props[k]).ok) { miss++; } } }
                if (sn.bypass) { bebWrite(res.fx, "bypass", true); }
                items.push("+ " + o.path + "  " + sn.name + " put back" + (o.driven ? " (its automation is gone)" : "") + (miss ? " (" + miss + " settings didn't restore)" : ""));
            }
        }
        if (err) { n.bad++; items.push("! " + o.path + "  " + err); } else { n.ok++; }
    }
    return n;
}

// fixedTrack (optional, from Tracks…): { key: "Kind|Name", label } limits every
// action, Add included, to that one track.
function bebFxDialog(infos, fixedTrack) {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    var result = null, catalog = [], shared = bebSharedEffects(), shown = [], folders = [""];
    for (var i = 0; i < shared.length; i++) { if (folders.indexOf(shared[i].folder) < 0) { folders.push(shared[i].folder); } }
    folders.sort();
    var folderItems = [{ text: "All folders  ·  " + shared.length }];
    for (var fi = 1; fi < folders.length; fi++) {
        var nIn = 0;
        for (var k = 0; k < shared.length; k++) { if (shared[k].folder === folders[fi]) { nIn++; } }
        folderItems.push({ text: (folders[fi] || "(top level)") + "  ·  " + nIn });
    }

    // The shared-effect list, narrowed by the Find text and Folder; keeps the
    // current choice when it still matches. Sorted by folder, then name.
    function rebuildPresets(w) {
        var prev = shown[bebWIndex(w, "bebf_preset") - 1];
        var find = bebTrim(bebWText(w, "bebf_find")).toLowerCase(), folder = folders[bebWIndex(w, "bebf_folder")];
        shown = [];
        for (var i = 0; i < shared.length; i++) {
            var e = shared[i];
            if (folder && e.folder !== folder) { continue; }
            if (find && (e.name + " " + e.type + " " + e.folder).toLowerCase().indexOf(find) < 0) { continue; }
            shown.push(e);
        }
        shown.sort(function (a, b) { return a.folder === b.folder ? (a.name.toLowerCase() < b.name.toLowerCase() ? -1 : 1) : (a.folder < b.folder ? -1 : 1); });
        var items = [{ text: !shared.length ? "(the project has no shared effects)" : shown.length ? "(choose a shared effect · " + shown.length + ")" : "(nothing matches)" }], idx = 0;
        for (var j = 0; j < shown.length; j++) {
            items.push({ text: bebFxRowText("shared", shown[j].name, shown[j].type) + "  ·  used by " + bebPlural(shown[j].uses, "event") });
            if (prev && shown[j] === prev) { idx = j + 1; }
        }
        if (!idx && shown.length === 1) { idx = 1; }                  // one match: pick it
        var cb = bebW(w, "bebf_preset");
        if (cb) { try { cb.setItems(items); cb.setCurrentIndex(idx); } catch (e) {} }
    }

    function read(w) {
        var scope = fixedTrack ? { track: fixedTrack.key }
                  : { master: bebWChecked(w, "bebf_master"), group: bebWChecked(w, "bebf_group"), ret: bebWChecked(w, "bebf_return") };
        var ci = bebWIndex(w, "bebf_fx"), pi = bebWIndex(w, "bebf_preset");
        return { action: BEB_FX_ACTIONS[bebWIndex(w, "bebf_action")], scope: scope,
                 key: ci > 0 && catalog[ci - 1] ? catalog[ci - 1].key : null,
                 entry: pi > 0 && shown[pi - 1] ? shown[pi - 1] : null,
                 preset: pi > 0 && shown[pi - 1] ? shown[pi - 1].preset : null,
                 position: BEB_FX_POSITIONS[bebWIndex(w, "bebf_position")].key,
                 skipExisting: bebWChecked(w, "bebf_skip") };
    }

    function rebuildCatalog(w) {
        var o = read(w);
        var prevKey = o.key;
        catalog = bebFxCatalog(infos, o.scope);
        var items = [{ text: catalog.length ? "(choose an effect on these events)" : "(no effects in the chosen tracks)" }], idx = 0;
        for (var c = 0; c < catalog.length; c++) {
            items.push({ text: catalog[c].name + "  ·  " + bebUsageText(catalog[c].events, infos.length, catalog[c].count) });
            if (catalog[c].key === prevKey) { idx = c + 1; }
        }
        var cb = bebW(w, "bebf_fx");
        if (cb) { try { cb.setItems(items); cb.setCurrentIndex(idx); } catch (e) {} }
    }

    function enable(w, id, on) { var x = bebW(w, id); if (x) { try { x.setEnabled(on); } catch (e) {} } }

    // The deck shows the shared effect being added, or the shared one chosen to remove/replace.
    var lastDeck = null;
    function updateDeck(w, o) {
        var sig = o.action + "|" + (o.key || "") + "|" + (o.entry ? bebGet(o.entry.preset, "id") : "") + "|" +
                  o.scope.master + o.scope.group + o.scope.ret;
        if (sig === lastDeck) { return; }
        lastDeck = sig;
        var d = bebW(w, "bebf_deck");
        if (d) { try { d.setHtml(bebDeckDoc(o, infos, shared)); } catch (e) {} }
    }

    // Labels say what each list is for in the current action.
    function setLabel(w, id, text) { var x = bebW(w, id); if (x) { try { x.setText(text); } catch (e) {} } }

    function update(w) {
        var o = read(w);
        var needFx = o.action !== "add", needPreset = o.action !== "remove";
        enable(w, "bebf_fx", needFx);
        if (!fixedTrack) { enable(w, "bebf_master", needFx); enable(w, "bebf_group", needFx); enable(w, "bebf_return", needFx); }
        enable(w, "bebf_preset", needPreset); enable(w, "bebf_find", needPreset); enable(w, "bebf_folder", needPreset);
        enable(w, "bebf_position", o.action === "add"); enable(w, "bebf_skip", o.action === "add");
        setLabel(w, "bebf_fxLabel", o.action === "replace" ? "Effect to replace" : "Effect to remove");
        setLabel(w, "bebf_presetHead", "<b>" + (o.action === "replace" ? "Replace it with this shared effect" : "Shared effect to add") + "</b>");
        setLabel(w, "bebf_presetLabel", o.action === "replace" ? "Replacement" : "Effect to add");
        updateDeck(w, o);
        var ask = needFx && !o.key ? "Choose the effect to " + o.action + "."
                : needPreset && !o.preset ? "Choose the shared effect to " + (o.action === "add" ? "add." : "put in its place.") : "";
        if (o.action === "replace" && o.key && o.preset && o.key === "S:" + bebGet(o.preset, "id")) { ask = "That's the same shared effect."; }
        if (ask) { bebSetPreview(w, "bebf_", bebGrey(ask), "Apply", false); return; }
        var plan = bebFxPlan(infos, o);
        var h = "<p><b>" + bebPlural(plan.changes, "change") + " on " + bebPlural(plan.rows.length, "event") + "</b>" +
                (plan.skipped ? bebGrey(" · " + plan.skipped + " unchanged") : "") + "</p>";
        if (o.action !== "remove") {
            h += "<p>" + bebGrey("Each event gets its own instance of the shared effect. Editing the shared effect later changes all of them.") + "</p>";
        } else if (o.key && o.key.charAt(0) === "S") {
            h += "<p>" + bebGrey("Only these events' instances are removed. The shared effect stays in the project.") + "</p>";
        }
        for (var x = 0; x < plan.warn.length; x++) {
            var wn = plan.warn[x];
            h += "<p><span style=\"color:#ff9a6a;\">" + bebEsc(wn.text) + "</span><br/>" +
                 bebGrey(bebEsc(wn.events.slice(0, 4).join(", ")) + (wn.events.length > 4 ? " …" : "")) + "</p>";
        }
        for (var r = 0; r < plan.rows.length && r < 40; r++) {
            var lines = [];
            for (var c = 0; c < plan.rows[r].chains.length; c++) { lines.push(bebFxChainPreview(plan.rows[r].chains[c], o)); }
            h += bebCard(plan.rows[r].info.path, "", lines.join("<br/>"), BEB_C.event);
        }
        if (plan.rows.length > 40) { h += bebGrey("… " + (plan.rows.length - 40) + " more"); }
        var verb = o.action === "add" ? "Add to " : o.action === "remove" ? "Remove from " : "Replace on ";
        bebSetPreview(w, "bebf_", h, plan.rows.length ? verb + bebPlural(plan.rows.length, "event") : "Nothing to change", plan.rows.length > 0);
    }

    function scopeChanged(w) { rebuildCatalog(w); update(w); }
    function filterChanged(w) { rebuildPresets(w); update(w); }

    var controls = { widgetType: W.Layout, layout: G.GridLayout, spacing: 6, items: [
        { widgetType: W.Label, text: "Action", row: 0, column: 0 },
        { widgetType: W.ComboBox, widgetId: "bebf_action", row: 0, column: 1, currentIndex: 0, minimumWidth: 360,
          items: [{ text: "Remove an effect" }, { text: "Add a shared effect" }, { text: "Replace an effect with a shared effect" }],
          onCurrentIndexChanged: function () { update(this); } },
        { widgetType: W.Label, text: fixedTrack ? "Track" : "Tracks", row: 1, column: 0 },
        fixedTrack ? { widgetType: W.Label, row: 1, column: 1, text: "<b>" + bebEsc(fixedTrack.label) + "</b> <span style=\"color:#9a9a9a;\">only</span>" } :
        { widgetType: W.Layout, layout: G.HBoxLayout, row: 1, column: 1, spacing: 12, items: [
            { widgetType: W.CheckBox, widgetId: "bebf_master", text: "Master", isChecked: true, onToggled: function () { scopeChanged(this); } },
            { widgetType: W.CheckBox, widgetId: "bebf_group", text: "Group tracks", isChecked: false, onToggled: function () { scopeChanged(this); } },
            { widgetType: W.CheckBox, widgetId: "bebf_return", text: "Return tracks", isChecked: false, onToggled: function () { scopeChanged(this); } },
            { widgetType: W.Spacer }
        ] },
        { widgetType: W.Label, widgetId: "bebf_fxLabel", text: "Effect to remove", row: 2, column: 0 },
        { widgetType: W.ComboBox, widgetId: "bebf_fx", row: 2, column: 1, currentIndex: 0, items: [{ text: "" }],
          onCurrentIndexChanged: function () { update(this); } },
        { widgetType: W.Label, widgetId: "bebf_presetHead", text: "<b>Shared effect to add</b>", row: 3, column: 0, columnSpan: 2 },
        { widgetType: W.Label, text: "Find", row: 4, column: 0 },
        { widgetType: W.LineEdit, widgetId: "bebf_find", row: 4, column: 1, text: "", onTextEdited: function () { filterChanged(this); } },
        { widgetType: W.Label, text: "Folder", row: 5, column: 0 },
        { widgetType: W.ComboBox, widgetId: "bebf_folder", row: 5, column: 1, currentIndex: 0, items: folderItems,
          onCurrentIndexChanged: function () { filterChanged(this); } },
        { widgetType: W.Label, widgetId: "bebf_presetLabel", text: "Effect to add", row: 6, column: 0 },
        { widgetType: W.ComboBox, widgetId: "bebf_preset", row: 6, column: 1, currentIndex: 0, items: [{ text: "" }],
          onCurrentIndexChanged: function () { update(this); } },
        { widgetType: W.Label, text: "Position", row: 7, column: 0 },
        { widgetType: W.ComboBox, widgetId: "bebf_position", row: 7, column: 1, currentIndex: 0,
          items: [{ text: BEB_FX_POSITIONS[0].text }, { text: BEB_FX_POSITIONS[1].text }, { text: BEB_FX_POSITIONS[2].text }],
          onCurrentIndexChanged: function () { update(this); } },
        { widgetType: W.CheckBox, widgetId: "bebf_skip", row: 8, column: 1, isChecked: true,
          text: fixedTrack ? "Skip events that already have it on this track" : "Skip events that already have it on the master track",
          onToggled: function () { update(this); } },
        { widgetType: W.Label, wordWrap: true, row: 9, column: 0, columnSpan: 2, maximumWidth: 440,
          text: "<span style=\"color:#9a9a9a;\">Tip: with the list closed, the arrow keys step through it and the preview follows. " +
                (fixedTrack ? "Every action, Add included, acts on this track only. " : "Adding always goes to the master track. ") +
                "Effects inside a shared effect chain are never changed here.</span>" }
    ] };

    studio.ui.showModalDialog({
        windowTitle: "Effects — " + (fixedTrack ? fixedTrack.label + " · " : "") + bebPlural(infos.length, "event"),
        windowWidth: 1080, windowHeight: 820,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 12, items: [
                { widgetType: W.Layout, layout: G.VBoxLayout, items: [controls, { widgetType: W.Spacer }] },
                { widgetType: W.TextEdit, widgetId: "bebf_deck", isReadOnly: true, html: "", minimumWidth: 470, minimumHeight: 330 }
            ] }
        ].concat(bebDialogFooter(W, G, "bebf_", function () {
            var o = read(this);
            var plan = bebFxPlan(infos, o);
            if (!plan.rows.length) { return; }
            result = bebApplyFx(plan, o);
            this.closeDialog();
        })),
        onConstructed: function () { rebuildCatalog(this); rebuildPresets(this); lastDeck = null; update(this); }
    });
    return result;
}

// ── Images (knobs, attenuation curves) ─────────────────────────────────────
// Ported from Copy-Paste Conditions (ccpBmp / ccpXpm / ccpWriteBytes), where it
// works in Studio: pixels are drawn in JS and written to <project>/.user/ as a
// BMP via File.writeBinary (the argument form that works is found by checking
// the file size), else as XPM text via writeText, else no images (text only).
// A "@2x" copy is written for high-DPI screens. Files are named beb_img_* and
// cleared when the main window opens.
var BEB_IMG = { ok: true, mode: null, form: "", why: "", written: {} };
var BEB_BIN_FORMS = ["ArrayBuffer", "Uint8Array", "Array"];

function bebUserDir() {
    var p = "";
    try { p = "" + (studio.project.filePath || ""); } catch (e) { p = ""; }
    var cut = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return cut < 0 ? "" : p.substring(0, cut + 1) + ".user/";
}

function bebHash(str) {
    var h = 5381;
    for (var i = 0; i < str.length; i++) { h = ((h * 33) ^ str.charCodeAt(i)) >>> 0; }
    return h.toString(16);
}

function bebRgb(hex) { return [parseInt(hex.substr(1, 2), 16), parseInt(hex.substr(3, 2), 16), parseInt(hex.substr(5, 2), 16)]; }

function bebBmp(w, h, rgb) {
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

function bebXpm(w, h, rgb) {
    var A = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-";
    for (var q = 1; q <= 64; q *= 2) {
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
            return "/* XPM */\nstatic char *img[] = {\n\"" + w + " " + h + " " + colours.length + " 2\",\n" +
                   colours.join("\n") + "\n" + rows.join("\n") + "\n};\n";
        }
    }
    return "";
}

function bebFileSize(path) {
    try { var f = studio.system.getFile(path); return f.exists() ? f.size() : -1; } catch (e) { return -1; }
}

function bebWriteBytes(path, bytes, onlyForm) {
    var forms = onlyForm ? [onlyForm] : BEB_BIN_FORMS, why = [];
    for (var i = 0; i < forms.length; i++) {
        var fm = forms[i], ret;
        var arg = fm === "ArrayBuffer" ? bytes.buffer : fm === "Uint8Array" ? bytes : Array.prototype.slice.call(bytes);
        try {
            var f = studio.system.getFile(path);
            if (!f.open(studio.system.openMode.WriteOnly)) { why.push(fm + ": couldn't open"); continue; }
            ret = f.writeBinary(arg);
            f.close();
        } catch (e) { why.push(fm + ": " + e); continue; }
        if (bebFileSize(path) === bytes.length) { return { form: fm }; }
        why.push(fm + ": returned " + ret);
    }
    return { form: "", why: why.join("; ") };
}

function bebWriteText(path, text) {
    try {
        var f = studio.system.getFile(path);
        if (!f.open(studio.system.openMode.WriteOnly)) { return false; }
        f.writeText(text);
        f.close();
    } catch (e) { return false; }
    return bebFileSize(path) === text.length;
}

function bebWriteImage(path, w, h, rgb) {
    if (BEB_IMG.mode !== "xpm") {
        var r = bebWriteBytes(path + ".bmp", bebBmp(w, h, rgb), BEB_IMG.form);
        if (r.form) { BEB_IMG.mode = "bmp"; BEB_IMG.form = r.form; return ".bmp"; }
        BEB_IMG.why = r.why;
        try { studio.system.getFile(path + ".bmp").remove(); } catch (e) {}
    }
    if (bebWriteText(path + ".xpm", bebXpm(w, h, rgb))) { BEB_IMG.mode = "xpm"; return ".xpm"; }
    try { studio.system.print("[" + BEB_TITLE + "] Images unavailable, showing values as text. writeBinary: " + BEB_IMG.why + "\n"); } catch (e2) {}
    BEB_IMG.ok = false;
    return "";
}

function bebClearImages() {
    BEB_IMG.written = {};
    BEB_IMG.ok = true;
    var dir = bebUserDir();
    if (!dir) { return; }
    var names = [];
    try { names = studio.system.readDir(dir, studio.system.readDirFilters.Files) || []; } catch (e) { names = []; }
    for (var i = 0; i < names.length; i++) {
        var n = "" + names[i];
        if (n.indexOf("beb_img_") === 0) { try { studio.system.getFile(dir + n).remove(); } catch (e2) {} }
    }
}

// An <img> drawn by draw(w, h, scale) → rgb, cached by key; null when images don't work.
function bebImage(key, w, h, draw) {
    if (!BEB_IMG.ok) { return null; }
    var dir = bebUserDir();
    if (!dir) { return null; }
    var base = dir + "beb_img_" + bebHash(key);
    if (!BEB_IMG.written[key]) {
        var ext = "";
        for (var s = 1; s <= 2; s++) {
            ext = bebWriteImage(base + (s === 2 ? "@2x" : ""), w * s, h * s, draw(w * s, h * s, s));
            if (!ext) { return null; }
        }
        BEB_IMG.written[key] = ext;
    }
    return "<img src=\"" + base + BEB_IMG.written[key] + "\" width=\"" + w + "\" height=\"" + h + "\" />";
}

// Pixel helpers for the drawings below.
function bebCanvas(w, h, bgHex) {
    var bg = bebRgb(bgHex), rgb = new Uint8Array(w * h * 3);
    for (var i = 0; i < w * h; i++) { rgb[i * 3] = bg[0]; rgb[i * 3 + 1] = bg[1]; rgb[i * 3 + 2] = bg[2]; }
    return { w: w, h: h, rgb: rgb };
}
function bebPut(cv, x, y, c, a) {
    if (x < 0 || y < 0 || x >= cv.w || y >= cv.h || a <= 0) { return; }
    var i = (y * cv.w + x) * 3;
    for (var k = 0; k < 3; k++) { cv.rgb[i + k] = Math.round(cv.rgb[i + k] + (c[k] - cv.rgb[i + k]) * Math.min(1, a)); }
}

// A Studio-style knob: dark disc, grey track over 270°, coloured value arc,
// white pointer. frac 0..1; bipolar arcs start from the middle (e.g. 0 dB).
var BEB_KNOB = 38;
function bebDrawKnob(frac, bipolar, colorHex) {
    return function (w, h, s) {
        var cv = bebCanvas(w, h, "#2b2b2b");
        var cx = w / 2, cy = h / 2, rOut = w / 2 - 1.5 * s, rIn = rOut - 3.2 * s, rDisc = rIn - 2 * s;
        var disc = bebRgb("#4a4a4a"), rim = bebRgb("#5c5c5c"), track = bebRgb("#3a3a3a"), col = bebRgb(colorHex), white = [235, 235, 235];
        var a0 = 0.75 * Math.PI, span = 1.5 * Math.PI;            // from 7:30 clockwise to 4:30
        var av = a0 + span * frac, aFrom = bipolar ? a0 + span / 2 : a0;
        var lo = Math.min(aFrom, av), hi = Math.max(aFrom, av);
        for (var y = 0; y < h; y++) {
            for (var x = 0; x < w; x++) {
                var dx = x + 0.5 - cx, dy = y + 0.5 - cy, d = Math.sqrt(dx * dx + dy * dy);
                var ang = Math.atan2(dy, dx); if (ang < 0) { ang += 2 * Math.PI; } if (ang < a0 - 0.0001) { ang += 2 * Math.PI; }
                if (d <= rDisc + 0.5) { bebPut(cv, x, y, d > rDisc - 1.2 * s ? rim : disc, Math.min(1, rDisc + 0.5 - d)); }
                if (d >= rIn - 0.5 && d <= rOut + 0.5 && ang <= a0 + span) {
                    var cover = Math.min(1, d - rIn + 0.5, rOut + 0.5 - d);
                    bebPut(cv, x, y, ang >= lo && ang <= hi ? col : track, cover);
                }
            }
        }
        for (var t = 0; t <= 1; t += 0.02 / s) {                  // pointer
            var px = cx + Math.cos(av) * (rDisc * (0.25 + 0.7 * t)), py = cy + Math.sin(av) * (rDisc * (0.25 + 0.7 * t));
            for (var oy = -1; oy <= 1; oy++) { for (var ox = -1; ox <= 1; ox++) {
                var dd = Math.sqrt(ox * ox + oy * oy);
                bebPut(cv, Math.floor(px) + ox, Math.floor(py) + oy, white, 1.1 * s * 0.7 - dd * 0.6);
            } }
        }
        return cv.rgb;
    };
}

// Distance attenuation, like the spatializer's own graph: level (0..1) against
// distance, with dotted lines at min and max. Shapes per FMOD's rolloff docs.
function bebRolloffAt(type, d, mn, mx) {
    if (d <= mn) { return 1; }
    if (d >= mx) { return type === 2 ? mn / mx : 0; }
    var t = (d - mn) / Math.max(1e-6, mx - mn);
    if (type === 1) { return 1 - t; }
    if (type === 2) { return mn / d; }
    if (type === 3) { return (mn / d) * (1 - t); }
    if (type === 4) { return 1 - t; }
    return (1 - t) * (1 - t);
}
function bebDrawRolloff(type, mn, mx) {
    return function (w, h, s) {
        var cv = bebCanvas(w, h, "#1c1c1c");
        var line = bebRgb("#ff3d6e"), grid = bebRgb("#3a3a3a"), fill = [70, 32, 44];
        var pad = 4 * s, pw = w - 2 * pad, ph = h - 2 * pad, top = mx * 1.15;
        function X(d) { return pad + d / top * pw; }
        function Y(v) { return pad + (1 - v) * ph; }
        var gx = [Math.round(X(mn)), Math.round(X(mx))];
        for (var g = 0; g < 2; g++) { for (var yy = 0; yy < h; yy += 3 * s) { bebPut(cv, gx[g], yy, grid, 1); bebPut(cv, gx[g], yy + 1, grid, 1); } }
        for (var px = 0; px < w; px++) {
            var ya = Y(bebRolloffAt(type, (px - pad) / pw * top, mn, mx)), yb = Y(bebRolloffAt(type, (px + 1 - pad) / pw * top, mn, mx));
            for (var py = Math.floor(Math.min(ya, yb)); py < h - pad; py++) { bebPut(cv, px, py, fill, 1); }
            var lo = Math.min(ya, yb) - 0.9 * s, hi = Math.max(ya, yb) + 0.9 * s;
            for (var qy = Math.max(0, Math.floor(lo)); qy <= Math.min(h - 1, Math.ceil(hi)); qy++) {
                bebPut(cv, px, qy, line, Math.min(qy + 1, hi) - Math.max(qy, lo));
            }
        }
        return cv.rgb;
    };
}

// ── Shared effect deck (preview of a shared effect, like Studio's deck) ────
// Knob range per unit (only to place the pointer; values print exactly).
function bebKnobRange(spec, v) {
    if (spec.unit === "dB") { return { lo: -80, hi: 10, bip: false, log: false }; }
    if (spec.unit === "Hz") { return { lo: 20, hi: 22000, log: true }; }
    if (spec.unit === "ms") { return { lo: 0, hi: Math.max(1000, v), log: false }; }
    if (spec.unit === "%") { return { lo: 0, hi: Math.max(100, v), log: false }; }
    if (spec.name === "minimumExtent" || spec.name === "userPanExtent") { return { lo: 0, hi: 360 }; }
    if (spec.name === "userPanDirection" || spec.name === "userStereoAxis") { return { lo: -180, hi: 180, bip: true }; }
    if (spec.name === "panBlend") { return { lo: 0, hi: 1 }; }
    return { lo: Math.min(0, v), hi: Math.max(1, Math.abs(v) * 2), log: false };
}

function bebKnobHtml(spec, v, driven) {
    var r = bebKnobRange(spec, v);
    var frac = r.log ? Math.log(Math.max(r.lo, v) / r.lo) / Math.log(r.hi / r.lo) : (v - r.lo) / (r.hi - r.lo);
    if (spec.name === "panBlend") { frac = 1 - v; }
    frac = Math.max(0, Math.min(1, frac));
    var q = Math.round(frac * 60) / 60;
    var color = driven ? "#e07be0" : "#ffcf3f";
    var img = bebImage("knob|" + q + "|" + !!r.bip + "|" + color, BEB_KNOB, BEB_KNOB, bebDrawKnob(q, !!r.bip, color));
    var val = bebEsc(bebFmt(spec, v));
    return "<td align=\"center\" valign=\"top\" width=\"74\" style=\"color:#cfcfcf; font-size:10px;\">" + bebEsc(bebPropLabel(spec.name)) +
           (driven ? "<br/>" + bebChip("auto", BEB_C.auto, BEB_C.autoText) : "") + "<br/>" +
           (img || "") + "<br/><span style=\"color:#ffffff; font-size:11px;\">" + val + "</span></td>";
}

function bebPillHtml(spec, v) {
    var on = v === true;
    var bg = typeof v === "boolean" ? (on ? "#c9a227" : "#3a3a3a") : "#3a3a3a", fg = on ? "#111111" : "#e0e0e0";
    return "<td align=\"center\" valign=\"top\" width=\"74\" style=\"color:#cfcfcf; font-size:10px;\">" + bebEsc(bebPropLabel(spec.name)) + "<br/><br/>" +
           "<span style=\"background-color:" + bg + "; color:" + fg + ";\">&nbsp;" + bebEsc(bebFmt(spec, v)) + "&nbsp;</span></td>";
}

// One effect as a deck panel: title bar, then knobs/pills in rows of 5.
function bebDeckPanel(eff, title) {
    var ent = "" + bebGet(eff, "entity");
    var specs = bebFxSpecs(ent), drivenProps = {}, dv = bebDriven(eff);
    for (var d = 0; d < dv.length; d++) { drivenProps[dv[d].prop] = true; }
    var cells = [], extra = "";
    var isSpatial = BEB_FX[ent] && BEB_FX[ent].spatial;
    if (isSpatial) {
        var mn = bebProp(eff, "minimumDistance"), mx = bebProp(eff, "maximumDistance"), ro = bebProp(eff, "distanceRolloffType");
        var ov = !!bebProp(eff, "overrideRange");
        if (typeof mn === "number" && typeof mx === "number") {
            var img = bebImage("rolloff|" + ro + "|" + mn + "|" + mx, 300, 70, bebDrawRolloff(ro, mn, mx));
            extra = "<tr><td colspan=\"5\" style=\"color:#cfcfcf; font-size:10px;\">Distance attenuation · " + bebEsc(BEB_ROLLOFF[ro] || ("#" + ro)) +
                    " · " + (ov ? "override <b>" + bebNum(mn) + " – " + bebNum(mx) + "</b>" : bebGrey("uses the event's min/max (" + bebNum(mn) + " – " + bebNum(mx) + " shown)")) +
                    "<br/>" + (img || "") + "</td></tr>";
        }
    }
    // At 0% 2D pan mix the panner settings do nothing, so (like Studio) they're hidden.
    var PANNER = { userPanDirection: 1, userPanExtent: 1, userLFELevel: 1, LFEUpmixEnabled: 1, stereoToSurroundUserPanMode: 1,
                   userStereoSeparation: 1, userStereoAxis: 1 };
    var no2D = bebSame(bebProp(eff, "panBlend"), 1);
    // In the data but not on Studio's Spatializer panel: listed in grey instead of drawn.
    var HIDDEN = { occlusionEnabled: 1, dopplerMultiplier: 1 }, hidden = [];
    for (var i = 0; i < specs.length; i++) {
        var sp = specs[i];
        if (no2D && PANNER[sp.name]) { continue; }
        if (isSpatial && HIDDEN[sp.name]) {
            var hv = bebProp(eff, sp.name);
            if (hv !== undefined) { hidden.push(bebPropLabel(sp.name) + " " + bebFmt(sp, hv)); }
            continue;
        }
        if (isSpatial && (sp.name === "minimumDistance" || sp.name === "maximumDistance" || sp.name === "distanceRolloffType" || sp.name === "overrideRange")) { continue; }
        var v = bebProp(eff, sp.name);
        if (v === undefined) { continue; }
        if (sp.name.indexOf("filterType") === 0 && v === 0) { i += 3; continue; }          // multiband band off
        cells.push(typeof v === "number" && !sp.raw && sp.name !== "extentMode" && sp.name !== "distanceRolloffType"
                   ? bebKnobHtml(sp, v, drivenProps[sp.name]) : bebPillHtml(sp, v));
    }
    var rows = "";
    for (var c = 0; c < cells.length; c += 5) { rows += "<tr>" + cells.slice(c, c + 5).join("") + "</tr>"; }
    var bypass = !!bebProp(eff, "bypass");
    return "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"4\" bgcolor=\"#2b2b2b\" style=\"margin-bottom:6px;\">" +
           "<tr><td bgcolor=\"#3d3d3d\" style=\"color:#ffffff;\"><b>" + bebEsc(title) + "</b>" + (bypass ? " " + bebWarn("⊘ bypassed") : "") + "</td></tr>" +
           "<tr><td><table cellspacing=\"2\" cellpadding=\"2\">" + extra + rows + "</table>" +
           (cells.length || extra ? "" : bebGrey("No settings to show.")) +
           (hidden.length ? "<span style=\"color:#8a8a8a; font-size:10px;\">Not on Studio's panel: " + bebEsc(hidden.join(" · ")) + "</span>" : "") +
           "</td></tr></table>";
}

// A heading strip: red for what goes, green for what comes in.
function bebDeckBanner(kind, text) {
    var bg = kind === "remove" ? "#4a2626" : "#24452a", fg = kind === "remove" ? "#ffb3b3" : "#b8f0c0";
    return "<table width=\"100%\" cellpadding=\"5\" cellspacing=\"0\" bgcolor=\"" + bg + "\"><tr><td style=\"color:" + fg + "; font-weight:bold;\">" +
           (kind === "remove" ? "− " : "+ ") + bebEsc(text) + "</td></tr></table>";
}

// How many of the target events have this key on any track.
function bebKeyCount(infos, key) {
    var n = 0;
    for (var i = 0; i < infos.length; i++) {
        var chains = bebScopeChains(infos[i].ev, { master: true, group: true, ret: true }), has = false;
        for (var c = 0; c < chains.length && !has; c++) {
            var l = bebChainFx(chains[c].chain);
            for (var f = 0; f < l.length; f++) { if (bebFxKey(l[f]) === key) { has = true; } }
        }
        if (has) { n++; }
    }
    return n;
}

// A shared effect: header (name, type, folder, usage) and one panel per effect in it.
function bebDeckShared(entry, infos) {
    var eff = bebGet(entry.preset, "effect"), ent = "" + bebGet(eff, "entity");
    var inTarget = bebKeyCount(infos, "S:" + bebGet(entry.preset, "id"));
    var h = "<p><span style=\"color:" + BEB_C.accent + "; font-size:14px; font-weight:bold;\">" + bebEsc(entry.name) + "</span>&nbsp; " +
            bebChip("Shared · " + entry.type, BEB_C.shared, BEB_C.sharedText) + "<br/>" +
            bebGrey(bebEsc(entry.folder || "(top level)") + " · used by " + bebPlural(entry.uses, "event") + " · " + inTarget + " of these " + infos.length) + "</p>";
    if (ent === "EffectChain") {
        var inner = bebList(bebGet(eff, "effects"));
        if (!inner.length) { h += bebGrey("An empty effect chain."); }
        for (var k = 0; k < inner.length; k++) { h += bebDeckPanel(inner[k], (k + 1) + ". " + bebFxLabel("" + bebGet(inner[k], "entity"), inner[k])); }
    } else {
        h += bebDeckPanel(eff, entry.type);
    }
    return h;
}

// A unique effect: each event has its own copy, so this shows the first one found
// and says the others may differ.
function bebDeckUnique(key, infos, scope) {
    var first = null, where = "", n = 0;
    for (var i = 0; i < infos.length; i++) {
        var chains = bebScopeChains(infos[i].ev, scope), found = false;
        for (var c = 0; c < chains.length; c++) {
            var l = bebChainFx(chains[c].chain);
            for (var f = 0; f < l.length; f++) {
                if (bebFxKey(l[f]) !== key) { continue; }
                found = true;
                if (!first) { first = l[f]; where = infos[i].path + " (" + chains[c].label + ")"; }
            }
        }
        if (found) { n++; }
    }
    if (!first) { return bebGrey("Not found on these events."); }
    var label = bebFxChipName(first);
    return "<p><span style=\"color:" + BEB_C.accent + "; font-size:14px; font-weight:bold;\">" + bebEsc(label) + "</span>&nbsp; " +
           bebChip("Unique", BEB_C.unique, BEB_C.uniqueText) + "<br/>" +
           bebGrey("Each event has its own copy · on " + n + " of these " + infos.length + (n > 1 ? " · settings shown from " + bebEsc(where) + ", others may differ" : " · " + bebEsc(where))) + "</p>" +
           bebDeckPanel(first, label);
}

// The right-hand preview for the Effects window: what's being removed (red), then
// what's being added (green). Replace shows both.
function bebDeckDoc(o, infos, shared) {
    var h = "", removing = null;
    if (o.action !== "add") {
        if (!o.key) { h += bebDeckBanner("remove", o.action === "remove" ? "Removing" : "Replacing") + "<p>" + bebGrey("Choose the effect to " + o.action + ".") + "</p>"; }
        else {
            if (o.key.charAt(0) === "S") {
                for (var i = 0; i < shared.length; i++) { if ("S:" + bebGet(shared[i].preset, "id") === o.key) { removing = shared[i]; } }
            }
            h += bebDeckBanner("remove", (o.action === "remove" ? "Removing" : "Replacing") + (removing ? " these events' instances" : " from these events")) +
                 (removing ? bebDeckShared(removing, infos) : bebDeckUnique(o.key, infos, o.scope));
        }
    }
    if (o.action !== "remove") {
        h += (h ? "<br/>" : "") + bebDeckBanner("add", o.action === "add" ? "Adding" : "Putting in its place");
        h += o.entry ? bebDeckShared(o.entry, infos) : "<p>" + bebGrey("Choose the shared effect to " + (o.action === "add" ? "add." : "put in its place.")) + "</p>";
    }
    return "<html><body style=\"color:#dddddd;\">" + h + "</body></html>";
}

// ── Tracks ─────────────────────────────────────────────────────────────────
// Edits one track, matched across the target events by kind and name (every
// group track called "Plot Functions", say): set or offset its fader volume,
// clear automation on it (fader, panner, or an effect on it), or open the
// Effects window limited to it. Automation is captured point by point before
// it's deleted, so Undo can rebuild it (the method Copy-Paste Conditions uses).

// Tracks present in the target: [{ key, kind, name, label, events }], Master first.
function bebTrackCatalog(infos) {
    var by = {}, order = [];
    for (var i = 0; i < infos.length; i++) {
        var tr = bebTracksOf(infos[i].ev), seen = {};
        for (var t = 0; t < tr.length; t++) {
            var key = bebTrackKey(tr[t]);
            if (!by[key]) {
                by[key] = { key: key, kind: tr[t].kind, name: tr[t].name, events: 0,
                            label: tr[t].kind === "Master" ? "Master" : tr[t].kind + " “" + tr[t].name + "”" };
                order.push(by[key]);
            }
            if (!seen[key]) { seen[key] = true; by[key].events++; }
        }
    }
    var rank = { Master: 0, Group: 1, Return: 2 };
    order.sort(function (a, b) { return rank[a.kind] - rank[b.kind] || b.events - a.events || (a.name < b.name ? -1 : 1); });
    return order;
}

function bebFindTrack(ev, key) {
    var tr = bebTracksOf(ev);
    for (var i = 0; i < tr.length; i++) { if (bebTrackKey(tr[i]) === key) { return tr[i]; } }
    return null;
}

// Everything automated on a track: the fader bus, its panner, and each effect
// at the top of its chain (a shared effect's instance automation included;
// automation inside a shared effect belongs to the shared effect and is left alone).
// → [{ key, obj, prop, label, automator }]
function bebTrackAutomation(track) {
    var out = [];
    function add(obj, prefix, keyBase) {
        var auts = bebList(bebGet(obj, "automators"));
        for (var a = 0; a < auts.length; a++) {
            var prop = "" + bebGet(auts[a], "nameOfPropertyBeingAutomated");
            out.push({ key: keyBase + "|" + prop, obj: obj, prop: prop, automator: auts[a], label: prefix + bebPropLabel(prop) });
        }
    }
    add(track.bus, "Fader ", "bus");
    var pan = bebGet(track.bus, "panner");
    if (pan) { add(pan, "Panner ", "pan"); }
    var fxs = bebChainFx(bebGet(track.bus, "effectChain"));
    for (var f = 0; f < fxs.length; f++) {
        var k = bebFxKey(fxs[f]);
        if (k) { add(fxs[f], (k.charAt(0) === "S" ? "[Shared] " : "") + bebFxChipName(fxs[f]) + " › ", "fx:" + k); }
    }
    for (var o = 0; o < out.length; o++) { if (out[o].label === "Fader Volume") { out[o].label = "Fader volume"; } }
    return out;
}

// Every curve of an automator, with its points, so it can be drawn and rebuilt.
function bebCaptureAutomator(aut) {
    var curves = bebList(bebGet(aut, "automationCurves")), out = [];
    for (var c = 0; c < curves.length; c++) {
        var param = bebGet(curves[c], "parameter"), pts = bebList(bebGet(curves[c], "automationPoints")), pd = [];
        for (var p = 0; p < pts.length; p++) {
            pd.push({ position: bebGet(pts[p], "position"), value: bebGet(pts[p], "value"),
                      curveShape: bebGet(pts[p], "curveShape") || 0, isSCurve: !!bebGet(pts[p], "isSCurve") });
        }
        pd.sort(function (x, y) { return x.position - y.position; });
        var lo = bebGet(param, "minimum"), hi = bebGet(param, "maximum");
        if (typeof lo !== "number" || typeof hi !== "number" || hi <= lo) {
            lo = pd.length ? pd[0].position : 0; hi = pd.length ? pd[pd.length - 1].position : 1;
            if (hi <= lo) { hi = lo + 1; }
        }
        out.push({ param: param, paramName: bebParamName(param), points: pd, lo: lo, hi: hi });
    }
    return out;
}

function bebAutoAt(points, x) {
    if (!points.length) { return 0; }
    if (x <= points[0].position) { return points[0].value; }
    for (var i = 1; i < points.length; i++) {
        if (x <= points[i].position) {
            var a = points[i - 1], b = points[i], t = (x - a.position) / Math.max(1e-9, b.position - a.position);
            return a.value + (b.value - a.value) * t;
        }
    }
    return points[points.length - 1].value;
}

// An automation curve, Studio-style (#1c1c1c ground), in the given colour.
function bebDrawAuto(points, lo, hi, colorHex) {
    return function (w, h, s) {
        var cv = bebCanvas(w, h, "#1c1c1c");
        var line = bebRgb(colorHex), grid = bebRgb("#343434");
        var bg = bebRgb("#1c1c1c"), fill = [0, 1, 2].map(function (i) { return Math.round(bg[i] + (line[i] - bg[i]) * 0.22); });
        var vlo = Infinity, vhi = -Infinity;
        for (var i = 0; i < points.length; i++) { vlo = Math.min(vlo, points[i].value); vhi = Math.max(vhi, points[i].value); }
        if (!points.length) { vlo = 0; vhi = 1; }
        if (vhi - vlo < 1e-9) { vlo -= 1; vhi += 1; }
        var pad = 5 * s, pw = w - 2 * pad, ph = h - 2 * pad;
        function X(px) { return lo + (hi - lo) * (px - pad) / pw; }
        function Y(v) { return pad + (1 - (v - vlo) / (vhi - vlo)) * ph; }
        for (var g = 0; g <= 4; g++) { var gx = Math.round(pad + pw * g / 4); for (var gy = 0; gy < h; gy += 2 * s) { bebPut(cv, gx, gy, grid, 1); } }
        for (var px = 0; px < w; px++) {
            var ya = Y(bebAutoAt(points, X(px))), yb = Y(bebAutoAt(points, X(px + 1)));
            for (var py = Math.floor(Math.min(ya, yb)); py < h; py++) { bebPut(cv, px, py, fill, 1); }
            var a0 = Math.min(ya, yb) - 0.9 * s, a1 = Math.max(ya, yb) + 0.9 * s;
            for (var qy = Math.max(0, Math.floor(a0)); qy <= Math.min(h - 1, Math.ceil(a1)); qy++) { bebPut(cv, px, qy, line, Math.min(qy + 1, a1) - Math.max(qy, a0)); }
        }
        var rad = 2.4 * s;
        for (var p = 0; p < points.length; p++) {
            var cx = pad + (points[p].position - lo) / (hi - lo) * pw, cy = Y(points[p].value);
            for (var dy = Math.floor(cy - rad - 1); dy <= cy + rad + 1; dy++) {
                for (var dx = Math.floor(cx - rad - 1); dx <= cx + rad + 1; dx++) {
                    var d = Math.sqrt((dx + 0.5 - cx) * (dx + 0.5 - cx) + (dy + 0.5 - cy) * (dy + 0.5 - cy));
                    bebPut(cv, dx, dy, line, rad + 0.5 - d);
                }
            }
        }
        return cv.rgb;
    };
}

function bebAutoCurveHtml(curve, colorHex) {
    var img = bebImage("auto|" + colorHex + "|" + curve.lo + "|" + curve.hi + "|" + JSON.stringify(curve.points), 260, 54,
                       bebDrawAuto(curve.points, curve.lo, curve.hi, colorHex));
    var vals = [];
    for (var i = 0; i < curve.points.length && i < 6; i++) { vals.push(bebNum(curve.points[i].position) + "→" + bebNum(curve.points[i].value)); }
    return (img ? img + "<br/>" : "") + "<span style=\"color:#8a8a8a; font-size:10px;\">" + bebEsc(curve.paramName) + ": " +
           bebEsc(vals.join(", ")) + (curve.points.length > 6 ? " …" : "") + "</span>";
}

// op = { action: "volume" | "auto", track: key, mode: "set"|"offset", db, autoKey ("*" = all) }
// → { rows: [{ info, track, steps }], changes, skipped, warn }
function bebTrackPlan(infos, op) {
    var plan = { rows: [], changes: 0, skipped: 0, warn: [] };
    var missing = [], drivenVol = [], same = 0;
    for (var i = 0; i < infos.length; i++) {
        var tr = bebFindTrack(infos[i].ev, op.track);
        if (!tr) { missing.push(infos[i].path); plan.skipped++; continue; }
        var steps = [];
        if (op.action === "volume") {
            var from = bebProp(tr.bus, "volume");
            if (typeof from !== "number") { plan.skipped++; continue; }
            var to = op.mode === "offset" ? from + op.db : op.db;
            to = Math.max(-80, Math.min(10, Math.round(to * 100) / 100));
            if (bebSame(from, to)) { same++; plan.skipped++; continue; }
            var autos = bebTrackAutomation(tr);
            for (var a = 0; a < autos.length; a++) { if (autos[a].key === "bus|volume") { drivenVol.push(infos[i].path); } }
            steps.push({ type: "volume", from: from, to: to });
        } else {
            var list = bebTrackAutomation(tr);
            for (var b = 0; b < list.length; b++) {
                if (op.autoKey === "*" || list[b].key === op.autoKey) { steps.push({ type: "auto", target: list[b], curves: bebCaptureAutomator(list[b].automator) }); }
            }
            if (!steps.length) { plan.skipped++; continue; }
        }
        plan.changes += steps.length;
        plan.rows.push({ info: infos[i], track: tr, steps: steps });
    }
    function warn(l, text) { if (l.length) { plan.warn.push({ text: l.length + " " + text, events: l }); } }
    warn(missing, "don't have this track: skipped");
    warn(drivenVol, "have this fader's volume automated: the automation overrides the level you set");
    if (same) { plan.warn.push({ text: same + " already at that level", events: [] }); }
    return plan;
}

function bebApplyTracks(plan, op, trackLabel) {
    var items = [], ok = 0, bad = 0, undoOps = [];
    for (var r = 0; r < plan.rows.length; r++) {
        var row = plan.rows[r], parts = [], fails = [];
        for (var s = 0; s < row.steps.length; s++) {
            var st = row.steps[s];
            if (st.type === "volume") {
                var w = bebWrite(row.track.bus, "volume", st.to);
                if (w.ok) {
                    ok++; parts.push("fader " + bebNum(st.from) + " → " + bebNum(st.to) + " dB");
                    undoOps.push({ op: "value", obj: row.track.bus, prop: "volume", before: st.from, after: w.after, name: trackLabel,
                                   path: row.info.path, what: "fader back to " + bebNum(st.from) + " dB" });
                } else { bad++; fails.push(w.error); }
            } else {
                var tgt = st.target, err = null;
                try { studio.project.deleteObject(tgt.automator); } catch (e) { err = "" + e; }
                var still = false, auts = bebList(bebGet(tgt.obj, "automators"));
                for (var a = 0; a < auts.length; a++) { if ("" + bebGet(auts[a], "nameOfPropertyBeingAutomated") === tgt.prop) { still = true; } }
                if (!err && !still) {
                    ok++; parts.push("cleared " + tgt.label + " automation");
                    undoOps.push({ op: "autoRemoved", obj: tgt.obj, prop: tgt.prop, curves: st.curves, name: trackLabel + " " + tgt.label, path: row.info.path });
                } else { bad++; fails.push(tgt.label + ": " + (err || "automation still there after delete")); }
            }
        }
        if (parts.length) { items.push("~ " + row.info.path + "  " + trackLabel + ": " + parts.join(", ")); }
        if (fails.length) { items.push("! " + row.info.path + "  " + fails.join("; ")); }
    }
    var what = op.action === "volume" ? (op.mode === "offset" ? "changed the fader by " + (op.db > 0 ? "+" : "") + bebNum(op.db) + " dB"
                                                              : "set the fader to " + bebNum(op.db) + " dB")
                                      : "cleared automation";
    var summary = [(bad ? "! " : "+ ") + "Tracks: " + trackLabel + ": " + what + " on " + bebPlural(plan.rows.length, "event") +
                   (plan.skipped ? " · " + plan.skipped + " skipped" : "") + (bad ? " · " + bad + " failed" : "")];
    for (var x = 0; x < plan.warn.length; x++) { summary.push("! " + plan.warn[x].text); }
    return { ok: ok, bad: bad, lines: bebResultLines(summary, items),
             undo: { kind: "fx", ops: undoOps, label: "Tracks: " + trackLabel + " " + what + " on " + bebPlural(plan.rows.length, "event") } };
}

// Rebuilds removed automation (Undo).
function bebRestoreAutomation(o) {
    var auts = bebList(bebGet(o.obj, "automators"));
    for (var a = 0; a < auts.length; a++) { if ("" + bebGet(auts[a], "nameOfPropertyBeingAutomated") === o.prop) { return { skip: "automated again since: left as it is" }; } }
    var aut = null;
    try { aut = o.obj.addAutomator(o.prop); } catch (e) { return { error: "addAutomator failed: " + e }; }
    if (!aut) { return { error: "addAutomator returned nothing" }; }
    var placed = 0, total = 0;
    for (var c = 0; c < o.curves.length; c++) {
        var cd = o.curves[c], curve = null;
        try { curve = aut.addAutomationCurve(cd.param); } catch (e2) { curve = null; }
        if (!curve) { continue; }
        var seeded = bebList(bebGet(curve, "automationPoints"));
        for (var s = 0; s < seeded.length; s++) { try { studio.project.deleteObject(seeded[s]); } catch (e3) {} }
        for (var p = 0; p < cd.points.length; p++) {
            total++;
            var pt = null;
            try { pt = curve.addAutomationPoint(cd.points[p].position, cd.points[p].value); } catch (e4) { pt = null; }
            if (!pt) { continue; }
            bebWrite(pt, "curveShape", cd.points[p].curveShape);
            bebWrite(pt, "isSCurve", cd.points[p].isSCurve);
            placed++;
        }
    }
    return { text: placed === total ? "automation rebuilt (" + bebPlural(placed, "point") + ")" : "automation rebuilt, " + placed + " of " + total + " points placed" };
}

function bebTracksDialog(infos) {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    var result = null, tracks = bebTrackCatalog(infos), autos = [];
    var ACTIONS = ["volume", "auto", "effects"];
    var trackItems = [];
    for (var i = 0; i < tracks.length; i++) { trackItems.push({ text: tracks[i].label + "  ·  " + bebUsageText(tracks[i].events, infos.length) }); }

    function track(w) { return tracks[bebWIndex(w, "bebk_track")] || null; }

    function rebuildAutos(w) {
        var t = track(w), by = {}, order = [];
        if (t) {
            for (var i = 0; i < infos.length; i++) {
                var tr = bebFindTrack(infos[i].ev, t.key);
                if (!tr) { continue; }
                var list = bebTrackAutomation(tr), seen = {};
                for (var a = 0; a < list.length; a++) {
                    var k = list[a].key;
                    if (!by[k]) {
                        var cv = bebCaptureAutomator(list[a].automator), ps = [];
                        for (var c = 0; c < cv.length; c++) { ps.push(cv[c].paramName); }
                        by[k] = { key: k, label: list[a].label, by: ps.join(", "), events: 0 };
                        order.push(by[k]);
                    }
                    if (!seen[k]) { seen[k] = true; by[k].events++; }
                }
            }
        }
        autos = order;
        var items = [{ text: order.length ? "All automation on this track  ·  " + order.length + " kinds" : "(nothing on this track is automated)" }];
        for (var o = 0; o < order.length; o++) {
            items.push({ text: order[o].label + "  ←  " + order[o].by + "  ·  " + bebUsageText(order[o].events, t.events) });
        }
        var cb = bebW(w, "bebk_auto");
        if (cb) { try { cb.setItems(items); cb.setCurrentIndex(0); } catch (e) {} }
    }

    function read(w) {
        var ai = bebWIndex(w, "bebk_auto"), t = track(w);
        var dbText = bebTrim(bebWText(w, "bebk_db")), db = parseFloat(dbText);
        return { action: ACTIONS[bebWIndex(w, "bebk_action")], track: t ? t.key : null, label: t ? t.label : "",
                 mode: bebWIndex(w, "bebk_mode") === 1 ? "offset" : "set",
                 db: db, dbOk: dbText !== "" && !isNaN(db) && ("" + Number(dbText)) !== "NaN",
                 autoKey: ai === 0 ? "*" : (autos[ai - 1] ? autos[ai - 1].key : "*"), hasAutos: autos.length > 0 };
    }

    function enable(w, id, on) { var x = bebW(w, id); if (x) { try { x.setEnabled(on); } catch (e) {} } }

    function update(w) {
        var o = read(w);
        enable(w, "bebk_mode", o.action === "volume"); enable(w, "bebk_db", o.action === "volume");
        enable(w, "bebk_auto", o.action === "auto");
        if (!o.track) { bebSetPreview(w, "bebk_", bebGrey("These events have no tracks."), "Apply", false); return; }
        if (o.action === "effects") {
            bebSetPreview(w, "bebk_", "<p>Opens the Effects window limited to <b>" + bebEsc(o.label) +
                          "</b>: remove, add or replace effects on this track only, with the same previews.</p>",
                          "Open Effects for this track…", true);
            return;
        }
        if (o.action === "volume" && !o.dbOk) { bebSetPreview(w, "bebk_", bebGrey("Type a level in dB" + (o.mode === "offset" ? " to add, e.g. -3." : ", e.g. -6.")), "Apply", false); return; }
        if (o.action === "auto" && !o.hasAutos) { bebSetPreview(w, "bebk_", bebGrey("Nothing on this track is automated."), "Apply", false); return; }
        var plan = bebTrackPlan(infos, o);
        var h = "<p><b>" + bebPlural(plan.changes, "change") + " on " + bebPlural(plan.rows.length, "event") + "</b>" +
                (plan.skipped ? bebGrey(" · " + plan.skipped + " unchanged") : "") + "</p>";
        if (o.action === "auto") { h += "<p>" + bebGrey("Undo rebuilds cleared automation point by point.") + "</p>"; }
        for (var x = 0; x < plan.warn.length; x++) {
            var wn = plan.warn[x];
            h += "<p><span style=\"color:#ff9a6a;\">" + bebEsc(wn.text) + "</span>" + (wn.events.length ? "<br/>" +
                 bebGrey(bebEsc(wn.events.slice(0, 4).join(", ")) + (wn.events.length > 4 ? " …" : "")) : "") + "</p>";
        }
        for (var r = 0; r < plan.rows.length && r < 30; r++) {
            var row = plan.rows[r], lines = [], chips = [];
            var fx = bebChainFx(bebGet(row.track.bus, "effectChain"));
            for (var f = 0; f < fx.length; f++) {
                var isF = "" + bebGet(fx[f], "entity") === "MixerBusFader";
                chips.push(isF ? bebChip("Fader", BEB_C.fader, BEB_C.faderText)
                               : bebChipHtml(bebEsc(bebFxChipName(fx[f])), bebKey(fx[f]).charAt(0) === "S" ? BEB_C.shared : BEB_C.unique,
                                             bebKey(fx[f]).charAt(0) === "S" ? BEB_C.sharedText : BEB_C.uniqueText));
            }
            lines.push(bebGrey(bebEsc(o.label) + ":") + " " + chips.join(" "));
            for (var s = 0; s < row.steps.length; s++) {
                var st = row.steps[s];
                if (st.type === "volume") {
                    lines.push("Fader " + BEB_DEL + "&nbsp;" + bebNum(st.from) + " dB&nbsp;</span> " + bebGrey("→") + " " + BEB_ADD + "&nbsp;" + bebNum(st.to) + " dB&nbsp;</span>");
                } else {
                    var cl = [];
                    for (var c = 0; c < st.curves.length; c++) { cl.push(bebAutoCurveHtml(st.curves[c], "#e05a5a")); }
                    lines.push(BEB_DEL + "&nbsp;" + bebEsc(st.target.label) + " automation&nbsp;</span><br/>" + cl.join("<br/>"));
                }
            }
            h += bebCard(row.info.path, "", lines.join("<br/>"), BEB_C.event);
        }
        if (plan.rows.length > 30) { h += bebGrey("… " + (plan.rows.length - 30) + " more"); }
        var verb = o.action === "volume" ? "Set the fader on " : "Clear automation on ";
        bebSetPreview(w, "bebk_", h, plan.rows.length ? verb + bebPlural(plan.rows.length, "event") : "Nothing to change", plan.rows.length > 0);
    }

    studio.ui.showModalDialog({
        windowTitle: "Tracks — " + bebPlural(infos.length, "event"),
        windowWidth: 820, windowHeight: 740,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [
            { widgetType: W.Layout, layout: G.GridLayout, spacing: 6, items: [
                { widgetType: W.Label, text: "Track", row: 0, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebk_track", row: 0, column: 1, currentIndex: 0, minimumWidth: 420,
                  items: trackItems.length ? trackItems : [{ text: "(no tracks)" }],
                  onCurrentIndexChanged: function () { rebuildAutos(this); update(this); } },
                { widgetType: W.Label, text: "Action", row: 1, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebk_action", row: 1, column: 1, currentIndex: 0,
                  items: [{ text: "Set the fader volume" }, { text: "Clear automation" }, { text: "Edit effects on this track…" }],
                  onCurrentIndexChanged: function () { update(this); } },
                { widgetType: W.Label, text: "Fader", row: 2, column: 0 },
                { widgetType: W.Layout, layout: G.HBoxLayout, row: 2, column: 1, spacing: 8, items: [
                    { widgetType: W.ComboBox, widgetId: "bebk_mode", currentIndex: 0, items: [{ text: "Set to" }, { text: "Change by" }],
                      onCurrentIndexChanged: function () { update(this); } },
                    { widgetType: W.LineEdit, widgetId: "bebk_db", text: "", minimumWidth: 90, onTextEdited: function () { update(this); } },
                    { widgetType: W.Label, text: "dB  <span style=\"color:#9a9a9a;\">(−80 to +10)</span>" },
                    { widgetType: W.Spacer }
                ] },
                { widgetType: W.Label, text: "Automation", row: 3, column: 0 },
                { widgetType: W.ComboBox, widgetId: "bebk_auto", row: 3, column: 1, currentIndex: 0, items: [{ text: "" }],
                  onCurrentIndexChanged: function () { update(this); } },
                { widgetType: W.Label, wordWrap: true, row: 4, column: 0, columnSpan: 2,
                  text: "<span style=\"color:#9a9a9a;\">Tracks are matched by name across the events. Automation inside a shared effect " +
                        "belongs to the shared effect and isn't offered here.</span>" }
            ] }
        ].concat(bebDialogFooter(W, G, "bebk_", function () {
            var o = read(this);
            if (o.action === "effects") {
                var t = track(this);
                result = bebFxDialog(infos, { key: t.key, label: t.label });   // a modal on top of this one
                this.closeDialog();
                return;
            }
            var plan = bebTrackPlan(infos, o);
            if (!plan.rows.length) { return; }
            result = bebApplyTracks(plan, o, o.label);
            this.closeDialog();
        })),
        onConstructed: function () { rebuildAutos(this); update(this); }
    });
    return result;
}

// ── Undo ───────────────────────────────────────────────────────────────────
// Studio has no "undo this batch" for scripts (window.triggerAction(Undo) is one
// step in whatever window was last active, and a batch is many steps). So every
// apply records what it changed, and Undo restores exactly that — skipping any
// value that has been changed since, so later hand edits are never clobbered.
// Kept while Studio runs (lost on Scripts > Reload); the last 10 batches.
var BEB_UNDO_DEPTH = 10;
var bebHistory = [];

function bebRemember(rec) {
    if (!rec || !((rec.entries && rec.entries.length) || (rec.events && rec.events.length) || (rec.ops && rec.ops.length))) { return; }
    bebHistory.push(rec);
    while (bebHistory.length > BEB_UNDO_DEPTH) { bebHistory.shift(); }
}

function bebUndo(rec) {
    var items = [], ok = 0, skipped = 0, bad = 0;
    if (rec.kind === "fx") {
        var n = bebUndoFx(rec, items);
        ok = n.ok; skipped = n.skipped; bad = n.bad;
    } else if (rec.kind === "values") {
        for (var i = 0; i < rec.entries.length; i++) {
            var e = rec.entries[i];
            var cur;
            try { cur = bebProp(e.obj, e.prop); } catch (x) {}
            if (!bebSame(cur, e.after)) { skipped++; items.push("= " + e.path + "  " + e.label + " changed since: left as it is"); continue; }
            var r = bebWrite(e.obj, e.prop, e.before);
            if (r.ok) { ok++; items.push("~ " + e.path + "  " + e.label + (e.def ? " " + bebMacroFmt(e.def, e.after) + " → " + bebMacroFmt(e.def, e.before) : " restored")); }
            else { bad++; items.push("! " + e.path + "  " + e.label + ": " + r.error); }
        }
    } else {
        var tag = bebFindTag(rec.name);
        for (var j = 0; j < rec.events.length; j++) {
            var ev = rec.events[j].ev, path = rec.events[j].path, err = null;
            var has = !!bebEventTag(ev, rec.name);
            if (has !== (rec.mode === "add")) { skipped++; items.push("= " + path + "  tag changed since: left as it is"); continue; }
            try {
                if (rec.mode === "add") { ev.relationships.tags.remove(bebEventTag(ev, rec.name)); }
                else {
                    if (!tag) { tag = studio.project.workspace.addTag(rec.name); }
                    ev.relationships.tags.add(tag);
                }
            } catch (x2) { err = "" + x2; }
            if (!err && !!bebEventTag(ev, rec.name) !== (rec.mode === "add")) { ok++; items.push((rec.mode === "add" ? "- " : "+ ") + path); }
            else { bad++; items.push("! " + path + "  " + (err || "didn't stick")); }
        }
        // A tag this tool created goes again, but only when nothing uses it.
        if (rec.mode === "add" && rec.created) {
            var t = bebFindTag(rec.name);
            if (t && !bebList(bebGet(t, "taggables")).length) {
                try { studio.project.deleteObject(t); items.unshift("- Deleted the tag “" + rec.name + "” (this tool created it; nothing uses it now)"); }
                catch (x3) { items.unshift("! Couldn't delete the tag “" + rec.name + "”: " + x3); }
            }
        }
    }
    return { lines: bebResultLines([(bad ? "! " : "+ ") + "Undid " + rec.label + " · " + ok + " restored" +
                                    (skipped ? " · " + skipped + " changed since, left alone" : "") + (bad ? " · " + bad + " failed" : "")], items) };
}

// ── Guide ──────────────────────────────────────────────────────────────────
// The in-window quick guide (Guide… button). README.md carries the same
// content; keep them in step.
function bebGuideHtml() {
    var C = BEB_C;
    function h(t) { return "<p style=\"margin-top:14px;\"><span style=\"color:" + C.accent + "; font-size:14px; font-weight:bold;\">" + t + "</span></p>"; }
    function p(t) { return "<p>" + t + "</p>"; }
    function li(items) { return "<ul style=\"margin-top:0;\"><li>" + items.join("</li><li>") + "</li></ul>"; }
    var fxS = bebFxChip({ shared: true, presetName: "s_map_emitter_medium", driven: [] });
    var fxU = bebFxChip({ shared: false, label: "3-EQ", driven: [] });
    var fxF = bebFxChip({ isFader: true, driven: [] });
    var fxA = bebFxChip({ shared: false, label: "Gain", driven: [{}] });
    var fxB = bebFxChip({ shared: false, label: "Chorus", bypass: true, driven: [] });
    var out = "";
    out += h("Quick start") + li([
        "Select events (or event folders) in the Event Browser's <b>Events</b> tab. The window follows the selection; click away and it keeps the last one.",
        "Pick a <b>View</b> on the right to inspect. Nothing in the views ever changes the project.",
        "Narrow <b>Apply to</b> to a group if you only want some of the events, then use <b>Tags… / Notes… / Macros… / Effects… / Tracks…</b>. Each previews its changes before Apply.",
        "<b>Undo</b> reverts the last batch. Save the project to keep changes."]);
    out += h("What it's for, and what it isn't") + li([
        "It's built to <b>standardise events on shared effects</b>: spot unique effects and inconsistent setups, then remove them or replace them with shared effects. It never creates new unique effects.",
        "It doesn't create or edit shared effects themselves. Do that in Studio's <b>Shared Effects</b> browser; this tool only adds, removes and swaps instances of them on events.",
        "It doesn't batch-edit <b>instruments</b> on tracks. For instrument trigger conditions and automation, use the <b>Copy-Paste Conditions &amp; Automation</b> script."]);
    out += h("How groups work") +
        p("Events are grouped by their <b>master track effect chain</b>: the same effects, in the same order. The fader counts as a position, so a send after the fader differs from one before it. Groups are lettered A, B, C… from the largest.") +
        li(["<b>Shared effects</b> are matched by name: s_map_emitter_medium and s_plots_UX are different.",
            "<b>Unique effects</b> are matched by type only: two events with their own Spatializer match even if their settings differ.",
            "A <b>bypassed</b> effect counts as different from an active one."]) +
        p("Groups do <b>not</b> compare effect values, group/return tracks, parameter sheets, tags or macros. Use the Spatializer, Effects and Macros views to see value differences (“mixed” and the outlier lists).") +
        p("<b>Apply to</b> picks the target: all events, or one group. Every view, Export TSV and every edit then covers only that target. <b>Rescan</b>, next to it, picks up edits made elsewhere in Studio.");
    out += h("Reading the views") + li([
        fxS + " shared effect (in the project's shared effects) &nbsp; " + fxU + " unique effect &nbsp; " + fxF + " fader",
        fxA + " automated or modulated &nbsp; " + fxB + " bypassed",
        "<b>×3</b> is how many events have that value. <span style=\"color:" + C.warn + ";\">mixed</span> means the target's events disagree.",
        "<b>Patterns</b>: groups, and a summary of everything below. <b>Differences</b>: a written brief of how the events differ, the outliers, and which event differs the most and why (details follow the brief). <b>Effects</b>: every insert with values and automation. <b>Spatializer</b>: settings and outliers. <b>Sheets</b>: timeline, parameter and action sheets. <b>Macros</b>, <b>Tags &amp; Notes</b>, <b>Per event</b>: one block per event.",
        "In <b>Differences</b>, an <b>outlier</b> is a value only a quarter of the events or fewer have, where most agree on another. The event that differs the most is the one off the majority most often.",
        "<b>Export TSV</b> (top right) copies one row per event of the target to the clipboard: chain, spatializer, distances, tags, note and macros. Paste it into a spreadsheet to sort, filter or share an audit. If the clipboard fails, it writes the file to <b>.user/</b> next to the project."]);
    out += h("How things are labelled") + li([
        "Dropdowns can't show colour, so every effect in a list says what it is in words: <b>[Shared]</b>&nbsp;s_map_emitter_medium — Spatializer, <b>[Unique]</b>&nbsp;Gain, <b>[Chain]</b>&nbsp;g_plots_special › Gain (an effect inside a shared effect chain).",
        "Inside a chain (“Gain › [Shared] send_rev › Fader”) shared effects keep their tag and unique effects are just their type.",
        "Counts read “on 3 of 7 events”. “(5 copies)” means some events have the effect more than once.",
        "In the views the same meaning is colour: blue chips are shared, amber chips unique, and event names are always <span style=\"color:" + C.event + ";\">this colour</span>."]);
    out += h("Values worth knowing") + li([
        "<b>Effective min / max</b>: the spatializer's own range when it overrides, otherwise the event's macro min/max.",
        "<b>2D pan mix</b> 0% is fully 3D. <b>Envelopment</b> is Auto, User or Off.",
        "<b>Max instances</b> 65 means ∞. <b>Cooldown</b> is in milliseconds.",
        "A spatializer that overrides the range ignores the macro min/max distance; the Macros editor warns about this.",
        "<b>Occlusion</b> (a Spatializer setting, On by default) decides whether occlusion from the game, set through the FMOD API or geometry, affects that spatializer. <b>Doppler</b> on a spatializer scales the event's Doppler effect. Neither appears on Studio's Spatializer panel, so the Effects preview lists them in grey under the panel rather than drawing them.",
        "<b>Pitch</b> in the macros is the event's master bus pitch, in semitones."]);
    out += h("Editing") + li([
        "<b>Tags…</b>: add or remove one tag. Type a new name or pick one. A new tag is only created if no tag has that name. Removing a tag from events never deletes it from the project.",
        "<b>Notes…</b>: Append (Enter makes new lines), Find &amp; remove (case-sensitive), or Clear. The preview shows " + BEB_ADD + "&nbsp;added&nbsp;</span> and " + BEB_DEL + "&nbsp;removed&nbsp;</span> text per event.",
        "<b>Macros…</b>: each row starts at the most common value; changing it ticks the row. The panel on the right is laid out like Studio's Event Macros drawer and shows the settings after Apply: changing ones in green with the old value struck in red, and “mixed” where the events differ. The preview shows " + BEB_DEL + "&nbsp;old&nbsp;</span> → " + BEB_ADD + "&nbsp;new&nbsp;</span> per event, and warns about min &gt; max, Doppler off, missing spatializers and automated macros.",
        "<b>Tracks…</b>: pick a track by name (every group track called “Plot Functions” across the events, say), then set its fader to a level or change it by ±dB, clear automation on it (the fader, its panner, or an effect on it; the preview draws each curve being removed in red), or open the Effects window limited to that track. Events without that track are skipped.",
        "Every value is read back after writing; anything that didn't stick is listed as a failure in <b>Last result</b>.",
        "<b>Effects…</b>: remove an effect, add a shared effect (before the fader by default), or replace an effect with a shared one in the same position. Remove and replace act on the Master track unless you tick Group or Return tracks; Add always goes to the Master track. Find and Folder narrow the shared-effect list, and the panel on the right previews it like Studio's deck (knobs, switches, the distance attenuation graph): what's being removed under a red heading, what's being added under a green one, both for Replace. With the list closed, the arrow keys step through it. The preview shows each chain with " + BEB_DEL + "&nbsp;removed&nbsp;</span> and " + BEB_ADD + "&nbsp;+ added&nbsp;</span> effects.",
        "Removing a shared effect only removes these events' instances; the shared effect stays in the project. Effects inside a shared effect chain are never changed here, because that would change the chain for every event using it. The fader is never touched."]);
    out += h("Undo") +
        p("The <b>Undo</b> button sits under Last result. It reverses this window's most recent edit batch (the one reported in Last result), then the one before. It only covers Batch Event Editor edits; Studio's own Edit › Undo is separate and steps back one change at a time. Studio can't undo a script's batch in one step, so the tool records what each batch changed and restores exactly that, newest first (10 batches kept until Scripts › Reload). Anything you've changed by hand since is left alone and reported. Undoing a new tag deletes it again if nothing uses it. Cleared automation is rebuilt point by point. Removed effects are put back with their settings, but <b>not their automation</b>, and plugin effects can't be re-created; the Effects preview warns about both. For big batches, commit Metadata/ to source control first.");
    out += h("Last result symbols") + p("<tt>+</tt> added/succeeded &nbsp; <tt>~</tt> changed &nbsp; <tt>-</tt> removed &nbsp; <tt>=</tt> skipped/unchanged &nbsp; <tt>!</tt> warning or failure");
    return "<html><body style=\"color:" + C.text + ";\">" + out + "</body></html>";
}

function bebShowGuide() {
    var W = studio.ui.widgetType, G = studio.ui.layoutType;
    studio.ui.showModelessDialog({
        windowTitle: BEB_TITLE + " — Guide",
        windowWidth: 640, windowHeight: 720,
        widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8,
        items: [
            { widgetType: W.TextEdit, widgetId: "bebg_text", isReadOnly: true, html: bebGuideHtml(),
              sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Expanding, verticalPolicy: studio.ui.sizePolicy.Expanding } },
            { widgetType: W.Layout, layout: G.HBoxLayout, items: [
                { widgetType: W.Spacer },
                { widgetType: W.PushButton, widgetId: "bebg_close", text: "Close", minimumWidth: 100,
                  onClicked: function () { this.closeDialog(); } }
            ] }
        ]
    });
}

// ── Window ─────────────────────────────────────────────────────────────────
studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Edit Events in Batch...",
    execute: function () {
        var W = studio.ui.widgetType;
        var G = studio.ui.layoutType;
        var FIXED = { horizontalPolicy: studio.ui.sizePolicy.Fixed, verticalPolicy: studio.ui.sizePolicy.Fixed };
        var GREY = "<span style=\"color:#9a9a9a;\">";
        var state = {
            cache: {}, timerId: null, inRefresh: false,
            rawSig: null,          // ids of the selected events/folders last scanned
            items: [],             // that selection
            res: null,             // resolved events
            infos: [],             // scanned events
            groups: [],
            targetSigs: [],        // combo index → group sig ("" = all)
            dirty: true,
            held: false,           // selection is empty: showing the last one
            scanMs: 0,
            log: []
        };

        function widget(w, id) { try { return w.findWidget(id); } catch (e) { return null; } }
        function isChecked(w, id) { var x = widget(w, id); try { return !!(x && x.isChecked()); } catch (e) { return false; } }
        function comboIndex(w, id) { var x = widget(w, id); try { return x ? x.currentIndex() : 0; } catch (e) { return 0; } }

        function setOnce(w, id, what, value) {
            var key = id + "|" + what;
            if (state.cache[key] === value) { return; }
            state.cache[key] = value;
            var x = widget(w, id);
            if (!x) { return; }
            try {
                if (what === "text") { x.setText(value); }
                else if (what === "html") { x.setHtml(value); }
                else { x.setEnabled(value); }
            } catch (e) {}
        }

        function setLog(w, lines) {
            var box = widget(w, "beb_log");
            if (box) { try { box.setText(lines.join("\n")); } catch (e) {} }
            studio.system.print("=== " + BEB_TITLE + " v" + BEB_VERSION + " ===\n" + lines.join("\n") + "\n");
        }

        function sigOf(items, recursive) {
            var ids = [];
            for (var i = 0; i < items.length; i++) { ids.push("" + items[i].id); }
            return ids.join(",") + "|" + (recursive ? "r" : "");
        }

        function rescan(w, items, recursive) {
            var t0 = new Date().getTime();
            state.items = items;
            state.res = bebResolve(items, recursive);
            state.infos = [];
            for (var i = 0; i < state.res.events.length; i++) { state.infos.push(bebScanEvent(state.res.events[i])); }
            state.infos.sort(function (a, b) { return a.path < b.path ? -1 : a.path > b.path ? 1 : 0; });
            state.groups = bebGroups(state.infos);
            state.scanMs = new Date().getTime() - t0;
            rebuildTargets(w);
            state.dirty = true;
        }

        // "Apply to": all, or one pattern group. Keeps the chosen group when
        // it still exists after a rescan.
        function rebuildTargets(w) {
            var cb = widget(w, "beb_target");
            var prevSig = state.targetSigs[comboIndex(w, "beb_target")];
            var items = [{ text: "All " + bebPlural(state.infos.length, "event") }];
            state.targetSigs = [""];
            for (var i = 0; i < state.groups.length; i++) {
                var g = state.groups[i];
                items.push({ text: "Group " + g.letter + " — " + bebTrunc(g.sig, 52) + "  ·  " + bebPlural(g.events.length, "event") });
                state.targetSigs.push(g.sig);
            }
            var idx = 0;
            for (var j = 0; j < state.targetSigs.length; j++) { if (prevSig && state.targetSigs[j] === prevSig) { idx = j; } }
            if (cb) {
                try { cb.setItems(items); cb.setCurrentIndex(idx); } catch (e) {}
            }
        }

        function targetInfos(w) {
            var sig = state.targetSigs[comboIndex(w, "beb_target")];
            if (!sig) { return state.infos; }
            for (var i = 0; i < state.groups.length; i++) { if (state.groups[i].sig === sig) { return state.groups[i].events; } }
            return state.infos;
        }

        function bannerHtml() {
            var r = state.res;
            var n = state.infos.length;
            var big, small, third;
            if (!r || (!n && !state.items.length)) {
                big = "<span style=\"font-size:15px; color:#9a9a9a;\">No events selected</span>";
                small = GREY + "Select events or folders in the Event Browser (Events tab)</span>";
                third = "&nbsp;";
            } else {
                big = "<span style=\"font-size:15px; font-weight:bold; color:#ffd84d;\">" + bebPlural(n, "event") + "</span>";
                var parts = [];
                if (r.direct) { parts.push(r.direct + " selected"); }
                if (r.folders) {
                    parts.push(state.recursive ? (r.direct ? "+ " : "") + r.folderEvents + " inside " + bebPlural(r.folders, "folder")
                                               : bebPlural(r.folders, "folder") + " not included");
                }
                small = GREY + bebEsc(bebTrunc(parts.join(" ") + (r.folderNames.length ? "  ·  " + r.folderNames.join(", ") : ""), 64)) + "</span>";
                third = state.held
                    ? "<span style=\"color:#8fc7ff;\">Holding the last selection</span>" + GREY + " · select events to change it</span>"
                    : GREY + "scanned in " + (state.scanMs / 1000).toFixed(1) + " s · " + bebPlural(state.groups.length, "pattern") + "</span>";
            }
            return "<table width=\"100%\" cellpadding=\"6\" bgcolor=\"#3b3a26\"><tr><td>" + big + "<br/>" + small + "<br/>" + third + "</td></tr></table>";
        }

        function refresh(w, force) {
            if (state.inRefresh) { return; }
            state.inRefresh = true;
            try {
                var recursive = isChecked(w, "beb_recursive");
                bebOptions.recursive = recursive;
                state.recursive = recursive;
                bebOptions.view = comboIndex(w, "beb_view");
                var items = bebRawSelection();
                // An empty selection (another browser tab, or focus in this
                // window) keeps the last one, so the inspector doesn't blank.
                if (!items.length && state.items.length) {
                    state.held = true;
                    items = state.items;
                } else {
                    state.held = false;
                }
                var sig = sigOf(items, recursive);
                if (force || sig !== state.rawSig) {
                    state.rawSig = sig;
                    rescan(w, items, recursive);
                }
                setOnce(w, "beb_selection", "text", bannerHtml());
                var targets = targetInfos(w);
                setOnce(w, "beb_export", "text", targets.length ? "Export TSV (" + targets.length + ")" : "Export TSV");
                setOnce(w, "beb_export", "enabled", targets.length > 0);
                setOnce(w, "beb_rescan", "enabled", state.items.length > 0);
                setOnce(w, "beb_tags", "enabled", targets.length > 0);
                setOnce(w, "beb_notes", "enabled", targets.length > 0);
                setOnce(w, "beb_macros", "enabled", targets.length > 0);
                setOnce(w, "beb_fx", "enabled", targets.length > 0);
                setOnce(w, "beb_tracks", "enabled", targets.length > 0);
                var last = bebHistory[bebHistory.length - 1];
                setOnce(w, "beb_undo", "text", last ? "Undo: " + bebTrunc(last.label, 48) + (bebHistory.length > 1 ? "  (+" + (bebHistory.length - 1) + ")" : "") : "Undo (nothing to undo)");
                setOnce(w, "beb_undo", "enabled", !!last);
                if (state.dirty) {
                    state.dirty = false;
                    setOnce(w, "beb_viewTitle", "text", "<b>" + bebEsc(BEB_VIEWS[bebOptions.view] || "") + "</b>" +
                            GREY + "  ·  " + bebPlural(targets.length, "event") + (targets.length !== state.infos.length ? " in this group" : "") + "</span>");
                    setOnce(w, "beb_html", "html", bebRenderView(bebOptions.view, targets, targets === state.infos ? state.groups : bebGroups(targets)));
                }
            } finally {
                state.inRefresh = false;
            }
        }

        function guarded(fn) {
            return function () {
                var w = this;
                if (state.inRefresh) { return; }
                try { fn(w); state.dirty = true; refresh(w); }
                catch (e) { setLog(w, ["! Unexpected error: " + e]); }
            };
        }

        function doExport(w) {
            var infos = targetInfos(w);
            var lines = bebTsv(infos);
            var err = bebCopyToClipboard(lines);
            if (!err) {
                setLog(w, ["+ Copied " + bebPlural(infos.length, "row") + " to the clipboard (tab-separated, with a header).",
                           "  Paste into a spreadsheet."]);
                return;
            }
            // Fall back to a file next to the project.
            var path = "";
            try {
                var fp = "" + studio.project.filePath;
                path = fp.substring(0, fp.lastIndexOf("/") + 1) + ".user/" + BEB_TITLE + " export.tsv";
                var f = studio.system.getFile(path);
                f.open(studio.system.openMode.WriteOnly);
                f.writeText(lines.join("\n"));
                f.close();
                setLog(w, ["! Clipboard failed (" + err + ").", "+ Wrote " + bebPlural(infos.length, "row") + " to " + path]);
            } catch (e) {
                setLog(w, ["! Clipboard failed (" + err + ") and the file write failed (" + e + ")."]);
            }
        }

        // Opens an edit dialog on the current target. After an apply, log the
        // result and rescan so the views show the new state.
        function runEdit(w, dialogFn) {
            var infos = targetInfos(w);
            if (!infos.length) { return; }
            var res = dialogFn(infos);
            if (!res) { return; }
            bebRemember(res.undo);
            setLog(w, res.lines.concat(["  Save the project to keep the changes. Undo reverts this batch."]));
            state.rawSig = null;
        }

        function runUndo(w) {
            var rec = bebHistory.pop();
            if (!rec) { return; }
            setLog(w, bebUndo(rec).lines.concat(["  Save the project to keep the changes."]));
            state.rawSig = null;
        }

        function zone(n, title, sub) {
            return { widgetType: W.Label, text: "<table width=\"100%\" cellpadding=\"3\" bgcolor=\"#2a2a2a\"><tr><td>" +
                     "<span style=\"color:#ffd84d;\"><b>" + n + "</b></span>&nbsp;&nbsp;<b>" + title + "</b>&nbsp;&nbsp;" +
                     GREY + sub + "</span></td></tr></table>" };
        }

        function editButton(id, text, dialogFn) {
            var d = { widgetType: W.PushButton, widgetId: id, text: text, isEnabled: false,
                      minimumWidth: 186, maximumWidth: 186, sizePolicy: FIXED };
            if (dialogFn) { d.onClicked = guarded(function (w) { runEdit(w, dialogFn); }); }
            return d;
        }

        var viewItems = [];
        for (var v = 0; v < BEB_VIEWS.length; v++) { viewItems.push({ text: BEB_VIEWS[v] }); }

        var leftItems = [
            { widgetType: W.Label, widgetId: "beb_selection", wordWrap: false, text: "",
              minimumHeight: 74, maximumHeight: 74, minimumWidth: 390, maximumWidth: 390 },

            zone("1", "Target", "what the views and actions cover"),
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 6, items: [
                { widgetType: W.Label, text: "Apply to" },
                { widgetType: W.ComboBox, widgetId: "beb_target", items: [{ text: "All events" }], currentIndex: 0,
                  minimumWidth: 320, maximumWidth: 320, onCurrentIndexChanged: guarded(function () {}) }
            ] },
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
                { widgetType: W.CheckBox, widgetId: "beb_recursive", isChecked: bebOptions.recursive,
                  text: "Include events inside selected folders", onToggled: guarded(function () {}) },
                { widgetType: W.Spacer },
                { widgetType: W.PushButton, widgetId: "beb_rescan", text: "Rescan", isEnabled: false, minimumWidth: 90,
                  onClicked: guarded(function (w) { state.rawSig = null; }) } ] },
            { widgetType: W.Label, wordWrap: true, maximumWidth: 390,
              text: GREY + "Rescan picks up edits made elsewhere in Studio.</span>" },

            zone("2", "Edit", "acts on the Apply-to target"),
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
                editButton("beb_tags", "Tags…", bebTagsDialog), editButton("beb_notes", "Notes…", bebNotesDialog) ] },
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
                editButton("beb_macros", "Macros…", bebMacrosDialog), editButton("beb_fx", "Effects…", function (infos) { return bebFxDialog(infos); }) ] },
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 8, items: [
                editButton("beb_tracks", "Tracks…", bebTracksDialog), { widgetType: W.Spacer } ] },
            { widgetType: W.Label, wordWrap: true, maximumWidth: 390,
              text: GREY + "Commit Metadata/ before big batches.</span>" },

            zone("3", "Last result", ""),
            { widgetType: W.TextEdit, widgetId: "beb_log", isReadOnly: true, minimumHeight: 120,
              minimumWidth: 390, maximumWidth: 390, text: "" },
            { widgetType: W.PushButton, widgetId: "beb_undo", text: "Undo (nothing to undo)", isEnabled: false,
              minimumWidth: 390, maximumWidth: 390, onClicked: guarded(function (w) { runUndo(w); }) },
            { widgetType: W.Label, wordWrap: true, maximumWidth: 390,
              text: GREY + "Undo reverses the most recent edit batch from this window (the one reported in Last result), " +
                    "then the one before it, up to 10. It only covers Batch Event Editor edits, and is separate from " +
                    "Studio's own Edit › Undo, which steps back one change at a time.</span>" }
        ];

        var rightItems = [
            { widgetType: W.Layout, layout: G.HBoxLayout, spacing: 6, items: [
                { widgetType: W.Label, text: "View" },
                { widgetType: W.ComboBox, widgetId: "beb_view", items: viewItems, currentIndex: bebOptions.view,
                  minimumWidth: 180, onCurrentIndexChanged: guarded(function () {}) },
                { widgetType: W.Label, widgetId: "beb_viewTitle", text: "" },
                { widgetType: W.Spacer },
                { widgetType: W.PushButton, widgetId: "beb_export", text: "Export TSV", isEnabled: false, minimumWidth: 120,
                  onClicked: guarded(function (w) { doExport(w); }) },
                { widgetType: W.PushButton, widgetId: "beb_guide", text: "Guide…", minimumWidth: 90,
                  onClicked: function () { try { bebShowGuide(); } catch (e) { studio.system.print("Guide: " + e); } } }
            ] },
            { widgetType: W.TextEdit, widgetId: "beb_html", isReadOnly: true, html: "",
              minimumWidth: 560, minimumHeight: 560,
              sizePolicy: { horizontalPolicy: studio.ui.sizePolicy.Expanding, verticalPolicy: studio.ui.sizePolicy.Expanding } }
        ];

        studio.ui.showModelessDialog({
            windowTitle: BEB_TITLE + "  v" + BEB_VERSION,
            windowWidth: 1100,
            windowHeight: 720,
            widgetType: W.Layout,
            layout: G.HBoxLayout,
            spacing: 16,
            items: [
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 8, items: leftItems },
                { widgetType: W.Layout, layout: G.VBoxLayout, spacing: 6, items: rightItems }
            ],
            onConstructed: function () {
                try { bebClearImages(); } catch (e0) {}
                try { refresh(this, true); } catch (e) { setLog(this, ["! " + e]); }
                try { state.timerId = this.startTimer(500); } catch (e2) {}
            },
            onTimerEvent: function () {
                try { refresh(this); } catch (e) {}
            },
            onClose: function () {
                try { if (state.timerId !== null) { this.stopTimer(state.timerId); } } catch (e) {}
            }
        });
    }
});
