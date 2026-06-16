#!/usr/bin/env nu

# Build the macOS Roastty app using xcodebuild with a clean environment
# to avoid Nix shell interference (NIX_LDFLAGS, NIX_CFLAGS_COMPILE, etc.).

def main [
    --scheme: string = "Roastty"       # Xcode scheme (Roastty, Roastty-iOS, DockTilePlugin)
    --configuration: string = "Debug"  # Build configuration (Debug, Release, ReleaseLocal)
    --action: string = "build"         # xcodebuild action (build, test, clean, etc.)
    --ui-tests                         # Include UI tests in CLI-driven test runs
    --only-testing: string = ""        # xcodebuild -only-testing selector
] {
    let repo_root = ($env.FILE_PWD | path dirname | path dirname)
    let project = ($env.FILE_PWD | path join "Roastty.xcodeproj")
    let build_dir = ($env.FILE_PWD | path join "build")

    if ($scheme == "Roastty") and ($action in [build test]) {
        let kit_config = if $configuration in [Release ReleaseLocal] {
            "release"
        } else {
            "debug"
        }
        ^bash ($repo_root | path join "scripts/roastty-app/build-roastty-kit.sh") $kit_config
    }

    # Skip UI tests for normal CLI-based invocations because they require
    # special permissions. They can be enabled explicitly with --ui-tests.
    let skip_testing = if $action == "test" and not $ui_tests {
        [-skip-testing RoasttyUITests]
    } else {
        []
    }

    let ui_test_env = if $ui_tests {
        ["IDE_DISABLED_OS_ACTIVITY_DT_MODE=1"]
    } else {
        []
    }

    let only_testing_args = if $only_testing == "" {
        []
    } else {
        [-only-testing $only_testing]
    }

    # Focused UI-test runs should skip unit-test execution. This keeps the
    # explicit UI gate independent from unrelated unit-test-only failures.
    let skip_unit_testing = if $ui_tests and ($only_testing | str starts-with "RoasttyUITests") {
        [-skip-testing RoasttyTests]
    } else {
        []
    }

    let test_runner_args = if $action == "test" {
        [-destination "platform=macOS,arch=arm64" -parallel-testing-enabled NO]
    } else {
        []
    }

    let local_arch_args = if ($scheme == "Roastty") and ($configuration in [Debug ReleaseLocal]) {
        [ARCHS=arm64 ONLY_ACTIVE_ARCH=YES]
    } else {
        []
    }

    (^env -i
        $"HOME=($env.HOME)"
        "PATH=/usr/bin:/bin:/usr/sbin:/sbin"
        ...$ui_test_env
        xcodebuild
        -project $project
        -scheme $scheme
        -configuration $configuration
        $"SYMROOT=($build_dir)"
        ...$local_arch_args
        ...$test_runner_args
        ...$skip_testing
        ...$skip_unit_testing
        ...$only_testing_args
        $action)
}
