#!/usr/bin/env nu

def is-w [p: string] { (^test -w $p | complete).exit_code == 0 }

def script-path [] {
  $env.CURRENT_FILE? | default ($env.FILE_PWD | path join "uninstall.nu") | path expand
}

def usage [] {
  print $"Usage: (script-path) <component>"
  print "Components: ahterm, ah-chromiumd, ahweb, all"
  print "Aliases: aht→ahterm, webtui→ahweb"
}

def is-root [] {
  (^id -u | str trim) == "0"
}

def needs-root [component: string, chromiumd_install_dir: string, applications_dir: string] {
  if $component == "ah-chromiumd" and $chromiumd_install_dir != "/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd" {
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

def uninstall-chromiumd [chromiumd_install_dir: string] {
  print "==> Uninstalling ah-chromiumd..."
  ^rm -rf $chromiumd_install_dir
  ^rm -rf /usr/local/chromium
  ^rm -f /usr/local/bin/chromium
  ^rm -rf /usr/local/lib/chromium
  ^rm -rf /opt/homebrew/opt/astrohacker-terminal-chromium

  print $"  Removed: ($chromiumd_install_dir)"
}

def uninstall-ahterm [applications_dir: string] {
  let app = ($applications_dir | path join "Astrohacker Terminal.app")

  print "==> Uninstalling Astrohacker Terminal..."
  ^rm -rf $app

  print $"  Removed: ($app)"
}

def uninstall-ahweb [] {
  print "==> Uninstalling ahweb..."
  ^rm -f /usr/local/bin/ahweb
  ^rm -f /usr/local/bin/web

  print "  Removed: /usr/local/bin/ahweb (and legacy /usr/local/bin/web if present)"
}

def --wrapped main [...args: string] {
  mut component = ($args | get 0? | default "")
  let applications_dir = ($env.TERMSURF_APPLICATIONS_DIR? | default "/Applications")
  let chromiumd_install_dir = ($env.ASTROHACKER_CHROMIUM_INSTALL_DIR? | default "/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd")

  if ($component | is-empty) {
    usage
    exit 1
  }

  match $component {
    "aht" => { $component = "ahterm" }
    "webtui" => { $component = "ahweb" }
    _ => {}
  }

  if $component not-in ["ahterm" "ah-chromiumd" "ahweb" "all"] {
    print $"Unknown component: ($component)"
    usage
    exit 1
  }

  if (not (is-root)) and (needs-root $component $chromiumd_install_dir $applications_dir) {
    reexec-root $applications_dir $chromiumd_install_dir $args
  }

  match $component {
    "ah-chromiumd" => { uninstall-chromiumd $chromiumd_install_dir }
    "ahterm" => { uninstall-ahterm $applications_dir }
    "ahweb" => { uninstall-ahweb }
    "all" => {
      uninstall-chromiumd $chromiumd_install_dir
      uninstall-ahterm $applications_dir
      uninstall-ahweb
      print ""
      print "Done (all)."
    }
    _ => {}
  }
}
