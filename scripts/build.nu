#!/usr/bin/env nu
# Build ahterm / ahsh / ahweb / engines and related components.

def is-d [p: string] { (^test -d $p | complete).exit_code == 0 }
def is-x [p: string] { (^test -x $p | complete).exit_code == 0 }
def has-cmd [name: string] { not (which $name | is-empty) }

def script-path [] {
  $env.CURRENT_FILE? | default ($env.FILE_PWD | path join "build.nu")
}

def usage [] {
  print $"Usage: (script-path) <component> [--release] [--clean] [--open]"
  print "Components: ahterm, ahsh, ahweb, ahcalc, ahkey, ahplt, ahebx, ahnexus, ahtch, chromium-fork, ah-chromiumd, all"
  print "Aliases: aht→ahterm, webtui→ahweb, chromium→ah-chromiumd"
}

def --env maybe-termsurf-version [] {
  let astro = ($env.ASTROHACKER_VERSION? | default "")
  let ts = ($env.TERMSURF_VERSION? | default "")
  if ($astro | is-empty) and (not ($ts | is-empty)) {
    $env.ASTROHACKER_VERSION = $ts
  }
}

def version-extra [] {
  let v = ($env.ASTROHACKER_VERSION? | default "")
  if ($v | is-empty) { "" } else { $", version ($v)" }
}

def bun-build [repo_dir: string, pkg_dir: string, script: string] {
  do {
    cd $pkg_dir
    if not (is-d ($pkg_dir | path join "node_modules")) and not (is-d ($repo_dir | path join "node_modules")) {
      ^bun install
    }
    ^bun run $script
  }
}

def build-chromium-fork [opts: record] {
  let chromium_src = $opts.chromium_src
  if not (is-d $chromium_src) {
    print $"==> Skipping Chromium \(($chromium_src) not found\)"
    return
  }
  $env.PATH = ($env.PATH | prepend ($opts.company_dir | path join "forks/chromium/depot_tools"))
  print "==> Ensuring Chromium product args.gn..."
  let ensure = ($opts.script_dir | path join "ensure-chromium-args.nu")
  with-env { ASTROHACKER_CHROMIUM_OUT: $opts.chromium_out } {
    ^$ensure
  }
  cd $chromium_src
  if $opts.clean {
    print "==> Cleaning Chromium..."
    ^gn clean out/Default
  }
  print "==> gn gen out/Default (product args)..."
  ^gn gen out/Default
  print "==> Building Chromium..."
  ^autoninja -C out/Default libtermsurf_chromium
  print $"  Chromium: ($opts.chromium_out)"
}

def build-ahweb [opts: record] {
  cd $opts.rust_dir
  if $opts.clean {
    print "==> Cleaning ahweb..."
    ^cargo clean -p ahweb
  }
  if $opts.release {
    print "==> Building ahweb (release)..."
    ^cargo build --release -p ahweb
    print $"  ahweb: ($opts.rust_dir)/target/release/ahweb"
  } else {
    print "==> Building ahweb (debug)..."
    ^cargo build -p ahweb
    print $"  ahweb: ($opts.rust_dir)/target/debug/ahweb"
  }
}

def build-ahsh [opts: record] {
  let ahsh_dir = ($opts.rust_dir | path join "rust/ahsh")
  if not (is-d ($opts.company_dir | path join "forks/nushell")) {
    print --stderr $"Missing Nushell fork checkout: ($opts.company_dir)/forks/nushell"
    print --stderr "Reconstruct it from patches/nushell before building ahsh."
    exit 1
  }
  if not (is-d ($opts.company_dir | path join "forks/reedline")) {
    print --stderr $"Missing Reedline fork checkout: ($opts.company_dir)/forks/reedline"
    print --stderr "Reconstruct it from patches/reedline before building ahsh."
    exit 1
  }
  cd $ahsh_dir
  if $opts.clean {
    print "==> Cleaning ahsh..."
    ^cargo clean
  }
  if $opts.release {
    print "==> Building ahsh (release)..."
    ^cargo build --release
    print $"  ahsh: ($ahsh_dir)/target/release/ahsh"
  } else {
    print "==> Building ahsh (debug)..."
    ^cargo build
    print $"  ahsh: ($ahsh_dir)/target/debug/ahsh"
  }
}

