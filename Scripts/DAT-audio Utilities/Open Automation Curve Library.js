// Automation Curve Library
// Version: 0.8.5
//
// FMOD Studio 2.02, 2.03 and 2.04 side of the Automation Curve Library: 2.04-style automation
// curve presets and precision editing, as a companion app. This script
// launches the app (Tools/AutomationCurveLibrary/curve_library) and carries a
// live link to it: the app sends JSON-line requests on its stdout, a small
// "Curve Library link" window polls them on a timer, and this script answers
// on the app's stdin. The app owns the curve libraries; Studio only reads the
// project and writes automation.
//
// Curve libraries live with the tool, per machine:
//   <FMOD per-user folder>/Tools/AutomationCurveLibrary/Libraries/<Library>/
// "Factory" ships with the tool and is read-only.
//
// Apply targets come from Studio's selection (deck, editor, browser): a shared
// effect, an effect insert on an event track, bus, return or master, an
// instrument, a track or bus volume, an event's master volume, or a selected
// automation curve/point (which also picks its property and parameter). Every
// shared effect in the project is offered too.
//
// History: started as "Curve Presets for 2.03" (scripts 0.1-0.3). 0.4.0 renamed the tool, moved libraries out of projects
// into the tool, dropped the usage log, and widened the apply targets.
// 0.5.0: every automatable property per object type (generated from the
// schema, with Studio's labels), ranges looked up through parent types,
// timeline length from instruments, markers and loop regions, a structured
// description of each target (unique or shared, event, track), and a light
// selection-only snapshot so polling doesn't slow Studio down.
// 0.6.0: Studio's ScriptProcess.writeStandardInput(text, timeout) writes and
// then WAITS up to `timeout` for the process to print something ("Timeout
// waiting for process output" otherwise). 0.5 passed 1000 ms and the app
// never answered a reply, so every reply froze Studio for a second. The app
// now prints an empty line for every reply it reads, and the timeout is 100
// ms as a backstop. Also: track names come from the track's mixer group (a
// GroupTrack has no name), and an event selected in the Mixer is labelled
// as that event's mixer input.
// 0.7.0: `removeCurve` (the app's "Undo last apply" when the apply created a
// new curve): deletes that curve, and its automator if nothing is left on it.
// 0.8.0: runs in 2.02 and 2.04 as well. 2.02 has the same scripting API, but
// its effects lack 2.03's wet/dry properties, so properties an object doesn't
// have are left out. 2.04 lets mixer targets use local parameters, and has
// project curve presets (CurvePreset, points 0-1 like the libraries): the
// `presets` and `writePresets` commands read and write them.
// 0.8.1: the selection comes from the most specific of deck, editor and
// browser, not all three (an instrument no longer drags in its event).
// 0.8.2: spatializers report their built-in distance attenuation (`spatial`).
// 0.8.3: part of the DAT-audio Utilities pack (loaded from its subfolder), so
// Tools/ is found next to the nearest enclosing Scripts folder, and the menu
// item moved into the pack's submenu.
// 0.8.4: Windows. The Tools folder holds both apps: curve_library (macOS) and
// curve_library.exe (Windows); the script starts the one for the platform
// Studio is running on, and names it if it's missing.
// 0.8.5: a selected Fader stands for its strip's Volume (Studio automates the
// bus, not the effect). On 2.02 the Fader has no properties of its own, so
// selecting it offered nothing to apply to.
//
// Menu: Scripts > DAT-audio Utilities > Open Automation Curve Library...

var ACL_VERSION = "0.8.5";
var ACL_TITLE = "Automation Curve Library";
var aclScriptDir = __dirname;       // valid only while this file loads

studio.menu.addMenuItem({
    name: "DAT-audio Utilities\\Open Automation Curve Library...",
    execute: function () { var m = aclLaunch(); if (m.error) alert(m.error); }
});

// Suggested value ranges per property, from the 2.03 manual's Effect Reference
// and Instrument Reference ("-oo dB" is -80). Units are the model's, which
// match the display except where noted. "*" applies to any target type.
var ACL_DB = [-80, 10, "dB"];
var ACL_RANGES = {
    "*": { volume: ACL_DB },
    Sound: { volume: ACL_DB, pitch: [-24, 24, "semitones"] },
    GainEffect: { gain: ACL_DB },
    ThreeEQEffect: { lowGain: ACL_DB, midGain: ACL_DB, highGain: ACL_DB, lowCrossover: [10, 22000, "Hz"], highCrossover: [10, 22000, "Hz"] },
    ChorusEffect: { rate: [0, 20, "Hz"], depth: [0, 100, "%"], mix: [0, 100, "%"] },
    CompressorEffect: { threshold: [-60, 0, "dB"], ratio: [1, 50, ":1"], attackTime: [0.1, 500, "ms"], releaseTime: [10, 5000, "ms"], gain: [-30, 30, "dB"] },
    ConvolutionReverbEffect: { wetLevel: ACL_DB, dryLevel: ACL_DB },
    DelayEffect: { delay: [1, 5000, "ms"], feedback: [0, 100, "%"], level: ACL_DB, dryLevel: ACL_DB },
    DistortionEffect: { level: [0, 1, "0-1 = 0-100%"] },
    FlangerEffect: { rate: [0, 20, "Hz"], delay: [0.01, 1, "ms"], mix: [0, 100, "%"] },
    LimiterEffect: { boost: [0, 12, "dB"], ceiling: [-12, 0, "dB"], release: [1, 1000, "ms"] },
    MultibandEqEffect: {
        frequencyA: [20, 22000, "Hz"], frequencyB: [20, 22000, "Hz"], frequencyC: [20, 22000, "Hz"], frequencyD: [20, 22000, "Hz"], frequencyE: [20, 22000, "Hz"],
        qualityA: [0.1, 10, "Q"], qualityB: [0.1, 10, "Q"], qualityC: [0.1, 10, "Q"], qualityD: [0.1, 10, "Q"], qualityE: [0.1, 10, "Q"],
        gainA: [-30, 30, "dB"], gainB: [-30, 30, "dB"], gainC: [-30, 30, "dB"], gainD: [-30, 30, "dB"], gainE: [-30, 30, "dB"]
    },
    PannerEffect: { stereoPan: [-100, 100, ""], surroundPanDirection: [-180, 180, "deg"], surroundPanExtent: [0, 360, "deg"], surroundStereoAxis: [-180, 180, "deg"], surroundStereoSeparation: [-180, 180, "deg"] },
    ParamEqEffect: { centreFrequency: [20, 22000, "Hz"], gain: [-30, 30, "dB"], bandwidth: [0.2, 5, ""] },
    SFXReverbEffect: {
        decayTime: [1, 20000, "ms"], earlyDelay: [0, 300, "ms"], lateDelay: [1, 100, "ms"], HFDecayRatio: [10, 100, "%"],
        HFReference: [20, 20000, "Hz"], diffusion: [0, 100, "%"], density: [0, 100, "%"], lowShelfGain: [-36, 12, "dB"],
        lowShelfFrequency: [20, 20000, "Hz"], highCut: [20, 20000, "Hz"], earlyLateMix: [0, 100, "%"], wetLevel: ACL_DB, dryLevel: ACL_DB
    },
    SpatialiserEffect: { userPanDirection: [-180, 180, "deg"], userPanExtent: [0, 360, "deg"], userStereoAxis: [-180, 180, "deg"], userStereoSeparation: [-180, 180, "deg"] },
    SpatialEffect: { soundSize: [0, 10000, ""], minimumExtent: [0, 360, "deg"] },
    ObjectSpatialiserEffect: { soundSize: [0, 10000, ""], minimumExtent: [0, 360, "deg"] },
    TransceiverEffect: { level: ACL_DB },
    LowpassEffect: { cutoff: [10, 22000, "Hz"] },
    HighpassEffect: { cutoff: [10, 22000, "Hz"] },
    LowpassSimpleEffect: { cutoff: [10, 22000, "Hz"] },
    HighpassSimpleEffect: { cutoff: [10, 22000, "Hz"] },
    // Not in the Effect Reference tables; from the FMOD Engine's DSP docs and
    // Studio's controls. Values outside these show in the app as a warning only.
    Module: { startOffset: [0, 100, "%"] },
    MixerEffect: { wetMix: [0, 100, "%"], baseWetLevel: ACL_DB, baseDryLevel: ACL_DB, inputGain: ACL_DB },
    MixerSend: { level: ACL_DB },
    SpatialEffect: { minimumDistance: [0, 10000, ""], maximumDistance: [0, 10000, ""], soundSize: [0, 10000, ""], minimumExtent: [0, 360, "deg"] },
    MixerBusPanner: { stereoPan: [-100, 100, ""], surroundPanDirection: [-180, 180, "deg"], surroundPanExtent: [0, 360, "deg"], surroundStereoAxis: [-180, 180, "deg"], surroundStereoSeparation: [-180, 180, "deg"], surroundLFELevel: [-80, 20, "dB"] }
};
ACL_RANGES.SpatialiserEffect.userLFELevel = [-80, 20, "dB"];
ACL_RANGES.PannerEffect.surroundLFELevel = [-80, 20, "dB"];

