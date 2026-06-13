// Issue 802 / Exp 4 — resolve a single app window's CGWindowID for `screencapture -l`.
//
// Usage:  swift winid.swift <owner-name|bundle-id|pid>
//   - prints one line:  <id>\t<x>\t<y>\t<w>\t<h>   for the frontmost real window
//   - env TS_LIST=1     prints all candidate windows (id, pid, owner, layer, WxH, onscreen)
//
// CGWindowListCopyWindowInfo enumerates windows front-to-back across all Spaces and
// needs no special permission for bounds/owner (only window *titles* need Screen
// Recording). This avoids the JXA `kCGWindowListOptionAll`-undefined bug from Exp 3.
import AppKit
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write("usage: winid.swift <owner-name|bundle-id|pid>\n".data(using: .utf8)!)
    exit(2)
}
let target = args[1]
let listMode = ProcessInfo.processInfo.environment["TS_LIST"] == "1"

// Resolve the target to a PID when it's numeric or a known bundle id; else match owner name.
var targetPID: Int? = Int(target)
if targetPID == nil, target.contains(".") {
    if let app = NSWorkspace.shared.runningApplications.first(where: { $0.bundleIdentifier == target }) {
        targetPID = Int(app.processIdentifier)
    }
}

guard let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}

struct Win { let id, pid, layer: Int; let owner: String; let x, y, w, h: Double; let onscreen: Bool }
var wins: [Win] = []
for d in info {
    guard let id = d[kCGWindowNumber as String] as? Int else { continue }
    let b = (d[kCGWindowBounds as String] as? [String: Any]) ?? [:]
    wins.append(Win(
        id: id,
        pid: (d[kCGWindowOwnerPID as String] as? Int) ?? -1,
        layer: (d[kCGWindowLayer as String] as? Int) ?? -1,
        owner: (d[kCGWindowOwnerName as String] as? String) ?? "",
        x: (b["X"] as? Double) ?? 0, y: (b["Y"] as? Double) ?? 0,
        w: (b["Width"] as? Double) ?? 0, h: (b["Height"] as? Double) ?? 0,
        onscreen: (d[kCGWindowIsOnscreen as String] as? Bool) ?? false
    ))
}

// Name match is a case-insensitive substring on purpose: // owner after the rename is "Roastty[DEBUG]". This harness passes the PID (from start-app.sh), so owner-name matching is bypassed. If two apps
// ever share a prefix, disambiguate by passing a pid or bundle id instead.
func matches(_ win: Win) -> Bool {
    if let tp = targetPID { return win.pid == tp }
    return win.owner.range(of: target, options: .caseInsensitive) != nil
}
let candidates = wins.filter(matches)

if listMode {
    for w in (candidates.isEmpty ? wins : candidates) {
        print("\(w.id)\t\(w.pid)\t\(w.owner)\tL\(w.layer)\t\(Int(w.w))x\(Int(w.h))\tonscreen=\(w.onscreen)")
    }
    exit(0)
}

// Front-to-back order is preserved, but AppKit may keep auxiliary layer-0
// windows offscreen. Prefer the first visible real window for screenshots and
// input coordinates.
let pick = candidates.first(where: { $0.onscreen && $0.layer == 0 && $0.w >= 50 && $0.h >= 50 })
    ?? candidates.first(where: { $0.layer == 0 && $0.w >= 50 && $0.h >= 50 })
    ?? candidates.first
guard let p = pick else {
    FileHandle.standardError.write("no window for target: \(target)\n".data(using: .utf8)!)
    exit(1)
}
print("\(p.id)\t\(Int(p.x))\t\(Int(p.y))\t\(Int(p.w))\t\(Int(p.h))")
