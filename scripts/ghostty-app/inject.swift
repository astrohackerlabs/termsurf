// Issue 802 / Exp 5 — CGEvent mouse injector (keyboard goes via osascript).
// Coordinates are global points (same units as CGWindowBounds from winid.swift).
//   inject.swift move  <x> <y>
//   inject.swift click <x> <y> [left|right|middle] [count] [control|command|shift|option]
//   inject.swift drag  <x1> <y1> <x2> <y2>
//   inject.swift scroll <x> <y> <lines>      (positive = up, negative = down)
//   inject.swift key <virtual-key-code> [control] [command] [shift] [option]
//   inject.swift type <utf8-text-file>
// Requires Accessibility for the responsible app (the host terminal).
import CoreGraphics
import Foundation

let a = CommandLine.arguments
func n(_ i: Int) -> CGFloat { CGFloat(Double(a[i]) ?? 0) }
func flags(_ values: ArraySlice<String>) -> CGEventFlags {
    var flags: CGEventFlags = []
    for flag in values {
        switch flag {
        case "control": flags.insert(.maskControl)
        case "command": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "option": flags.insert(.maskAlternate)
        default:
            FileHandle.standardError.write("unknown flag: \(flag)\n".data(using: .utf8)!)
            exit(2)
        }
    }
    return flags
}
func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap) }
func key(_ code: CGKeyCode, _ down: Bool) -> CGEvent? {
    CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)
}
func ev(_ t: CGEventType, _ p: CGPoint, _ b: CGMouseButton) -> CGEvent? {
    CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: b)
}
guard a.count >= 2 else {
    FileHandle.standardError.write("usage: inject <move|click|drag|scroll|key|type> ...\n".data(using: .utf8)!)
    exit(2)
}

switch a[1] {
case "move":
    let e = ev(.mouseMoved, CGPoint(x: n(2), y: n(3)), .left)
    e?.flags = flags(a.dropFirst(4))
    post(e)

case "click":
    let p = CGPoint(x: n(2), y: n(3))
    let which = a.count > 4 ? a[4] : "left"
    let count: Int
    let modifierStart: Int
    if a.count > 5, let parsedCount = Int(a[5]) {
        count = parsedCount
        modifierStart = 6
    } else {
        count = 1
        modifierStart = 5
    }
    let eventFlags = flags(a.dropFirst(modifierStart))
    let (down, up, b): (CGEventType, CGEventType, CGMouseButton) =
        which == "right" ? (.rightMouseDown, .rightMouseUp, .right)
        : which == "middle" ? (.otherMouseDown, .otherMouseUp, .center)
        : (.leftMouseDown, .leftMouseUp, .left)
    post(ev(.mouseMoved, p, b))
    for i in 1...count {
        let d = ev(down, p, b); d?.flags = eventFlags; d?.setIntegerValueField(.mouseEventClickState, value: Int64(i)); post(d)
        let u = ev(up, p, b); u?.flags = eventFlags; u?.setIntegerValueField(.mouseEventClickState, value: Int64(i)); post(u)
    }

case "drag":
    let p1 = CGPoint(x: n(2), y: n(3)), p2 = CGPoint(x: n(4), y: n(5))
    post(ev(.mouseMoved, p1, .left))
    post(ev(.leftMouseDown, p1, .left))
    let steps = 12
    for i in 1...steps {
        let t = CGFloat(i) / CGFloat(steps)
        post(ev(.leftMouseDragged, CGPoint(x: p1.x + (p2.x - p1.x) * t, y: p1.y + (p2.y - p1.y) * t), .left))
    }
    post(ev(.leftMouseUp, p2, .left))

case "scroll":
    post(ev(.mouseMoved, CGPoint(x: n(2), y: n(3)), .left))
    let e = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1,
                    wheel1: Int32(Double(a[4]) ?? 0), wheel2: 0, wheel3: 0)
    e?.post(tap: .cghidEventTap)

case "key":
    guard a.count >= 3, let code = UInt16(a[2]) else {
        FileHandle.standardError.write("usage: inject key <virtual-key-code> [control] [command] [shift] [option]\n".data(using: .utf8)!)
        exit(2)
    }
    let flags = flags(a.dropFirst(3))
    let down = key(CGKeyCode(code), true)
    down?.flags = flags
    post(down)
    let up = key(CGKeyCode(code), false)
    up?.flags = flags
    post(up)

case "type":
    guard a.count >= 3 else {
        FileHandle.standardError.write("usage: inject type <utf8-text-file>\n".data(using: .utf8)!)
        exit(2)
    }
    let text = try String(contentsOfFile: a[2], encoding: .utf8)
    for unit in text.utf16 {
        var value = unit
        let down = key(0, true)
        down?.flags = []
        down?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &value)
        post(down)
        let up = key(0, false)
        up?.flags = []
        up?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &value)
        post(up)
        usleep(1_000)
    }

default:
    FileHandle.standardError.write("unknown subcommand: \(a[1])\n".data(using: .utf8)!)
    exit(2)
}
