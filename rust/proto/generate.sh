#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."

# Generate the tracked C bindings consumed by the current Ghostty fork.
output_dir="forks/ghostty/src/protobuf"
protoc-c --c_out="$output_dir" --proto_path=rust/proto rust/proto/termsurf.proto

echo "Generated $output_dir/termsurf.pb-c.{c,h}"