// Studio's names for properties ("Type.prop" first, then "prop").
var ACL_LABELS = {
    volume: "Volume", pitch: "Pitch", startOffset: "Start Offset", gain: "Gain", cutoff: "Cutoff", resonance: "Resonance",
    wetMix: "Wet/Dry Mix", baseWetLevel: "Wet Level", baseDryLevel: "Dry Level", inputGain: "Input Gain", level: "Level",
    "SpatialiserEffect.minimumDistance": "Min Distance", "SpatialiserEffect.maximumDistance": "Max Distance",
    minimumDistance: "Min Distance", maximumDistance: "Max Distance", soundSize: "Sound Size", minimumExtent: "Min Extent",
    dopplerMultiplier: "Doppler Multiplier", panBlend: "Pan Override Mix", userPanDirection: "Pan Override Direction",
    userPanExtent: "Pan Override Extent (Surround Extent)", userLFELevel: "LFE", userStereoSeparation: "Pan Override Stereo Separation",
    userStereoAxis: "Pan Override Stereo Axis",
    stereoPan: "Pan", stereoLeftPan: "Left Pan", stereoRightPan: "Right Pan", surroundPanDirection: "Direction",
    surroundPanExtent: "Extent", surroundLFELevel: "LFE", surroundStereoSeparation: "Stereo Separation",
    surroundStereoAxis: "Stereo Axis", surroundHeightBlend: "Height",
    lowGain: "Low", midGain: "Mid", highGain: "High", lowCrossover: "X-Low", highCrossover: "X-High",
    threshold: "Threshold", ratio: "Ratio", attackTime: "Attack", releaseTime: "Release", boost: "Input", ceiling: "Ceiling", release: "Release",
    rate: "Rate", depth: "Depth", mix: "Mix", delay: "Delay", feedback: "Feedback", dryLevel: "Dry Level", wetLevel: "Wet Level",
    "DelayEffect.level": "Wet Level", centreFrequency: "Frequency", bandwidth: "Bandwidth",
    decayTime: "Reverb Time", earlyDelay: "Early Delay", lateDelay: "Late Delay", HFReference: "HF Reference", HFDecayRatio: "HF Decay",
    diffusion: "Diffusion", density: "Density", lowShelfFrequency: "Low Freq", lowShelfGain: "Low Gain", highCut: "High Cut",
    earlyLateMix: "Early/Late", targetValue: "Target Value", intensity: "Intensity", duration: "Duration",
    spawnRate: "Spawn Rate", volumeRandomization: "Volume Randomization", pitchRandomization: "Pitch Randomization",
    minimumTimeBetweenSounds: "Min Spawn Interval", maximumTimeBetweenSounds: "Max Spawn Interval",
    minimumScatterDistance: "Min Scatter Distance", maximumScatterDistance: "Max Scatter Distance"
};