def build-ahtch [opts: record] {
  let ahtch_dir = ($opts.rust_dir | path join "rust/ahtch")
  if not (is-d $ahtch_dir) {
    print --stderr $"Missing nested ahtch workspace: ($ahtch_dir)"
    exit 1
  }
  let libtorch = ($ahtch_dir | path join ".libtorch")
  let has_dir = (^test -d $libtorch | complete).exit_code == 0
  let has_link = (^test -L $libtorch | complete).exit_code == 0
  if (not $has_dir) and (not $has_link) {
    print --stderr $"Missing LibTorch pin: ($libtorch)"
    print --stderr "Run: rust/ahtch/scripts/bootstrap.sh"
    exit 1
  }
  cd $ahtch_dir
  if $opts.clean {
    print "==> Cleaning ahtch..."
    ^cargo clean
  }
  if $opts.release {
    print "==> Building ahtch (release)..."
    ^cargo build --release --bin ahtch
    print $"  ahtch: ($ahtch_dir)/target/release/ahtch"
  } else {
    print "==> Building ahtch (debug)..."
    ^cargo build --bin ahtch
    print $"  ahtch: ($ahtch_dir)/target/debug/ahtch"
  }
}

def build-chromiumd [opts: record] {
  cd $opts.rust_dir
  if not (is-d $opts.chromium_out) {
    print --stderr $"Missing Chromium output directory: ($opts.chromium_out)"
    print --stderr $"Build Chromium first with: (script-path) chromium-fork"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning Chromium..."
    ^cargo clean -p ah-chromiumd
  }
  if $opts.release {
    print "==> Building Chromium (release)..."
    ^cargo build --release -p ah-chromiumd
    ^cp ($opts.rust_dir | path join "target/release/ah-chromiumd") ($opts.chromium_out | path join "ah-chromiumd")
  } else {
    print "==> Building Chromium (debug)..."
    ^cargo build -p ah-chromiumd
    ^cp ($opts.rust_dir | path join "target/debug/ah-chromiumd") ($opts.chromium_out | path join "ah-chromiumd")
  }
  print $"  Chromium: ($opts.chromium_out)/ah-chromiumd"
}

def build-ahterm [opts: record] {
  mut configuration = "Debug"
  mut zig_optimize = "Debug"
  if $opts.release {
    $configuration = "Release"
    $zig_optimize = "ReleaseFast"
  }

  print $"==> Building GhosttyKit / libghostty \(($zig_optimize)\)..."
  cd $opts.ghostty_dir
  let ts_ver = ($env.TERMSURF_VERSION? | default "")
  if not ($ts_ver | is-empty) {
    ^zig build -Demit-macos-app=false $"-Doptimize=($zig_optimize)" $"-Dversion-string=($ts_ver)"
  } else {
    ^zig build -Demit-macos-app=false $"-Doptimize=($zig_optimize)"
  }

  cd ($opts.ghostty_dir | path join "macos")
  if $opts.clean {
    print $"==> Cleaning ahterm \(($configuration)\)..."
    ^nu ./build.nu --configuration $configuration --action clean
  }

  print $"==> Building ahterm \(($configuration)\)..."
  if not ($ts_ver | is-empty) {
    ^nu ./build.nu --configuration $configuration --action build --version $ts_ver
  } else {
    ^nu ./build.nu --configuration $configuration --action build
  }
  if $opts.release {
    let app = ($opts.ghostty_dir | path join "macos" "build" $configuration "Astrohacker TermSurf.app")
    ^codesign --force --deep --sign - $app
  }
  let app = ($opts.ghostty_dir | path join "macos" "build" $configuration "Astrohacker TermSurf.app")
  print $"  ahterm: ($app)"
  print $"  ahterm executable: ($app)/Contents/MacOS/ahterm"
}

