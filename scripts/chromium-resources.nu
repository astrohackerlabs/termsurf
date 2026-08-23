#!/usr/bin/env nu
# Chromium runtime resource copy helpers for install.nu.

const CHROMIUM_REQUIRED_GENERATED_RESOURCES = [
  "gen/chrome/pdf_resources.pak"
  "gen/chrome/generated_resources_en-US.pak"
  "gen/chrome/common_resources.pak"
  "gen/components/components_resources.pak"
  "gen/components/strings/components_strings_en-US.pak"
  "gen/extensions/extensions_renderer_resources.pak"
]

def is-f [p: string] {
  (^test -f $p | complete).exit_code == 0
}

def copy-glob [pattern: string, destination: string] {
  let files = (try { glob --no-dir $pattern } catch { [] } | each {|p| $p | into string })
  if ($files | is-empty) {
    ^cp $pattern $destination
  } else {
    ^cp ...$files $destination
  }
}

export def copy-required-chromium-resource [
  chromium_out: string
  destination: string
  relative_path: string
] {
  let source_path = ($chromium_out | path join $relative_path)
  let destination_path = ($destination | path join $relative_path)

  if not (is-f $source_path) {
    print --stderr $"Error: Required Chromium resource missing: ($source_path)"
    print --stderr "Run: scripts/build.nu chromium-fork && scripts/build.nu chromium --release"
    exit 1
  }

  mkdir ($destination_path | path dirname)
  ^cp $source_path $destination_path
}

export def copy-chromium-runtime-resources [
  chromium_out: string
  destination: string
] {
  mkdir $destination

  let helper = ($destination | path join "ah-chromiumd")
  if (is-f $helper) {
    do {
      ^install_name_tool -delete_rpath $chromium_out $helper
    } | complete
  }

  print "==> Copying Chromium dylibs..."
  copy-glob ($chromium_out | path join "*.dylib") $"($destination)/"

  print "==> Copying Chromium resources..."
  copy-glob ($chromium_out | path join "*.pak") $"($destination)/"
  ^cp ($chromium_out | path join "icudtl.dat") $"($destination)/"
  copy-glob ($chromium_out | path join "v8_context_snapshot*.bin") $"($destination)/"

  print "==> Copying generated Chromium resources..."
  for relative_path in $CHROMIUM_REQUIRED_GENERATED_RESOURCES {
    copy-required-chromium-resource $chromium_out $destination $relative_path
  }
}