// BEGIN GENERATED (from the 2.03.14 schema)
var ACL_PROPS = {"ChannelMixEffect":["gain00","gain01","gain02","gain03","gain04","gain05","gain06","gain07","gain08","gain09","gain10","gain11","gain12","gain13","gain14","gain15","gain16","gain17","gain18","gain19","gain20","gain21","gain22","gain23","gain24","gain25","gain26","gain27","gain28","gain29","gain30","gain31","wetMix","baseWetLevel","baseDryLevel","inputGain"],"ChorusEffect":["rate","depth","mix","wetMix","baseWetLevel","baseDryLevel","inputGain"],"CommandSound":["targetValue","volume","pitch","startOffset"],"CompressorEffect":["threshold","ratio","attackTime","releaseTime","gain","wetMix","baseWetLevel","baseDryLevel","inputGain"],"ConvolutionReverbEffect":["wetLevel","dryLevel","wetMix","baseWetLevel","baseDryLevel","inputGain"],"DelayEffect":["delay","feedback","level","dryLevel","wetMix","baseWetLevel","baseDryLevel","inputGain"],"DistortionEffect":["level","wetMix","baseWetLevel","baseDryLevel","inputGain"],"EventMixerGroup":["pitch","volume"],"EventMixerMaster":["pitch","volume"],"EventMixerReturn":["volume"],"EventSound":["volume","pitch","startOffset"],"FlangerEffect":["rate","delay","mix","wetMix","baseWetLevel","baseDryLevel","inputGain"],"GainEffect":["gain","wetMix","baseWetLevel","baseDryLevel","inputGain"],"HighpassEffect":["cutoff","resonance","wetMix","baseWetLevel","baseDryLevel","inputGain"],"HighpassSimpleEffect":["cutoff","wetMix","baseWetLevel","baseDryLevel","inputGain"],"ITEchoEffect":["wetDryMix","feedback","leftDelay","rightDelay","panDelay","wetMix","baseWetLevel","baseDryLevel","inputGain"],"LimiterEffect":["boost","ceiling","release","wetMix","baseWetLevel","baseDryLevel","inputGain"],"LoudnessMeter":["wetMix","baseWetLevel","baseDryLevel","inputGain"],"LowpassEffect":["cutoff","resonance","wetMix","baseWetLevel","baseDryLevel","inputGain"],"LowpassSimpleEffect":["cutoff","wetMix","baseWetLevel","baseDryLevel","inputGain"],"MixerBusFader":["wetMix","baseWetLevel","baseDryLevel","inputGain"],"MixerBusPanner":["stereoPan","stereoLeftPan","stereoRightPan","surroundPanDirection","surroundPanExtent","surroundLFELevel","surroundStereoSeparation","surroundStereoAxis","surroundHeightBlend","wetMix","baseWetLevel","baseDryLevel","inputGain"],"MixerGroup":["pitch","volume"],"MixerInput":["volume"],"MixerMaster":["pitch","volume"],"MixerPort":["pitch","volume"],"MixerReturn":["volume"],"MixerSend":["level","wetMix","baseWetLevel","baseDryLevel","inputGain"],"MixerVCA":["volume"],"MultiSound":["volume","pitch","startOffset"],"MultibandDynamicsEffect":["lowerFrequency","upperFrequency","gainA","gainB","gainC","thresholdA","thresholdB","thresholdC","ratioA","ratioB","ratioC","attackA","attackB","attackC","releaseA","releaseB","releaseC","gainMakeUpA","gainMakeUpB","gainMakeUpC","wetMix","baseWetLevel","baseDryLevel","inputGain"],"MultibandEqEffect":["frequencyA","frequencyB","frequencyC","frequencyD","frequencyE","qualityA","qualityB","qualityC","qualityD","qualityE","gainA","gainB","gainC","gainD","gainE","wetMix","baseWetLevel","baseDryLevel","inputGain"],"ObjectSpatialiserEffect":["minimumDistance","maximumDistance","soundSize","minimumExtent","wetMix","baseWetLevel","baseDryLevel","inputGain"],"PannerEffect":["stereoPan","stereoLeftPan","stereoRightPan","surroundPanDirection","surroundPanExtent","surroundLFELevel","surroundStereoSeparation","surroundStereoAxis","surroundHeightBlend","wetMix","baseWetLevel","baseDryLevel","inputGain"],"ParamEqEffect":["centreFrequency","gain","bandwidth","wetMix","baseWetLevel","baseDryLevel","inputGain"],"PitchShifterEffect":["pitch","wetMix","baseWetLevel","baseDryLevel","inputGain"],"PluginSound":["volume","pitch","startOffset"],"ProgrammerSound":["volume","pitch","startOffset"],"SFXReverbEffect":["decayTime","earlyDelay","lateDelay","HFReference","HFDecayRatio","diffusion","density","lowShelfFrequency","lowShelfGain","highCut","earlyLateMix","wetLevel","dryLevel","wetMix","baseWetLevel","baseDryLevel","inputGain"],"Sidechain":["level","wetMix","baseWetLevel","baseDryLevel","inputGain"],"SilenceSound":["duration","volume","pitch","startOffset"],"SingleSound":["volume","pitch","startOffset"],"SnapshotModule":["intensity","volume","pitch","startOffset"],"SoundScatterer":["minimumTimeBetweenSounds","maximumTimeBetweenSounds","spawnRate","minimumScatterDistance","maximumScatterDistance","volumeRandomization","pitchRandomization","volume","pitch","startOffset"],"SpatialEffect":["minimumDistance","maximumDistance","soundSize","minimumExtent","wetMix","baseWetLevel","baseDryLevel","inputGain"],"SpatialiserEffect":["dopplerMultiplier","panBlend","userPanDirection","userPanExtent","userLFELevel","userStereoSeparation","userStereoAxis","minimumDistance","maximumDistance","soundSize","minimumExtent","wetMix","baseWetLevel","baseDryLevel","inputGain"],"SpectralSidechain":["level","minimumFrequency","maximumFrequency","wetMix","baseWetLevel","baseDryLevel","inputGain"],"SpectrumAnalyzer":["wetMix","baseWetLevel","baseDryLevel","inputGain"],"ThreeEQEffect":["lowGain","midGain","highGain","lowCrossover","highCrossover","wetMix","baseWetLevel","baseDryLevel","inputGain"],"TransceiverEffect":["level","wetMix","baseWetLevel","baseDryLevel","inputGain"],"TremoloEffect":["frequency","depth","shape","skew","duty","square","phase","spread","wetMix","baseWetLevel","baseDryLevel","inputGain"]};
var ACL_CHAIN = {"ChannelMixEffect":["ChannelMixEffect","MixerEffect","AutomatableObject"],"ChorusEffect":["ChorusEffect","MixerEffect","AutomatableObject"],"CommandSound":["CommandSound","Sound","Module","AutomatableObject"],"CompressorEffect":["CompressorEffect","MixerEffect","AutomatableObject"],"ConvolutionReverbEffect":["ConvolutionReverbEffect","MixerEffect","AutomatableObject"],"DelayEffect":["DelayEffect","MixerEffect","AutomatableObject"],"DistortionEffect":["DistortionEffect","MixerEffect","AutomatableObject"],"EventMixerGroup":["EventMixerGroup","MixerGroup","MixerBus","MixerStrip","AutomatableObject"],"EventMixerMaster":["EventMixerMaster","MixerMaster","MixerGroup","MixerBus","MixerStrip","AutomatableObject"],"EventMixerReturn":["EventMixerReturn","MixerReturn","MixerBus","MixerStrip","AutomatableObject"],"EventSound":["EventSound","Sound","Module","AutomatableObject"],"FlangerEffect":["FlangerEffect","MixerEffect","AutomatableObject"],"GainEffect":["GainEffect","MixerEffect","AutomatableObject"],"HighpassEffect":["HighpassEffect","MixerEffect","AutomatableObject"],"HighpassSimpleEffect":["HighpassSimpleEffect","MixerEffect","AutomatableObject"],"ITEchoEffect":["ITEchoEffect","MixerEffect","AutomatableObject"],"LimiterEffect":["LimiterEffect","MixerEffect","AutomatableObject"],"LoudnessMeter":["LoudnessMeter","MixerEffect","AutomatableObject"],"LowpassEffect":["LowpassEffect","MixerEffect","AutomatableObject"],"LowpassSimpleEffect":["LowpassSimpleEffect","MixerEffect","AutomatableObject"],"MixerBusFader":["MixerBusFader","MixerEffect","AutomatableObject"],"MixerBusPanner":["MixerBusPanner","PannerEffect","MixerEffect","AutomatableObject"],"MixerGroup":["MixerGroup","MixerBus","MixerStrip","AutomatableObject"],"MixerInput":["MixerInput","MixerBus","MixerStrip","AutomatableObject"],"MixerMaster":["MixerMaster","MixerGroup","MixerBus","MixerStrip","AutomatableObject"],"MixerPort":["MixerPort","MixerGroup","MixerBus","MixerStrip","AutomatableObject"],"MixerReturn":["MixerReturn","MixerBus","MixerStrip","AutomatableObject"],"MixerSend":["MixerSend","MixerEffect","AutomatableObject"],"MixerVCA":["MixerVCA","MixerStrip","AutomatableObject"],"MultiSound":["MultiSound","Sound","Module","AutomatableObject"],"MultibandDynamicsEffect":["MultibandDynamicsEffect","MixerEffect","AutomatableObject"],"MultibandEqEffect":["MultibandEqEffect","MixerEffect","AutomatableObject"],"ObjectSpatialiserEffect":["ObjectSpatialiserEffect","SpatialEffect","MixerEffect","AutomatableObject"],"PannerEffect":["PannerEffect","MixerEffect","AutomatableObject"],"ParamEqEffect":["ParamEqEffect","MixerEffect","AutomatableObject"],"PitchShifterEffect":["PitchShifterEffect","MixerEffect","AutomatableObject"],"PluginSound":["PluginSound","Sound","Module","AutomatableObject"],"ProgrammerSound":["ProgrammerSound","Sound","Module","AutomatableObject"],"SFXReverbEffect":["SFXReverbEffect","MixerEffect","AutomatableObject"],"Sidechain":["Sidechain","MixerEffect","AutomatableObject"],"SilenceSound":["SilenceSound","Sound","Module","AutomatableObject"],"SingleSound":["SingleSound","Sound","Module","AutomatableObject"],"SnapshotModule":["SnapshotModule","EventSound","Sound","Module","AutomatableObject"],"SoundScatterer":["SoundScatterer","Sound","Module","AutomatableObject"],"SpatialEffect":["SpatialEffect","MixerEffect","AutomatableObject"],"SpatialiserEffect":["SpatialiserEffect","SpatialEffect","MixerEffect","AutomatableObject"],"SpectralSidechain":["SpectralSidechain","SpectrumAnalyzer","MixerEffect","AutomatableObject"],"SpectrumAnalyzer":["SpectrumAnalyzer","MixerEffect","AutomatableObject"],"ThreeEQEffect":["ThreeEQEffect","MixerEffect","AutomatableObject"],"TransceiverEffect":["TransceiverEffect","MixerEffect","AutomatableObject"],"TremoloEffect":["TremoloEffect","MixerEffect","AutomatableObject"]};
// END GENERATED


