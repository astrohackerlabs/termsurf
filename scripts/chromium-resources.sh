#!/usr/bin/env bash

CHROMIUM_REQUIRED_GENERATED_RESOURCES=(
  "gen/chrome/pdf_resources.pak"
  "gen/chrome/generated_resources_en-US.pak"
  "gen/chrome/common_resources.pak"
  "gen/components/components_resources.pak"
  "gen/components/strings/components_strings_en-US.pak"
  "gen/extensions/extensions_renderer_resources.pak"
)

copy_required_chromium_resource() {
  local chromium_out="$1"
  local destination="$2"
  local relative_path="$3"
  local source_path="$chromium_out/$relative_path"
  local destination_path="$destination/$relative_path"

  if [ ! -f "$source_path" ]; then
    echo "Error: Required Chromium resource missing: $source_path" >&2
    echo "Run: scripts/build.sh chromium-fork && scripts/build.sh chromium --release" >&2
    return 1
  fi

  mkdir -p "$(dirname "$destination_path")"
  cp "$source_path" "$destination_path"
}

copy_chromium_runtime_resources() {
  local chromium_out="$1"
  local destination="$2"

  mkdir -p "$destination"

  if [ -f "$destination/ah-chromiumd" ]; then
    install_name_tool -delete_rpath "$chromium_out" "$destination/ah-chromiumd" 2>/dev/null || true
  fi

  echo "==> Copying Chromium dylibs..."
  cp "$chromium_out"/*.dylib "$destination/"

  echo "==> Copying Chromium resources..."
  cp "$chromium_out"/*.pak "$destination/"
  cp "$chromium_out/icudtl.dat" "$destination/"
  cp "$chromium_out"/v8_context_snapshot*.bin "$destination/"

  echo "==> Copying generated Chromium resources..."
  local relative_path
  for relative_path in "${CHROMIUM_REQUIRED_GENERATED_RESOURCES[@]}"; do
    copy_required_chromium_resource "$chromium_out" "$destination" "$relative_path"
  done
}