def build-ahcalc [opts: record] {
  let ahcalc_dir = ($opts.repo_dir | path join "bun/ahcalc")
  if not (is-d $ahcalc_dir) {
    print --stderr $"Error: ahcalc package missing: ($ahcalc_dir)"
    exit 1
  }
  if not (has-cmd "bun") {
    print --stderr "Error: bun is required to build ahcalc (not found on PATH)"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning ahcalc dist..."
    ^rm -rf ($ahcalc_dir | path join "dist")
  }
  maybe-termsurf-version
  let kind = (if $opts.release { "release" } else { "debug" })
  print $"==> Building ahcalc \(($kind)(version-extra)\)..."
  bun-build $opts.repo_dir $ahcalc_dir "build:ahcalc"
  print $"  ahcalc: ($ahcalc_dir)/dist/ahcalc"
}

def build-ahkey [opts: record] {
  let ahkey_dir = ($opts.repo_dir | path join "bun/ahkey")
  if not (is-d $ahkey_dir) {
    print --stderr $"Error: ahkey package missing: ($ahkey_dir)"
    exit 1
  }
  if not (has-cmd "bun") {
    print --stderr "Error: bun is required to build ahkey (not found on PATH)"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning ahkey dist..."
    ^rm -rf ($ahkey_dir | path join "dist")
  }
  maybe-termsurf-version
  let kind = (if $opts.release { "release" } else { "debug" })
  print $"==> Building ahkey \(($kind)(version-extra)\)..."
  bun-build $opts.repo_dir $ahkey_dir "build:ahkey"
  print $"  ahkey: ($ahkey_dir)/dist/ahkey"
}

def build-ahplt [opts: record] {
  let ahplt_dir = ($opts.repo_dir | path join "bun/ahplt")
  if not (is-d $ahplt_dir) {
    print --stderr $"Error: ahplt package missing: ($ahplt_dir)"
    exit 1
  }
  if not (has-cmd "bun") {
    print --stderr "Error: bun is required to build ahplt (not found on PATH)"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning ahplt dist..."
    ^rm -rf ($ahplt_dir | path join "dist")
  }
  maybe-termsurf-version
  let kind = (if $opts.release { "release" } else { "debug" })
  print $"==> Building ahplt \(($kind)(version-extra)\)..."
  bun-build $opts.repo_dir $ahplt_dir "build:ahplt"
  print $"  ahplt: ($ahplt_dir)/dist/ahplt"
}

def build-ahebx [opts: record] {
  let ahebx_dir = ($opts.repo_dir | path join "bun/ahebx")
  if not (is-d $ahebx_dir) {
    print --stderr $"Error: ahebx package missing: ($ahebx_dir)"
    exit 1
  }
  if not (has-cmd "bun") {
    print --stderr "Error: bun is required to build ahebx (not found on PATH)"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning ahebx dist..."
    ^rm -rf ($ahebx_dir | path join "dist")
  }
  maybe-termsurf-version
  let kind = (if $opts.release { "release" } else { "debug" })
  print $"==> Building ahebx \(($kind)(version-extra)\)..."
  bun-build $opts.repo_dir $ahebx_dir "build:ahebx"
  print $"  ahebx: ($ahebx_dir)/dist/ahebx"
}

def build-ahnexus [opts: record] {
  let ahnexus_spa = ($opts.repo_dir | path join "bun/ahnexus")
  if not (is-d $ahnexus_spa) {
    print --stderr $"Error: ahnexus SPA package missing: ($ahnexus_spa)"
    exit 1
  }
  if not (has-cmd "bun") {
    print --stderr "Error: bun is required to build ahnexus SPA (not found on PATH)"
    exit 1
  }
  if $opts.clean {
    print "==> Cleaning ahnexus SPA dist..."
    ^rm -rf ($ahnexus_spa | path join "dist")
    if $opts.release {
      do {
        cd $opts.rust_dir
        ^cargo clean -p ahnexus
      } | complete
    }
  }
  maybe-termsurf-version
  let kind = (if $opts.release { "release" } else { "debug" })
  print $"==> Building ahnexus SPA \(($kind)(version-extra)\)..."
  do {
    cd $opts.repo_dir
    if not (is-d ($ahnexus_spa | path join "node_modules")) and not (is-d ($opts.repo_dir | path join "node_modules")) {
      ^bun install
    }
    ^bun run build:ahnexus
  }
  print $"  ahnexus SPA: ($ahnexus_spa)/dist"
  cd $opts.rust_dir
  if $opts.release {
    print "==> Building ahnexus binary (release)..."
    ^cargo build --release -p ahnexus
    print $"  ahnexus: ($opts.rust_dir)/target/release/ahnexus"
  } else {
    print "==> Building ahnexus binary (debug)..."
    ^cargo build -p ahnexus
    print $"  ahnexus: ($opts.rust_dir)/target/debug/ahnexus"
  }
}

