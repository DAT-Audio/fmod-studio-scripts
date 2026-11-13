// ═══════════════════════════════════════════════════════════════════════════════
//  DAT-audio Utilities — loader
//  Version: 1.7.0 (pack)
// ═══════════════════════════════════════════════════════════════════════════════
//  Registers every DAT-audio utility under a single "DAT-audio Utilities"
//  submenu of FMOD Studio's Scripts menu.
//
//  WHY THIS FILE EXISTS
//  FMOD Studio only auto-evaluates .js files sitting DIRECTLY in a Scripts
//  directory — it does not recurse into subfolders. (Verified against FMOD
//  Studio 2.02.33: scripts placed in a subfolder register no menu items. The
//  manual's studio.system.require notes confirm it — placing modules in a
//  subfolder "ensures the module's files are not loaded as top-level script
//  files".) So this one file stays at the top level and pulls in the rest from
//  the "DAT-audio Utilities" folder next to it.
//
//  INSTALL — copy this file AND the "DAT-audio Utilities" folder, side by side,
//  into any Scripts directory:
//    macOS   : ~/Library/Application Support/FMOD Studio/Scripts/
//    Windows : %LOCALAPPDATA%\FMOD Studio\Scripts\
//    Project : <project root>/Scripts/          (per-project install)
//
//  Resulting layout:
//    Scripts/
//      DAT-audio Utilities.js        <- this file (must stay at the top level)
//      DAT-audio Utilities/
//        Audit & Detect.js
//        Audit & Detect/             <- the checks it runs
//        ...
//
//  After adding or editing any script: Scripts > Reload in FMOD Studio.
//
//  ADDING A NEW UTILITY
//    1. Put the .js in the "DAT-audio Utilities" folder.
//    2. Name its menu item "DAT-audio Utilities\\<Tool Name>" (a '\' in the
//       name is what creates the submenu; escape it as '\\' in the string).
//    3. Add the filename to RELEASE_SCRIPTS below.
//  Compatible with FMOD Studio 2.02.x and 2.03.x (Open Automation Curve
//  Library, Open Profiler First Person View, Open Reference Viewer and Open
//  Sandbox First Person View also 2.04).
// ═══════════════════════════════════════════════════════════════════════════════

"use strict";

// Pack version — see the top-level README for every tool's own version.
var DAT_PACK_VERSION = "1.7.0";

// Paths are relative to this file. Use forward slashes — Qt normalises them on
// Windows too.
var SUBFOLDER = "DAT-audio Utilities/";

// Release tools — always loaded.
var RELEASE_SCRIPTS = [
    "Audit & Detect.js",              // runs the Audit/Detect checks in "Audit & Detect/"
    "Copy Selected Event Names.js",
    "Copy-Paste Conditions & Automation.js",
    "Edit Events in Batch.js",
    "Event Recorder.js",
    "Open Automation Curve Library.js",
    "Open Profiler First Person View.js",
    "Open Reference Viewer.js",
    "Open Sandbox First Person View.js"
];

function loadAll(files) {
    files.forEach(function (file) {
        try {
            studio.system.require(SUBFOLDER + file);
        } catch (e) {
            studio.system.error(
                "[DAT-audio Utilities] Failed to load \"" + SUBFOLDER + file +
                "\": " + e + " — check the file is present next to " +
                "\"DAT-audio Utilities.js\" and contains valid JavaScript.");
        }
    });
}

loadAll(RELEASE_SCRIPTS);

studio.system.print("[DAT-audio Utilities] v" + DAT_PACK_VERSION + " loaded from " +
                    ((typeof __dirname === "string") ? __dirname : "?"));
