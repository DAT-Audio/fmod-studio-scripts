// FMOD Studio Script — Bank Assignment Check
// Version: 1.1.0
// Validates whether events appear to be assigned to a suitable soundbank
// based on their folder path hierarchy.
// Compatible with FMOD Studio 2.02.x and 2.03.x
//
//
// Part of the DAT-audio Utilities pack — loaded and run by "Audit & Detect.js"
// in the folder above this one.
// Run from: Scripts > DAT-audio Utilities > Audit & Detect... (Run button)

"use strict";

// ─────────────────────────────────────────────────────────────────────────────
// CONFIGURATION
// Extend these tables to match your project's naming conventions.
// ─────────────────────────────────────────────────────────────────────────────

// Each rule: if ANY of pathKeywords appear in the event's folder path (lowercased),
// at least one assigned bank name must match bankKeywords OR exemptBankKeywords.
// If only exemptBankKeywords match (e.g. a DLC bank for a Music event), an INFO
// suggestion is raised instead of a WARN.
// Rules are tested in order — first match wins, so put more specific rules first.
var RULES = [
    {
        label:             "Music",
        pathKeywords:      ["music", "/mx/", "/mus/"],
        bankKeywords:      ["music", "mx", "mus", "soundtrack", "ost"],
        exemptBankKeywords: ["dlc", "ep", "pack", "expansion", "season"],
        notes: "Music events should generally be in a Music/MX/OST bank, " +
               "or a clearly DLC-scoped bank if the event path is also under that DLC hierarchy."
    },
    {
        label:             "Voice Over",
        pathKeywords:      ["voiceover", "voice_over", "voover", "/vo/", "/dia/", "dialogue"],
        bankKeywords:      ["vo", "voice", "dialogue", "dia", "speech", "loc"],
        exemptBankKeywords: ["dlc", "ep", "pack", "expansion", "season"],
        notes: "VO/Dialogue events should be in a VO or Dialogue bank."
    },
    {
        label:             "SFX",
        pathKeywords:      ["/sfx/", "/sfx"],
        bankKeywords:      ["sfx", "soundeffect", "sound_effect", "fx"],
        exemptBankKeywords: ["ui", "ux", "interface", "menu", "dlc", "ep", "pack", "expansion"],
        notes: "SFX events should be in an SFX bank (UI/UX banks are also acceptable)."
    },
    {
        label:             "UX / UI",
        pathKeywords:      ["/ux/", "/ui/", "/interface/", "/menu/"],
        bankKeywords:      ["ux", "ui", "interface", "menu", "sfx", "fx"],
        exemptBankKeywords: ["dlc", "ep", "pack", "expansion"],
        notes: "UX/UI events are commonly placed in an SFX bank — this is normal. " +
               "Flag raised if assigned to something clearly unrelated (e.g. Music or VO)."
    }
];

// Path segments that imply a DLC scope. If the event path AND at least one
// of its bank names both contain one of these segments, the event is globally
// exempt from all category warnings (DLC bank matches DLC path = intentional).
var DLC_PATH_PATTERNS = ["dlc", "/ep/", "/pack/", "/expansion/", "/season/"];

// ─────────────────────────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────────────────────────

function normalize(str) {
    return str.toLowerCase().replace(/[\s_\-\.]/g, "");
}

// True if normalized haystack contains any of the (also normalized) keywords.
function containsAny(haystack, keywords) {
    var h = normalize(haystack);
    for (var i = 0; i < keywords.length; i++) {
        if (h.indexOf(normalize(keywords[i])) !== -1) return true;
    }
    return false;
}

// Returns bank names for an event using event.banks
function getBankNamesForEvent(event) {
    var names = [];
    try {
        var banks = event.banks;
        for (var i = 0; i < banks.length; i++) {
            var b = banks[i];
            if (b && b.name) names.push(b.name);
        }
    } catch (e) {
        // event.banks not available — skip
    }
    return names;
}

// Collect all events recursively from a folder.
function collectEvents(folder, out) {
    folder.items.forEach(function (item) {
        if (item.isOfExactType("Event")) {
            out.push(item);
        } else if (item.isOfExactType("EventFolder")) {
            collectEvents(item, out);
        }
    });
}