def --wrapped main [...args: string] {
  let script_dir = ($env.FILE_PWD? | default $env.PWD | path expand)
  let repo_dir = ($script_dir | path dirname)
  let company_dir = $repo_dir
  let rust_dir = $company_dir
  let chromium_src = ($company_dir | path join "forks/chromium/src")
  let chromium_out = ($chromium_src | path join "out/Default")
  let chromium_protoc = ($chromium_out | path join "protoc")
  let ghostty_dir = ($company_dir | path join "forks/ghostty")

  mut release = false
  mut clean = false
  mut open_flag = false
  mut print_paths = false
  mut component = ""

  for arg in $args {
    match $arg {
      "--print-paths" => { $print_paths = true }
      "--release" => { $release = true }
      "--clean" => { $clean = true }
      "--open" => { $open_flag = true }
      _ if ($arg | str starts-with "-") => {
        print $"Unknown flag: ($arg)"
        usage
        exit 1
      }
      _ => {
        if ($component | is-empty) {
          $component = $arg
        } else {
          print "Error: multiple components specified"
          exit 1
        }
      }
    }
  }

  if $print_paths {
    print $"SCRIPT_DIR=($script_dir)"
    print $"REPO_DIR=($repo_dir)"
    print $"COMPANY_DIR=($company_dir)"
    print $"RUST_DIR=($rust_dir)"
    print $"CHROMIUM_SRC=($chromium_src)"
    print $"WEBKIT_SRC=($env.WEBKIT_SRC? | default "")"
    print $"GHOSTTY_DIR=($ghostty_dir)"
    exit 0
  }

  if ($component | is-empty) {
    usage
    exit 1
  }

  # Export PROTOC from Chromium if available (needed by prost_build).
  if (is-x $chromium_protoc) {
    $env.PROTOC = $chromium_protoc
  }

  let opts = {
    release: $release
    clean: $clean
    open: $open_flag
    script_dir: $script_dir
    repo_dir: $repo_dir
    company_dir: $company_dir
    rust_dir: $rust_dir
    chromium_src: $chromium_src
    chromium_out: $chromium_out
    ghostty_dir: $ghostty_dir
  }

  match $component {
    "chromium-fork" => { build-chromium-fork $opts }
    "ahweb" | "webtui" => { build-ahweb $opts }
    "ahsh" => { build-ahsh $opts }
    "ahcalc" => { build-ahcalc $opts }
    "ahkey" => { build-ahkey $opts }
    "ahplt" => { build-ahplt $opts }
    "ahebx" => { build-ahebx $opts }
    "ahnexus" => { build-ahnexus $opts }
    "ah-chromiumd" | "chromium" => { build-chromiumd $opts }
    "ahtch" => { build-ahtch $opts }
    "ahterm" | "aht" => { build-ahterm $opts }
    "all" => {
      build-chromium-fork $opts
      build-ahweb $opts
      build-ahsh $opts
      build-ahcalc $opts
      build-ahkey $opts
      build-ahplt $opts
      build-ahebx $opts
      build-ahnexus $opts
      build-ahtch $opts
      build-chromiumd $opts
      build-ahterm $opts
      print ""
      print "Done (all)."
    }
    _ => {
      print $"Unknown component: ($component)"
      usage
      exit 1
    }
  }
}