// ── Helpers ─────────────────────────────────────────────────────────────────

function aclLog(s) { studio.system.print("[" + ACL_TITLE + "] " + s); }

function aclList(rel) {
    var out = [], i;
    if (!rel) return out;
    for (i = 0; i < rel.length; i++) out.push(rel[i]);
    return out;
}

function aclIsType(o, t) { try { return !!o && o.isOfType(t); } catch (e) { return false; } }

function aclLookup(id) {
    if (!id) return null;
    try { return studio.project.lookup(id) || null; } catch (e) { return null; }
}

// Studio's minor version: 2 for 2.02, 3 for 2.03, 4 for 2.04.
function aclStudioMinor() {
    try { return Number(studio.version.majorVersion) || 3; } catch (e) { return 3; }
}

function aclName(o) {
    if (!o) return "";
    try { if (o.name) return "" + o.name; } catch (e) { }
    return "";
}

function aclExists(path) {
    try { return studio.system.getFile(path).exists(); } catch (e) { return false; }
}

function aclParamName(gp) {
    if (!gp) return "(no parameter)";
    if (aclIsType(gp, "Timeline")) return "Timeline";
    try { if (gp.presetOwner && gp.presetOwner.name) return "" + gp.presetOwner.name; } catch (e) { }
    return "(local parameter)";
}

function aclParamRange(gp) {
    return aclIsType(gp, "GameParameter") ? [gp.minimum, gp.maximum] : null;
}

function aclIsLabeled(gp) {
    try { return !!(gp.enumerationLabels && gp.enumerationLabels.length); } catch (e) { return false; }
}

function aclIsGlobal(gp) {
    try { return !!gp.isGlobal; } catch (e) { return false; }
}

// ── Paths ───────────────────────────────────────────────────────────────────

// Tools/ sits next to the nearest enclosing Scripts folder (the script may be
// directly in Scripts/ or in a pack's subfolder).
function aclToolsDir() {
    var d = ("" + aclScriptDir).split("\\").join("/");
    if (d.charAt(d.length - 1) === "/") d = d.substring(0, d.length - 1);
    var p = d;
    while (p.length) {
        var cut = p.lastIndexOf("/");
        if (p.substring(cut + 1).toLowerCase() === "scripts") return p.substring(0, cut + 1) + "Tools/AutomationCurveLibrary/";
        if (cut <= 0) break;
        p = p.substring(0, cut);
    }
    return d.substring(0, d.lastIndexOf("/") + 1) + "Tools/AutomationCurveLibrary/";
}

function aclIsWindows() {
    try { return ("" + studio.application.filePath).toLowerCase().indexOf(".exe") >= 0; } catch (e) { return false; }
}

// Both apps sit in the same Tools folder and share its Libraries.
function aclAppName() { return aclIsWindows() ? "curve_library.exe" : "curve_library"; }
function aclPlatform() { return aclIsWindows() ? "Windows" : "macOS"; }
function aclAppPath() { return aclToolsDir() + aclAppName(); }

function aclLibrariesDir() { return aclToolsDir() + "Libraries"; }

// ── Targets: what a curve can be applied to ─────────────────────────────────

// The event an object belongs to, or null for mixer (bus/master) objects and
// shared effects.
function aclOwnerEvent(o) {
    try {
        if (aclIsType(o, "MixerEffect")) {
            var chain = o.owner;
            o = chain ? (chain.bus || null) : null;
            if (!o) return null;
        }
        if (aclIsType(o, "EventMixerGroup") || aclIsType(o, "EventMixerMaster")) return o.groupTrack ? o.groupTrack.event : null;
        if (aclIsType(o, "MixerInput")) return o.event || null;
        if (aclIsType(o, "Module")) return o.audioTrack && o.audioTrack.event ? o.audioTrack.event : null;
        if (aclIsType(o, "EventAutomatableProperties")) return o.event || null;
    } catch (e) { }
    return null;
}

// A track's name lives on its mixer group (GroupTrack has no name property).
function aclTrackName(track) {
    try { if (track && track.mixerGroup && track.mixerGroup.name) return "" + track.mixerGroup.name; } catch (e) { }
    try { if (aclIsType(track, "MasterTrack")) return "Master"; } catch (e) { }
    return aclName(track) || "Audio track";
}

// A readable place for a mixer strip or effect: "Track 'Engine' in event 'Car'".
function aclWhere(o) {
    var ev = aclOwnerEvent(o), bus = o, i;
    try { if (aclIsType(o, "MixerEffect") && o.owner) bus = o.owner.bus || o; } catch (e) { }
    var strip = "";
    try {
        if (aclIsType(bus, "EventMixerMaster")) strip = "master track";
        else if (aclIsType(bus, "EventMixerGroup")) strip = "track '" + aclName(bus) + "'";
        else if (aclIsType(bus, "MixerInput")) strip = "";
        else if (aclIsType(bus, "MixerMaster")) strip = "mixer master bus";
        else if (aclIsType(bus, "MixerReturn")) strip = "return '" + aclName(bus) + "'";
        else if (aclIsType(bus, "MixerStrip")) strip = "bus '" + aclName(bus) + "'";
    } catch (e) { }
    if (aclIsType(o, "Module")) {
        try { strip = "track '" + aclTrackName(o.audioTrack) + "'"; } catch (e) { }
    }
    return (strip ? strip : "") + (ev ? (strip ? " in " : "") + "event '" + aclName(ev) + "'" : "");
}

function aclEffectLabel(fx) { return ("" + fx.entity).split("Effect").join("") || "Effect"; }

// "Single instrument 'Engine_loop'" etc. (the asset name for single instruments).
function aclInstrumentName(o) {
    var kind = ("" + o.entity).split("Sound").join("") || "Sound";
    var kinds = { Single: "Single instrument", Multi: "Multi instrument", Event: "Event instrument", Programmer: "Programmer instrument",
        Silence: "Silence instrument", Command: "Command instrument", Snapshot: "Snapshot instrument", Plugin: "Plugin instrument" };
    var name = aclName(o);
    try { if (!name && o.audioFile && o.audioFile.assetPath) name = ("" + o.audioFile.assetPath).split("/").pop(); } catch (e) { }
    return (kinds[kind] || o.entity) + (name ? " '" + name + "'" : "");
}

