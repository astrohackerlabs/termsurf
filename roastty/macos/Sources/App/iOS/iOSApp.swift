import SwiftUI
import RoasttyKit

@main
struct Roastty_iOSApp: App {
    @StateObject private var roastty_app: Roastty.App

    init() {
        if roastty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv) != ROASTTY_SUCCESS {
            preconditionFailure("Initialize roastty backend failed")
        }
        _roastty_app = StateObject(wrappedValue: Roastty.App())
    }

    var body: some Scene {
        WindowGroup {
            iOS_RoasttyTerminal()
                .environmentObject(roastty_app)
        }
    }
}

struct iOS_RoasttyTerminal: View {
    @EnvironmentObject private var roastty_app: Roastty.App

    var body: some View {
        ZStack {
            // Make sure that our background color extends to all parts of the screen
            Color(roastty_app.config.backgroundColor).ignoresSafeArea()

            Roastty.Terminal()
        }
    }
}

struct iOS_RoasttyInitView: View {
    @EnvironmentObject private var roastty_app: Roastty.App

    var body: some View {
        VStack {
            Image("AppIconImage")
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(maxHeight: 96)
            Text("Roastty")
            Text("State: \(roastty_app.readiness.rawValue)")
        }
        .padding()
    }
}