// Build a lookup of parentEventId -> { childEvent } using EventSound instruments.
// EventSounds don't have audioTrack directly — it's on their owner module:
// EventSound.owner.audioTrack.event = the parent event containing this instrument.
function buildEventSoundMap() {
    var map = {};  // parentId -> { parentEvent, childIds: { childId: childEvent } }
    try {
        var allSounds = studio.project.model.EventSound.findInstances();
        for (var i = 0; i < allSounds.length; i++) {
            try {
                var s          = allSounds[i];
                var childEvent = s.event;
                if (!childEvent) continue;

                // Path 1: s.owner.audioTrack.event (nested inside a module/instrument)
                var parentEvent = null;

                // Walk up the owner chain until we find a node with audioTrack.event.
                // This handles any depth of nesting: direct module, MultiSound, etc.
                try {
                    var node = s;
                    for (var depth = 0; depth < 8; depth++) {
                        try {
                            if (node.audioTrack && node.audioTrack.event) {
                                parentEvent = node.audioTrack.event;
                                break;
                            }
                        } catch(e) {}
                        try {
                            if (node.owner) { node = node.owner; continue; }
                        } catch(e) {}
                        break;
                    }
                } catch(e) {}

                if (!parentEvent) continue;
                if (!map[parentEvent.id]) map[parentEvent.id] = { event: parentEvent, childIds: {} };
                map[parentEvent.id].childIds[childEvent.id] = childEvent;
            } catch(e) { /* skip bad entries */ }
        }
    } catch(e) { /* EventSound.findInstances not available */ }
    return map;
}

// Recursively mark an event and everything it references as covered.
function markCovered(eventId, soundMap, coveredIds, visited) {
    if (visited[eventId]) return;
    visited[eventId] = true;
    coveredIds[eventId] = true;
    var entry = soundMap[eventId];
    if (!entry) return;
    for (var childId in entry.childIds) {
        markCovered(childId, soundMap, coveredIds, visited);
    }
}

// Build a set of event IDs covered by being referenced (directly or transitively)
// inside any event that has at least one bank assignment.
function buildReferencedEventIdSet(allEvents) {
    var soundMap   = buildEventSoundMap();
    var coveredIds = {};
    var visited    = {};
    for (var i = 0; i < allEvents.length; i++) {
        var event = allEvents[i];
        if (getBankNamesForEvent(event).length > 0) {
            // Walk all children of this banked event recursively
            var entry = soundMap[event.id];
            if (!entry) continue;
            for (var childId in entry.childIds) {
                markCovered(childId, soundMap, coveredIds, visited);
            }
        }
    }
    return coveredIds;
}

// Check if the event's path AND one of its bank names share a DLC segment.
function hasDlcBankMatch(eventPath, bankNames) {
    var epNorm = normalize(eventPath);
    for (var i = 0; i < DLC_PATH_PATTERNS.length; i++) {
        var seg = normalize(DLC_PATH_PATTERNS[i]);
        if (epNorm.indexOf(seg) !== -1) {
            for (var j = 0; j < bankNames.length; j++) {
                if (normalize(bankNames[j]).indexOf(seg) !== -1) return true;
            }
        }
    }
    return false;
}

// ─────────────────────────────────────────────────────────────────────────────
// MAIN
// ─────────────────────────────────────────────────────────────────────────────

// Shown in the window title / console output; see the pack's top-level README.
var BNK_VERSION = "1.1.0";

