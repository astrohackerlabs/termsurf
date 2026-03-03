#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Generate C code from the proto schema.
protoc --c_out=gui/src/protobuf --proto_path=proto proto/termsurf.proto

echo "Generated gui/src/protobuf/termsurf.pb-c.{c,h}"