// Which parameters a target may be automated on.
function aclAllowedParams(target, ev, shared) {
    var out = [], seen = {}, pps = [], i, gp;
    function add(gp, inEvent) {
        if (!gp || seen[gp.id] || aclIsLabeled(gp)) return;
        seen[gp.id] = true;
        out.push({ id: gp.id, inEvent: inEvent });
    }
    if (ev) {
        try { add(ev.timeline, true); } catch (e) { }
        var ps = aclList(ev.parameters);
        for (i = 0; i < ps.length; i++) {
            try { if (aclIsType(ps[i], "ParameterProxy")) add(ps[i].preset, true); } catch (e) { }
        }
    }
    try { pps = studio.project.model.ParameterPreset.findInstances(); } catch (e) { pps = []; }
    for (i = 0; i < pps.length; i++) {
        gp = pps[i].parameter;
        if (!gp) continue;
        // Up to 2.03, mixer targets can only use global parameters (2.04 lifted it).
        if (!ev && !shared && !aclIsGlobal(gp) && aclStudioMinor() < 4) continue;
        add(gp, false);
    }
    return out;
}

function aclCurveInfo(curve) {
    var gp = curve.parameter, pr = aclParamRange(gp), raw = [], pts = aclList(curve.automationPoints), i, p;
    for (i = 0; i < pts.length; i++) {
        p = pts[i];
        raw.push({ position: p.position, value: p.value, shape: p.curveShape || 0, s: !!p.isSCurve });
    }
    raw.sort(function (a, b) { return a.position - b.position; });
    return { parameterId: gp ? gp.id : "", parameterName: aclParamName(gp), pmin: pr ? pr[0] : null, pmax: pr ? pr[1] : null, points: raw };
}

// Types this object is, most specific first (from the generated chain, else
// the entity alone).
function aclChain(obj) {
    return ACL_CHAIN[obj.entity] || [obj.entity];
}

function aclLabel(obj, prop) {
    var c = aclChain(obj), i;
    for (i = 0; i < c.length; i++) if (ACL_LABELS[c[i] + "." + prop]) return ACL_LABELS[c[i] + "." + prop];
    if (ACL_LABELS[prop]) return ACL_LABELS[prop];
    var out = "";          // camelCase → "Camel Case"
    for (i = 0; i < prop.length; i++) {
        var ch = prop.charAt(i);
        out += (i === 0) ? ch.toUpperCase() : ((ch >= "A" && ch <= "Z") ? " " + ch : ch);
    }
    return out;
}

function aclRange(obj, prop) {
    var c = aclChain(obj), i, t;
    for (i = 0; i < c.length; i++) {
        t = ACL_RANGES[c[i]];
        if (t && t[prop]) return t[prop];
    }
    if (aclIsType(obj, "Module")) { t = ACL_RANGES.Sound; if (t[prop]) return t[prop]; }
    return ACL_RANGES["*"][prop] || null;
}

// Every property a curve can go on: automated ones first, then the rest of
// the type's automatable properties.
function aclProperties(obj) {
    var out = [], seen = {}, autos = aclList(obj.automators), i, j, name;
    function add(name, automated) {
        if (seen[name]) { if (automated) seen[name].automated = true; return seen[name]; }
        var r = aclRange(obj, name);
        var info = { name: name, label: aclLabel(obj, name), automated: !!automated, curves: [] };
        if (r) { info.lo = r[0]; info.hi = r[1]; info.unit = r[2] || ""; }
        seen[name] = info;
        out.push(info);
        return info;
    }
    for (i = 0; i < autos.length; i++) {
        var info = add(autos[i].nameOfPropertyBeingAutomated, true);
        var cl = aclList(autos[i].automationCurves);
        for (j = 0; j < cl.length; j++) info.curves.push(aclCurveInfo(cl[j]));
    }
    var listed = ACL_PROPS[obj.entity] || [], old = aclStudioMinor() < 3;
    for (i = 0; i < listed.length; i++) {
        // The table comes from 2.03's schema; 2.02 lacks some (effects' wet/dry).
        if (old && typeof obj[listed[i]] === "undefined") continue;
        add(listed[i], false);
    }
    return out;
}

// The timeline's useful length: the end of its last instrument, marker, loop
// region or automation point. { length, estimated } where estimated means
// nothing was found and a default was used.
var ACL_DEFAULT_TIMELINE = 10;
function aclTimelineLength(ev) {
    var end = 0, tracks = [], i, j, list;
    try { tracks = aclList(ev.groupTracks); if (ev.masterTrack) tracks.push(ev.masterTrack); } catch (e) { }
    for (i = 0; i < tracks.length; i++) {
        list = aclList(tracks[i].modules);
        for (j = 0; j < list.length; j++) {
            try {
                if (list[j].parameter && aclIsType(list[j].parameter, "Timeline")) end = Math.max(end, (list[j].start || 0) + (list[j].length || 0));
            } catch (e) { }
        }
    }
    try {
        list = aclList(ev.timeline.markers);
        for (i = 0; i < list.length; i++) end = Math.max(end, (list[i].position || 0) + (list[i].length || 0));
    } catch (e) { }
    try {
        list = aclList(ev.timeline.automationCurves);
        for (i = 0; i < list.length; i++) {
            var pts = aclList(list[i].automationPoints);
            for (j = 0; j < pts.length; j++) end = Math.max(end, pts[j].position || 0);
        }
    } catch (e) { }
    return end > 0 ? { length: end, estimated: false } : { length: ACL_DEFAULT_TIMELINE, estimated: true };
}

// A target descriptor for the app. `obj` is the AutomatableObject the curve
// goes on. `prefer` optionally names { property, parameterId } to preselect.
var ACL_KIND_LABELS = {
    shared: "Shared effect", insert: "Effect insert", instrument: "Instrument", track: "Track",
    bus: "Mixer bus", event: "Event master track", mixerInput: "Event in the mixer"
};

// A spatializer's distance attenuation is a built-in mode, not an automatable
// property, in 2.02, 2.03 and 2.04: it can't take a custom curve (the way
// round it is attenuation Off plus a Gain automated on Distance). So it's
// reported for display: the mode, and the min/max it uses (its own when
// overridden, else its event's). distanceRolloffType's order is from
// Studio's own enum: LinearSquared 0, Linear 1, Inverse 2, InverseTapered 3,
// Custom 4 (shown as "Off").
var ACL_ROLLOFF = ["Linear Squared", "Linear", "Inverse", "Inverse Tapered", "Off"];
function aclSpatial(obj, ev, shared) {
    if (!aclIsType(obj, "SpatialEffect")) return null;
    var type = 0, own = false, min = 1, max = 20, source = "effect";
    try { type = Number(obj.distanceRolloffType) || 0; } catch (e) { }
    try { own = !!obj.overrideRange; } catch (e) { }
    try {
        if (!own && ev && ev.automatableProperties) {
            min = ev.automatableProperties.minimumDistance;
            max = ev.automatableProperties.maximumDistance;
            source = "event";
        } else {
            min = obj.minimumDistance;
            max = obj.maximumDistance;
            source = own ? "effect" : (shared ? "each event" : "effect");
        }
    } catch (e) { }
    return { rolloff: ACL_ROLLOFF[type] || ("mode " + type), rolloffType: type, min: Number(min), max: Number(max), source: source };
}

