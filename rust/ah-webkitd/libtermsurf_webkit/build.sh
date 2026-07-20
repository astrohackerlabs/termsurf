#!/bin/sh
set -eu

cd "$(dirname "$0")"

repo_root="$(git rev-parse --show-toplevel)"
configuration="${TERMSURF_WEBKIT_CONFIGURATION:-Debug}"
clean=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --debug)
      configuration="Debug"
      ;;
    --release)
      configuration="Release"
      ;;
    --configuration)
      shift
      if [ "$#" -eq 0 ]; then
        printf '%s\n' "error: --configuration requires Debug or Release" >&2
        exit 1
      fi
      configuration="$1"
      ;;
    --clean)
      clean=true
      ;;
    *)
      printf '%s\n' "error: unknown flag: $1" >&2
      printf '%s\n' "usage: $0 [--debug|--release|--configuration Debug|Release] [--clean]" >&2
      exit 1
      ;;
  esac
  shift
done

case "$configuration" in
  Debug | Release) ;;
  *)
    printf '%s\n' "error: unsupported configuration: $configuration" >&2
    printf '%s\n' "expected: Debug or Release" >&2
    exit 1
    ;;
esac

webkit_build="$repo_root/forks/webkit/src/WebKitBuild/$configuration"

if [ ! -d "$webkit_build/WebKit.framework" ]; then
  printf '%s\n' "error: missing $webkit_build/WebKit.framework" >&2
  if [ "$configuration" = "Release" ]; then
    printf '%s\n' "run: forks/webkit/src/Tools/Scripts/build-webkit --release" >&2
  else
    printf '%s\n' "run: forks/webkit/src/Tools/Scripts/build-webkit --debug" >&2
  fi
  exit 1
fi

if [ "$clean" = true ]; then
  rm -rf build
fi

mkdir -p build

common_flags="
  -fobjc-arc
  -Wall
  -Wextra
  -Werror
  -Wno-deprecated-declarations
  -Iinclude
  -F$webkit_build
"

common_links="
  -framework Cocoa
  -framework PDFKit
  -framework QuartzCore
  -framework WebKit
  -rpath $webkit_build
"

clang++ \
  $common_flags \
  -std=c++17 \
  -dynamiclib \
  -install_name @rpath/libtermsurf_webkit.dylib \
  src/libtermsurf_webkit.mm \
  $common_links \
  -o build/libtermsurf_webkit.dylib

install_name_tool \
  -change /System/Library/Frameworks/WebKit.framework/Versions/A/WebKit \
  @rpath/WebKit.framework/Versions/A/WebKit \
  build/libtermsurf_webkit.dylib

clang \
  -Wall \
  -Wextra \
  -Werror \
  -Iinclude \
  smoke-test/smoke_test.c \
  -Lbuild \
  -ltermsurf_webkit \
  -rpath "$PWD/build" \
  -rpath "$webkit_build" \
  -o build/smoke-test

clang \
  -fobjc-arc \
  -Wall \
  -Wextra \
  -Werror \
  -Iinclude \
  -Ismoke-test \
  smoke-test/back_action_smoke.m \
  -Lbuild \
  -ltermsurf_webkit \
  -framework Cocoa \
  -rpath "$PWD/build" \
  -rpath "$webkit_build" \
  -o build/back-action-smoke

clang \
  -fobjc-arc \
  -Wall \
  -Wextra \
  -Werror \
  -Iinclude \
  -Ismoke-test \
  smoke-test/refresh_action_smoke.m \
  -Lbuild \
  -ltermsurf_webkit \
  -framework Cocoa \
  -rpath "$PWD/build" \
  -rpath "$webkit_build" \
  -o build/refresh-action-smoke

printf '%s\n' "built webkit/libtermsurf_webkit/build/libtermsurf_webkit.dylib"
printf '%s\n' "built webkit/libtermsurf_webkit/build/smoke-test"
printf '%s\n' "built webkit/libtermsurf_webkit/build/back-action-smoke"
printf '%s\n' "built webkit/libtermsurf_webkit/build/refresh-action-smoke"
