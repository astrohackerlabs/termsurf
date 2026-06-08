//
//  RoasttyTitleUITests.swift
//  RoasttyUITests
//
//  Created by luca on 13.10.2025.
//

import XCTest

final class RoasttyTitleUITests: RoasttyCustomConfigCase {
    override func setUp() async throws {
        try await super.setUp()
        try updateConfig(#"title = "RoasttyUITestsLaunchTests""#)
    }

    @MainActor
    func testTitle() throws {
        let app = try roasttyApplication()
        app.launch()

        XCTAssertEqual(app.windows.firstMatch.title, "RoasttyUITestsLaunchTests", "Oops, `title=` doesn't work!")
    }
}
