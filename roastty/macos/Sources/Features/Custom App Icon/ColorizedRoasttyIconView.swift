import SwiftUI
import Cocoa

// For testing.
struct ColorizedRoasttyIconView: View {
    var body: some View {
        Image(nsImage: ColorizedRoasttyIcon(
            screenColors: [.purple, .blue],
            ghostColor: .yellow,
            frame: .aluminum
        ).makeImage(in: .main)!)
    }
}
