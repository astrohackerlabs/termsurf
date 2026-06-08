import Foundation

extension UserDefaults {
    static var roasttySuite: String? {
        #if DEBUG
        ProcessInfo.processInfo.environment["ROASTTY_USER_DEFAULTS_SUITE"]
        #else
        nil
        #endif
    }

    static var roastty: UserDefaults {
        roasttySuite.flatMap(UserDefaults.init(suiteName:)) ?? .standard
    }
}
