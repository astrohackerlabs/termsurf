#!/usr/bin/env nu

use ./chromium-resources.nu [copy-required-chromium-resource copy-chromium-runtime-resources]

def is-d [p: string] { (^test -d $p | complete).exit_code == 0 }
def is-f [p: string] { (^test -f $p | complete).exit_code == 0 }
def is-x [p: string] { (^test -x $p | complete).exit_code == 0 }
def is-w [p: string] { (^test -w $p | complete).exit_code == 0 }

def script-path [] {
  $env.CURRENT_FILE? | default ($env.FILE_PWD | path join "install.nu") | path expand
}

def usage [] {
  print $"Usage: (script-path) <component>"
  print "Components: ahterm, roamari-chromiumd, roamari, all"
  print "Aliases: aht→ahterm"
}

def is-root [] {
  (^id -u | str trim) == "0"
}

def needs-root [component: string, chromiumd_install_dir: string, applications_dir: string] {
  if $component == "roamari-chromiumd" and $chromiumd_install_dir != "/opt/homebrew/opt/roamari-chromiumd" {
    try {
      mkdir $chromiumd_install_dir
    } catch {
      print $"Error: ASTROHACKER_CHROMIUM_INSTALL_DIR is not writable: ($chromiumd_install_dir)"
      exit 1
    }
    if (is-w $chromiumd_install_dir) {
      return false
    }
    print $"Error: ASTROHACKER_CHROMIUM_INSTALL_DIR is not writable: ($chromiumd_install_dir)"
    exit 1
  }
  if $component == "ahterm" and $applications_dir != "/Applications" {
    try {
      mkdir $applications_dir
    } catch {
      print $"Error: TERMSURF_APPLICATIONS_DIR is not writable: ($applications_dir)"
      exit 1
    }
    if (is-w $applications_dir) {
      return false
    }
    print $"Error: TERMSURF_APPLICATIONS_DIR is not writable: ($applications_dir)"
    exit 1
  }
  true
}

def reexec-root [applications_dir: string, chromiumd_install_dir: string, args: list<string>] {
  let script = (script-path)
  try {
    ^sudo env $"TERMSURF_APPLICATIONS_DIR=($applications_dir)" $"ASTROHACKER_CHROMIUM_INSTALL_DIR=($chromiumd_install_dir)" $nu.current-exe $script ...$args
    exit 0
  } catch {|err|
    exit ($err.exit_code? | default 1)
  }
}

def install-chromiumd [rust_dir: string, chromium_out: string, chromiumd_install_dir: string] {
  let chromiumd_src = ($rust_dir | path join "target/release/roamari-chromiumd")
  let install_dir = $chromiumd_install_dir

  if not (is-f $chromiumd_src) {
    print $"Error: Release build not found at ($chromiumd_src)"
    print "Run: scripts/build.nu roamari-chromiumd --release"
    print "(alias: scripts/build.nu chromium --release)"
    exit 1
  }

  print $"==> Installing roamari-chromiumd to ($install_dir)..."
  mkdir $install_dir
  ^cp $chromiumd_src ($install_dir | path join "roamari-chromiumd")

  copy-chromium-runtime-resources $chromium_out $install_dir

  print "==> Codesigning roamari-chromiumd..."
  try { ^codesign --force --sign - ($install_dir | path join "roamari-chromiumd") } catch { }

  ^rm -rf /usr/local/chromium
  ^rm -f /usr/local/bin/chromium
  ^rm -rf /usr/local/lib/chromium
  ^rm -rf /opt/homebrew/opt/astrohacker-terminal-chromium
  ^rm -rf /opt/homebrew/opt/astrohacker-terminal-ah-chromiumd

  print $"  Dir: ($install_dir)"
  print $"  Bin: ($install_dir)/roamari-chromiumd"
}

def install-ahterm [ahterm_release_app: string, applications_dir: string, lsregister: string] {
  let app_src = $ahterm_release_app
  let app = ($applications_dir | path join "Astrohacker Terminal.app")

  if not (is-x ($app_src | path join "Contents/MacOS/ahterm")) {
    print $"Error: Release app not found at ($app_src)"
    print "Run: scripts/build.nu ahterm --release"
    exit 1
  }

  print $"==> Installing Astrohacker Terminal to ($app)..."
  ^rm -rf $app
  ^cp -R $app_src $app

  print "==> Codesigning..."
  try { ^codesign --force --deep --sign - $app } catch { }

  if (is-x $lsregister) {
    try { ^$lsregister -f -R -trusted $app } catch { }
  }

  print $"  App: ($app)"
}

def install-roamari [rust_dir: string] {
  let web = ($rust_dir | path join "target/release/roamari")

  if not (is-f $web) {
    print $"Error: Release build not found at ($web)"
    print "Run: scripts/build.nu roamari --release"
    exit 1
  }

  print "==> Installing roamari to /usr/local/bin/roamari..."
  ^cp $web /usr/local/bin/roamari
  try { ^codesign --force --sign - /usr/local/bin/roamari } catch { }

  print "  Bin: /usr/local/bin/roamari"
}

def --wrapped main [...args: string] {
  let script_dir = ($env.FILE_PWD? | default $env.PWD | path expand)
  let repo_dir = ($script_dir | path dirname)
  let rust_dir = $repo_dir
  let chromium_out = ($repo_dir | path join "forks/chromium/src/out/Default")
  let lsregister = "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
  let ahterm_release_app = ($repo_dir | path join "forks/ghostty/macos/build/Release/Astrohacker Terminal.app")
  let applications_dir = ($env.TERMSURF_APPLICATIONS_DIR? | default "/Applications")
  let chromiumd_install_dir = ($env.ASTROHACKER_CHROMIUM_INSTALL_DIR? | default "/opt/homebrew/opt/roamari-chromiumd")

  mut component = ($args | get 0? | default "")

  if ($component | is-empty) {
    usage
    exit 1
  }

  match $component {
    "aht" => { $component = "ahterm" }
    _ => {}
  }

  if $component not-in ["ahterm" "roamari-chromiumd" "roamari" "all"] {
    print $"Unknown component: ($component)"
    usage
    exit 1
  }

  if $component == "ahterm" and not (is-x ($ahterm_release_app | path join "Contents/MacOS/ahterm")) {
    print $"Error: Release app not found at ($ahterm_release_app)"
    print "Run: scripts/build.nu ahterm --release"
    exit 1
  }

  if (not (is-root)) and (needs-root $component $chromiumd_install_dir $applications_dir) {
    reexec-root $applications_dir $chromiumd_install_dir $args
  }

  match $component {
    "roamari-chromiumd" => { install-chromiumd $rust_dir $chromium_out $chromiumd_install_dir }
    "ahterm" => { install-ahterm $ahterm_release_app $applications_dir $lsregister }
    "roamari" => { install-roamari $rust_dir }
    "all" => {
      install-chromiumd $rust_dir $chromium_out $chromiumd_install_dir
      install-ahterm $ahterm_release_app $applications_dir $lsregister
      install-roamari $rust_dir
      print ""
      print "Done (all)."
    }
    _ => {}
  }
}
