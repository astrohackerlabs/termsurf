//
//  RoasttyCustomConfigCase.swift
//  Roastty
//
//  Created by luca on 16.10.2025.
//

import XCTest

class RoasttyCustomConfigCase: XCTestCase {
    static let defaultsSuiteName: String = "ROASTTY_UI_TESTS"

    private let configFile: URL = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        .appendingPathExtension("roastty")

    override func setUpWithError() throws {
        continueAfterFailure = false
        try updateConfig("")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: configFile)
    }

    func updateConfig(_ newConfig: String) throws {
        try newConfig.write(to: configFile, atomically: true, encoding: .utf8)
    }

    func roasttyApplication(defaultsSuite: String = RoasttyCustomConfigCase.defaultsSuiteName) throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments.append(contentsOf: ["-ApplePersistenceIgnoreState", "YES"])
        app.launchEnvironment["ROASTTY_CONFIG_PATH"] = configFile.path
        app.launchEnvironment["ROASTTY_USER_DEFAULTS_SUITE"] = defaultsSuite
        return app
    }
}