// Exported to "Audit & Detect.js", which shows the title and purpose and
// a status line from `selection`, and calls execute() from the Run button.
module.exports = {
    title:     "Audit Events vs. Bank Assignments",
    version:   BNK_VERSION,
    selection: "project",   // "project", "events" or "assets"
    purpose:   "Checks each event is in a bank that suits its folder path (Music, VO, SFX, UI keywords), and flags events in no bank. The rules are a table at the top of its script.",
    execute: function () {

        var allEvents = [];
        collectEvents(studio.project.workspace.masterEventFolder, allEvents);

        // Pre-build the set of events covered by being referenced in a banked parent
        var referencedCoveredIds = buildReferencedEventIdSet(allEvents);

        var findings  = [];
        var okCount   = 0;
        var skipCount = 0;

        allEvents.forEach(function (event) {
            // Skip events that are referenced inside a parent that has bank assignments
            if (referencedCoveredIds[event.id]) {
                skipCount++;
                return;
            }
            var eventPath  = event.getPath();
            var bankNames  = getBankNamesForEvent(event);
            var banksStr   = bankNames.length ? bankNames.join(", ") : "(none)";

            // ── 1. No bank assigned ──────────────────────────────────────────
            if (bankNames.length === 0) {
                findings.push({
                    path: eventPath, banks: banksStr,
                    msg:  "Event is not assigned to any bank.",
                    sev:  "WARN"
                });
                return;
            }

            // ── 2. DLC path + matching DLC bank — globally exempt ────────────
            if (hasDlcBankMatch(eventPath, bankNames)) {
                okCount++;
                return;
            }

            // ── 3. Category rules ────────────────────────────────────────────
            var ruleMatched = false;
            for (var r = 0; r < RULES.length; r++) {
                var rule = RULES[r];
                if (!containsAny(eventPath, rule.pathKeywords)) continue;
                ruleMatched = true;

                var bankOk     = containsAny(banksStr, rule.bankKeywords);
                var bankExempt = rule.exemptBankKeywords.length > 0 &&
                                 containsAny(banksStr, rule.exemptBankKeywords);

                if (bankOk) {
                    okCount++;
                } else if (bankExempt) {
                    // Acceptable but not ideal — suggest a better bank
                    okCount++;
                    findings.push({
                        path: eventPath, banks: banksStr,
                        msg:  rule.label + " event is in an acceptable but non-ideal bank. " +
                              "Consider whether a dedicated " + rule.label + " bank (" +
                              rule.bankKeywords.join(" / ") + ") would be more appropriate. " +
                              rule.notes,
                        sev:  "INFO"
                    });
                } else {
                    findings.push({
                        path: eventPath, banks: banksStr,
                        msg:  rule.label + " event — expected a bank matching: " +
                              rule.bankKeywords.join(" / ") +
                              (rule.exemptBankKeywords.length
                                  ? " (or: " + rule.exemptBankKeywords.join(" / ") + " for DLC-scoped events)"
                                  : "") + ". " + rule.notes,
                        sev:  "WARN"
                    });
                }
                break; // First matching rule wins
            }

            if (!ruleMatched) okCount++;
        });

        // ── Build report ─────────────────────────────────────────────────────
        var warns  = findings.filter(function (f) { return f.sev === "WARN"; });
        var infos  = findings.filter(function (f) { return f.sev === "INFO"; });

        var lines = [];
        lines.push("=== Bank Assignment Check ===");
        lines.push("");
        lines.push("Events scanned : " + allEvents.length);
        lines.push("Skipped (ref'd): " + skipCount);
        lines.push("OK / exempt    : " + okCount);
        lines.push("Warnings       : " + warns.length);
        lines.push("Suggestions    : " + infos.length);
        lines.push("");

        if (findings.length === 0) {
            lines.push("No issues found. All checked events appear to be in suitable banks.");
        } else {
            if (warns.length > 0) {
                lines.push("WARNINGS (" + warns.length + ")");
                warns.forEach(function (f) {
                    lines.push("");
                    lines.push("[WARN] " + f.path);
                    lines.push("       Banks  : " + f.banks);
                    lines.push("       Reason : " + f.msg);
                });
                lines.push("");
            }
            if (infos.length > 0) {
                lines.push("SUGGESTIONS (" + infos.length + ")");
                infos.forEach(function (f) {
                    lines.push("");
                    lines.push("[INFO] " + f.path);
                    lines.push("       Banks  : " + f.banks);
                    lines.push("       Note   : " + f.msg);
                });
                lines.push("");
            }
        }

        lines.push("=== Check Complete ===");
        var resultText = lines.join("\n");

        studio.system.print("=== Bank Assignment Check v" + BNK_VERSION + " ===");
        studio.system.print(resultText);

        studio.ui.showModalDialog({
            windowTitle:   "Bank Assignment Check  v" + BNK_VERSION,
            widgetType:    studio.ui.widgetType.TextEdit,
            text:          resultText,
            minimumWidth:  700,
            minimumHeight: 500
        });
    }
};