function aclTarget(obj, kind, label, prefer, shared) {
    var ev = shared ? null : aclOwnerEvent(obj);
    var tl = ev ? aclTimelineLength(ev) : null;
    var t = {
        id: obj.id, kind: kind, kindLabel: ACL_KIND_LABELS[kind] || kind, label: label, where: shared ? "" : aclWhere(obj),
        unique: !shared, eventId: ev ? ev.id : null, eventName: ev ? aclName(ev) : "",
        entity: obj.entity, properties: aclProperties(obj), parameters: aclAllowedParams(obj, ev, shared),
        timelineLength: tl ? tl.length : null, timelineEstimated: tl ? tl.estimated : false
    };
    if (prefer) t.prefer = prefer;
    var sp = aclSpatial(obj, ev, shared);
    if (sp) t.spatial = sp;
    return t;
}

function aclSharedTarget(fp) {
    var n = 0;
    try { n = fp.proxies.length; } catch (e) { }
    var t = aclTarget(fp.effect, "shared", "'" + aclName(fp) + "' (" + aclEffectLabel(fp.effect) + ")", null, true);
    t.uses = n;
    t.where = "used in " + n + " place" + (n === 1 ? "" : "s") + "; every use gets the curve";
    return t;
}

// Turns one selected object into targets.
function aclTargetsFor(o) {
    var out = [];
    try {
        if (aclIsType(o, "AutomationPoint")) o = o.automationCurve;
        if (aclIsType(o, "AutomationCurve")) {
            var au = o.automator, obj = au ? au.objectBeingAutomated : null;
            if (obj) {
                var inner = aclTargetsFor(obj);
                for (var k = 0; k < inner.length; k++) {
                    inner[k].prefer = { property: au.nameOfPropertyBeingAutomated, parameterId: o.parameter ? o.parameter.id : null };
                    inner[k].selectedCurve = aclLabel(obj, au.nameOfPropertyBeingAutomated) + " on " + aclParamName(o.parameter);
                }
                return inner;
            }
            return out;
        }
        if (aclIsType(o, "ProxyEffect") && o.preset && o.preset.effect) return [aclSharedTarget(o.preset)];
        if (aclIsType(o, "EffectPreset") && o.effect) return [aclSharedTarget(o)];
        // The Fader's Volume knob is its strip's volume (Studio automates the
        // bus, not the effect), and the Fader itself has nothing else on 2.02
        // (its wet/dry properties are new in 2.03). So it stands for its strip.
        if (aclIsType(o, "MixerBusFader") && o.owner && o.owner.bus) {
            var strip = aclTargetsFor(o.owner.bus);
            for (var f = 0; f < strip.length; f++) {
                strip[f].label += " (Fader)";
                if (!strip[f].prefer) strip[f].prefer = { property: "volume", parameterId: null };
            }
            return strip;
        }
        if (aclIsType(o, "MixerEffect") && !aclIsType(o, "EffectChain")) {
            if (o.presetOwner && aclIsType(o.presetOwner, "EffectPreset")) return [aclSharedTarget(o.presetOwner)];
            return [aclTarget(o, "insert", aclEffectLabel(o), null, false)];
        }
        if (aclIsType(o, "Sound")) return [aclTarget(o, "instrument", aclInstrumentName(o), null, false)];
        if (aclIsType(o, "MasterTrack") && o.mixerGroup) return [aclTarget(o.mixerGroup, "event", "Master track", null, false)];
        if (aclIsType(o, "GroupTrack") && o.mixerGroup) return [aclTarget(o.mixerGroup, "track", "'" + aclTrackName(o) + "'", null, false)];
        if (aclIsType(o, "EventMixerMaster")) return [aclTarget(o, "event", "Master track", null, false)];
        if (aclIsType(o, "EventMixerGroup")) return [aclTarget(o, "track", "'" + aclName(o) + "'", null, false)];
        if (aclIsType(o, "MixerInput")) {
            var t = aclTarget(o, "mixerInput", "'" + aclName(o.event) + "'", null, false);
            var out = "";
            try { out = o.output ? aclName(o.output) || (aclIsType(o.output, "MixerMaster") ? "Master Bus" : "") : ""; } catch (e) { }
            t.where = "the event's level where it enters the mixer" + (out ? ", routed into '" + out + "'" : "");
            return [t];
        }
        if (aclIsType(o, "MixerStrip")) return [aclTarget(o, "bus", "'" + (aclName(o) || o.entity) + "'", null, false)];
        if (aclIsType(o, "Event") && o.masterTrack && o.masterTrack.mixerGroup) {
            return [aclTarget(o.masterTrack.mixerGroup, "event", "Master track", null, false)];
        }
    } catch (e) {
        aclLog("Couldn't read a selected " + (o ? o.entity : "object") + ": " + e);
    }
    return out;
}

// The most specific of Studio's selections wins: the deck (an effect), then
// the editor (instruments, tracks), then the browser (events). They're not
// merged: an event stays highlighted in the Events browser while you select
// an instrument in it, and that isn't a second thing to apply to (0.8.1).
function aclSelectionTargets() {
    var out = [], seen = {}, sources = [], i, j, list, ts;
    try { sources.push(studio.window.deckSelection() || []); } catch (e) { }
    try { sources.push(studio.window.editorSelection() || []); } catch (e) { }
    try { sources.push(studio.window.browserSelection() || []); } catch (e) { }
    for (i = 0; i < sources.length && out.length === 0; i++) {
        list = sources[i];
        for (j = 0; j < list.length && out.length < 20; j++) {
            ts = aclTargetsFor(list[j]);
            for (var k = 0; k < ts.length; k++) {
                var key = ts[k].id + "|" + (ts[k].prefer ? ts[k].prefer.property + "|" + ts[k].prefer.parameterId : "");
                if (seen[key]) continue;
                seen[key] = true;
                out.push(ts[k]);
            }
        }
    }
    return out;
}

// The selection part is cheap and polled often; shared effects only when the
// app asks (args.shared), every few seconds.
function aclSnapshot(withShared) {
    var pps = [], params = [], shared = [], i, gp, fps = [];
    try { pps = studio.project.model.ParameterPreset.findInstances(); } catch (e) { pps = []; }
    for (i = 0; i < pps.length; i++) {
        gp = pps[i].parameter;
        if (!gp || aclIsLabeled(gp)) continue;
        params.push({ id: gp.id, name: "" + pps[i].name, min: gp.minimum, max: gp.maximum, global: aclIsGlobal(gp) });
    }
    // Event timelines named by the selection targets.
    var sel = aclSelectionTargets(), seen = {};
    for (i = 0; i < sel.length; i++) {
        var ev = sel[i].eventId ? aclLookup(sel[i].eventId) : null;
        if (ev && ev.timeline && !seen[ev.timeline.id]) {
            seen[ev.timeline.id] = true;
            var tl = aclTimelineLength(ev);
            params.push({ id: ev.timeline.id, name: "Timeline ('" + aclName(ev) + "')", min: 0, max: tl.length, timeline: true, estimated: tl.estimated });
        }
    }
    if (withShared) {
        try { fps = studio.project.model.EffectPreset.findInstances(); } catch (e) { fps = []; }
        for (i = 0; i < fps.length; i++) if (fps[i].effect) shared.push(aclSharedTarget(fps[i]));
        shared.sort(function (a, b) { return a.label.toLowerCase() < b.label.toLowerCase() ? -1 : 1; });
    }
    return { selection: sel, sharedEffects: withShared ? shared : null, parameters: params };
}

