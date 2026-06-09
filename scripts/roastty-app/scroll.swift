// Post CGEvent scroll-wheel events over a window: swift scroll.swift <x> <y> <ticks>
// <x> <y> = screen point to scroll at (window center). <ticks> = line steps; positive = up
// (toward scrollback history). Restores the prior cursor position. (Issue 802 / Exp 23.)
import CoreGraphics
import Foundation
let a = CommandLine.arguments
guard a.count == 4, let x = Double(a[1]), let y = Double(a[2]), let ticks = Int(a[3]) else {
    print("usage: scroll.swift <x> <y> <ticks>"); exit(1)
}
let prior = CGEvent(source: nil)?.location ?? CGPoint(x: x, y: y)
CGWarpMouseCursorPosition(CGPoint(x: x, y: y))
usleep(120_000)
let step: Int32 = ticks > 0 ? 3 : -3   // 3 lines per tick
for _ in 0..<abs(ticks) {
    if let ev = CGEvent(scrollWheelEvent2Source: nil, units: .line,
                        wheelCount: 1, wheel1: step, wheel2: 0, wheel3: 0) {
        ev.location = CGPoint(x: x, y: y)
        ev.post(tap: .cghidEventTap)
    }
    usleep(20_000)
}
usleep(150_000)
CGWarpMouseCursorPosition(prior)   // restore the cursor
print("scrolled \(ticks) ticks at (\(Int(x)),\(Int(y)))")
