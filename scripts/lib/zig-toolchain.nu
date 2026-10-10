#!/usr/bin/env nu
# Select the Zig compiler required by a Ghostty checkout.
#
# Ghostty's build.zig.zon names `minimum_zig_version`, and its `requireZig`
# rejects any compiler whose major/minor differ or whose patch is lower. The
# Homebrew `zig` formula tracks the newest release, so the versioned keg-only
# `zig@<major>.<minor>` formula is preferred over plain PATH `zig`.
#
# Run directly to print the selected path for commands such as `zig build test`:
#   ^(nu scripts/lib/zig-toolchain.nu) build test

const REPO = (path self | path dirname | path dirname | path dirname)

def parse-version [text: string] {
  let m = ($text | str trim | parse --regex '^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)')
  if ($m | is-empty) { return null }
  let v = ($m | first)
  {major: ($v.major | into int), minor: ($v.minor | into int), patch: ($v.patch | into int)}
}

# Ghostty's requireZig rule: same major and minor, patch at least the minimum.
export def zig-version-ok [found: string, required: string]: nothing -> bool {
  let f = (parse-version $found)
  let r = (parse-version $required)
  if $f == null or $r == null { return false }
  $f.major == $r.major and $f.minor == $r.minor and $f.patch >= $r.patch
}

export def required-zig-version [ghostty_dir: string] {
  let zon = ($ghostty_dir | path join "build.zig.zon")
  if not ($zon | path exists) {
    error make --unspanned {msg: $"Cannot select Zig: ($zon) not found. Prepare the fork with `ah fork prepare ghostty`."}
  }
  let m = (open --raw $zon | parse --regex '\.minimum_zig_version\s*=\s*"(?<v>[^"]+)"')
  if ($m | is-empty) or ((parse-version ($m | first | get v)) == null) {
    error make --unspanned {msg: $"Cannot select Zig: no valid minimum_zig_version in ($zon)."}
  }
  $m | first | get v
}

export def brew-prefix []: nothing -> string {
  let r = (do { ^brew --prefix } | complete)
  if $r.exit_code == 0 and not ($r.stdout | str trim | is-empty) {
    $r.stdout | str trim
  } else {
    "/opt/homebrew"
  }
}

def probe [path: string] {
  let r = (do { ^$path version } | complete)
  if $r.exit_code == 0 { $r.stdout | str trim } else { null }
}

# Returns {path, version, required}; raises with the install command when no
# candidate satisfies the checkout's requirement.
export def resolve-zig [ghostty_dir: string, brew_prefix: string] {
  let required = (required-zig-version $ghostty_dir)
  let r = (parse-version $required)
  let formula = $"zig@($r.major).($r.minor)"
  let keg = ($brew_prefix | path join "opt" $formula "bin" "zig")
  let on_path = (which zig | where type == external | get path?.0?)
  let candidates = ([$keg $on_path] | compact | where {|p| $p | path exists } | uniq)

  mut found = []
  for c in $candidates {
    let v = (probe $c)
    if $v == null { continue }
    if (zig-version-ok $v $required) {
      return {path: $c, version: $v, required: $required}
    }
    $found = ($found | append $"($c) is ($v)")
  }

  let seen = if ($found | is-empty) { "no zig found" } else { $"found ($found | str join ', ')" }
  error make --unspanned {msg: $"Ghostty requires Zig ($r.major).($r.minor).x \(>= ($required)\); ($seen). Install it: brew install ($formula)"}
}

def main [ghostty_dir?: string] {
  let dir = ($ghostty_dir | default ($REPO | path join "forks" "ghostty"))
  print (resolve-zig $dir (brew-prefix)).path
}