// ── Applying ────────────────────────────────────────────────────────────────

function aclSetProp(obj, name, value) {
    try { obj.properties[name].setValue(value); } catch (e) { obj[name] = value; }
}

function aclFindAutomator(obj, prop) {
    var list = aclList(obj.automators);
    for (var i = 0; i < list.length; i++) if (list[i].nameOfPropertyBeingAutomated === prop) return list[i];
    return null;
}

function aclFindCurve(automator, param) {
    if (!automator || !param) return null;
    var list = aclList(automator.automationCurves);
    for (var i = 0; i < list.length; i++) if (list[i].parameter && list[i].parameter.id === param.id) return list[i];
    return null;
}

// Writes 0-1 points onto obj.prop's automation on `param`, stretched over the
// ranges. Reuses existing points in place (so the curve is never empty), adds
// or deletes the difference, and copies bend and S-curve exactly. Adds a
// project parameter to the owning event if it isn't there yet.
// Returns { ok, curve, replaced, created, addedParameter } or { ok: false, error }.
function aclApply(points, obj, prop, param, rg) {
    var au, curve, created = false, addedParameter = false, i, old, want = [];
    var ev = aclOwnerEvent(obj);
    if (ev && aclIsType(param, "GameParameter")) {
        var has = false, ps = aclList(ev.parameters);
        for (i = 0; i < ps.length; i++) { try { if (ps[i].preset && ps[i].preset.id === param.id) has = true; } catch (e) { } }
        if (!has) {
            try { ev.addGameParameter(param); addedParameter = true; } catch (e) { return { ok: false, error: "couldn't add parameter " + aclParamName(param) + " to the event: " + e }; }
        }
    }
    au = aclFindAutomator(obj, prop);
    if (!au) {
        try { au = obj.addAutomator(prop); } catch (e) { return { ok: false, error: "couldn't automate '" + prop + "': " + e }; }
        if (!au) return { ok: false, error: "couldn't automate '" + prop + "'" };
        created = true;
    }
    curve = aclFindCurve(au, param);
    if (!curve) {
        try { curve = au.addAutomationCurve(param); } catch (e) { return { ok: false, error: "couldn't add a curve on " + aclParamName(param) + ": " + e }; }
        if (!curve) return { ok: false, error: "couldn't add a curve on " + aclParamName(param) };
        created = true;
    }
    for (i = 0; i < points.length; i++) {
        want.push({
            position: rg.plo + points[i].x * (rg.phi - rg.plo),
            value: rg.vlo + points[i].y * (rg.vhi - rg.vlo),
            shape: points[i].shape || 0, s: !!points[i].s
        });
    }
    old = aclList(curve.automationPoints);
    old.sort(function (a, b) { return a.position - b.position; });
    var replaced = old.length;
    for (i = 0; i < want.length; i++) {
        var ap = i < old.length ? old[i] : null;
        if (!ap) {
            try { ap = curve.addAutomationPoint(want[i].position, want[i].value); } catch (e) { return { ok: false, error: "couldn't add a point: " + e }; }
        }
        aclSetProp(ap, "position", want[i].position);
        aclSetProp(ap, "value", want[i].value);
        aclSetProp(ap, "curveShape", want[i].shape);
        aclSetProp(ap, "isSCurve", want[i].s);
    }
    for (i = want.length; i < old.length; i++) {
        try { studio.project.deleteObject(old[i]); } catch (e) { }
    }
    var count = curve.automationPoints.length;
    if (count !== want.length) return { ok: false, curve: curve, error: "the curve has " + count + " points after applying, expected " + want.length };
    return { ok: true, curve: curve, replaced: created ? 0 : replaced, created: created, addedParameter: addedParameter };
}

// ── 2.04 curve presets ──────────────────────────────────────────────────────
// Presets live in the project (Metadata/CurvePreset), in CurvePresetFolders
// under the workspace's master folder. Their points are 0-1, like a library's.

function aclHasPresets() {
    try { return !!studio.project.model.CurvePreset && !!studio.project.workspace.masterCurvePresetFolder; } catch (e) { return false; }
}

// A preset's folder names, outermost first (the master folder isn't named).
function aclPresetPath(o) {
    var names = [], f = null, guard = 0;
    try { f = o.folder; } catch (e) { f = null; }
    while (f && !aclIsType(f, "MasterCurvePresetFolder") && guard++ < 64) {
        names.unshift(aclName(f));
        try { f = f.folder; } catch (e) { f = null; }
    }
    return names;
}

function aclPresets() {
    var all = studio.project.model.CurvePreset.findInstances(), out = [], i, j;
    for (i = 0; i < all.length; i++) {
        var pr = all[i], pts = aclList(pr.automationPoints), raw = [];
        for (j = 0; j < pts.length; j++) raw.push({ x: pts[j].position, y: pts[j].value, shape: pts[j].curveShape || 0, s: !!pts[j].isSCurve });
        raw.sort(function (a, b) { return a.x - b.x; });
        out.push({ id: pr.id, name: aclName(pr), folder: aclPresetPath(pr), points: raw });
    }
    return out;
}

// The preset folder at `path` (names, outermost first), made if missing.
function aclPresetFolder(path) {
    var f = studio.project.workspace.masterCurvePresetFolder, i, j;
    for (i = 0; i < path.length; i++) {
        var kids = studio.project.model.CurvePresetFolder.findInstances(), next = null;
        for (j = 0; j < kids.length; j++) {
            if (kids[j].folder && kids[j].folder.id === f.id && aclName(kids[j]) === path[i]) { next = kids[j]; break; }
        }
        if (!next) {
            next = studio.project.create("CurvePresetFolder");
            next.name = path[i];
            next.folder = f;
        }
        f = next;
    }
    return f;
}

// Writes curves as presets into the folder at `path`; a preset of the same
// name there is replaced. Returns { created, replaced }.
function aclWritePresets(path, curves) {
    var folder = aclPresetFolder(path), all = studio.project.model.CurvePreset.findInstances(), made = 0, replaced = 0, i, j;
    for (i = 0; i < curves.length; i++) {
        var c = curves[i], pr = null;
        for (j = 0; j < all.length; j++) {
            if (all[j].folder && all[j].folder.id === folder.id && aclName(all[j]) === c.name) { pr = all[j]; break; }
        }
        if (pr) {
            var old = aclList(pr.automationPoints);
            for (j = 0; j < old.length; j++) studio.project.deleteObject(old[j]);
            replaced++;
        } else {
            pr = studio.project.create("CurvePreset");
            pr.name = c.name;
            pr.folder = folder;
            made++;
        }
        for (j = 0; j < c.points.length; j++) {
            var q = c.points[j], ap = studio.project.create("AutomationPoint");
            ap.position = q.x;
            ap.value = q.y;
            ap.curveShape = q.shape || 0;
            ap.isSCurve = !!q.s;
            ap.automationCurve = pr;
        }
    }
    return { created: made, replaced: replaced };
}

// ── Link ────────────────────────────────────────────────────────────────────

var ACL = { proc: null, buf: "", handled: 0, ended: false, lastCmd: "" };

function aclHandle(req) {
    var a = req.args || {};
    try {
        if (req.cmd === "hello") {
            var path = "";
            try { path = "" + (studio.project.filePath || ""); } catch (e) { }
            return { id: req.id, ok: true, result: {
                studio: studio.version.productVersion + "." + studio.version.majorVersion + "." + studio.version.minorVersion,
                project: path, script: ACL_VERSION, presets: aclHasPresets() } };
        }
        if (req.cmd === "snapshot") return { id: req.id, ok: true, result: aclSnapshot(a.shared !== false) };
        if (req.cmd === "apply") {
            var obj = aclLookup(a.targetId), param = aclLookup(a.parameterId), rg = a.range || {};
            if (!obj || !aclIsType(obj, "AutomatableObject")) return { id: req.id, ok: false, error: "that target no longer exists" };
            if (!param || !(aclIsType(param, "GameParameter") || aclIsType(param, "Timeline"))) return { id: req.id, ok: false, error: "that parameter no longer exists" };
            if (!a.property || !(a.points instanceof Array) || a.points.length < 2) return { id: req.id, ok: false, error: "bad apply request" };
            if (!(rg.phi > rg.plo) || rg.vhi === rg.vlo) return { id: req.id, ok: false, error: "bad ranges" };
            var res = aclApply(a.points, obj, a.property, param, rg);
            if (!res.ok) return { id: req.id, ok: false, error: res.error };
            aclLog("Applied a curve to " + (a.label || obj.entity) + " / " + a.property + " on " + aclParamName(param));
            return { id: req.id, ok: true, result: { created: !!res.created, replaced: res.replaced || 0, addedParameter: !!res.addedParameter, curve: aclCurveInfo(res.curve) } };
        }
        if (req.cmd === "presets") {
            if (!aclHasPresets()) return { id: req.id, ok: false, error: "curve presets need FMOD Studio 2.04 or later" };
            return { id: req.id, ok: true, result: { presets: aclPresets() } };
        }
        if (req.cmd === "writePresets") {
            if (!aclHasPresets()) return { id: req.id, ok: false, error: "curve presets need FMOD Studio 2.04 or later" };
            if (!(a.curves instanceof Array) || !(a.folder instanceof Array)) return { id: req.id, ok: false, error: "bad writePresets request" };
            var wr = aclWritePresets(a.folder, a.curves);
            aclLog("Wrote " + a.curves.length + " curve preset(s) to " + (a.folder.join("/") || "the top level"));
            return { id: req.id, ok: true, result: wr };
        }
        if (req.cmd === "removeCurve") {
            var o = aclLookup(a.targetId), prm = aclLookup(a.parameterId);
            if (!o || !aclIsType(o, "AutomatableObject")) return { id: req.id, ok: false, error: "that target no longer exists" };
            var au = aclFindAutomator(o, a.property), cv = aclFindCurve(au, prm);
            if (!cv) return { id: req.id, ok: true, result: { removed: false } };
            studio.project.deleteObject(cv);
            if (au && aclList(au.automationCurves).length === 0) { try { studio.project.deleteObject(au); } catch (e) { } }
            aclLog("Removed a curve from " + (a.label || o.entity) + " / " + a.property + " on " + aclParamName(prm) + " (undo)");
            return { id: req.id, ok: true, result: { removed: true } };
        }
        return { id: req.id, ok: false, error: "unknown command " + req.cmd };
    } catch (e) {
        return { id: req.id, ok: false, error: "" + e };
    }
}

// Reads the app's requests and answers them. Never reads a finished process:
// that raises a script error try/catch can't stop.
function aclPoll(w) {
    var proc = ACL.proc, running = false;
    if (!proc || ACL.ended) return;
    try { running = proc.isRunning(); } catch (e) { running = false; }
    if (!running) {
        ACL.ended = true;
        ACL.proc = null;
        w.findWidget("acl_status").setText("The Curve Library app was closed. You can close this window.");
        return;
    }
    var chunk = "", err = "";
    try { chunk = "" + (proc.readAllStandardOutput() || ""); } catch (e) { chunk = ""; }
    try { err = "" + (proc.readAllStandardError() || ""); } catch (e) { err = ""; }
    if (err) aclLog("Curve Library: " + err.split("\n").join(" ").trim());
    if (chunk) ACL.buf += chunk;
    var nl, line, msg, reply;
    while ((nl = ACL.buf.indexOf("\n")) >= 0) {
        line = ACL.buf.substring(0, nl).trim();
        ACL.buf = ACL.buf.substring(nl + 1);
        if (!line || line.charAt(0) !== "{") continue;
        try { msg = JSON.parse(line); } catch (e) { aclLog("Unreadable line from the app: " + line.substring(0, 200)); continue; }
        reply = aclHandle(msg);
        ACL.handled++;
        ACL.lastCmd = msg.cmd;
        // The app acknowledges each reply with an empty line, which ends this
        // call's wait for output at once (see the 0.6.0 note at the top).
        try { proc.writeStandardInput(JSON.stringify(reply) + "\n", 100); } catch (e) { aclLog("Couldn't answer the app: " + e); }
    }
    if (ACL.handled) {
        w.findWidget("acl_status").setText("<b>Linked.</b> Keep this window open while you use the Curve Library. " +
            "<span style=\"color:#8a8a8a\">" + ACL.handled + " requests answered.</span>");
    }
}

function aclOpenLinkWindow() {
    var T = studio.ui.widgetType, L = studio.ui.layoutType;
    studio.ui.showModelessDialog({
        windowTitle: "Curve Library link",
        windowWidth: 380, windowHeight: 120,
        widgetType: T.Layout, layout: L.VBoxLayout,
        items: [
            { widgetType: T.Label, widgetId: "acl_status", wordWrap: true, text: "Starting the Curve Library…" },
            { widgetType: T.Label, wordWrap: true, text: "<span style=\"color:#8a8a8a\">This window carries the link between Studio and the " +
                "Curve Library app. Closing it unlinks the app; the libraries still work.</span>" },
            {
                // Last widget, so the status label exists. Owns the polling timer.
                widgetType: T.Spacer,
                onConstructed: function () { try { this.startTimer(40); } catch (e) { aclLog("Link timer failed: " + e); } },
                onTimerEvent: function () { aclPoll(this); }
            }
        ]
    });
}

function aclRunning() {
    try { return !!ACL.proc && ACL.proc.isRunning(); } catch (e) { return false; }
}

// Starts the app linked to Studio. Returns { ok } or { error }.
function aclLaunch() {
    if (aclRunning()) return { error: "The Curve Library is already open (look for its window)." };
    var exe = aclAppPath();
    if (!aclExists(exe)) {
        return { error: "The Curve Library app for " + aclPlatform() + " (" + aclAppName() + ") isn't installed. Expected it at:\n" + exe +
            "\n\nSee Tools/AutomationCurveLibrary/README.md for how to install it." };
    }
    try {
        ACL.proc = studio.system.startAsync(exe, { args: ["--bridge", "--libraries", aclLibrariesDir()], workingDir: aclToolsDir() });
    } catch (e) {
        return { error: "Couldn't start the Curve Library: " + e };
    }
    if (!ACL.proc) return { error: "Couldn't start the Curve Library (no process)." };
    ACL.buf = ""; ACL.handled = 0; ACL.ended = false;
    aclLog("v" + ACL_VERSION + " (" + aclPlatform() + "): started " + exe);
    aclOpenLinkWindow();
    return { ok: true };
}
