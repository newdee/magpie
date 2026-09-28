#!/usr/bin/env bash
# Linux only: put Microsoft's ONNX Runtime build at src-tauri/lib/libonnxruntime.so,
# which the Linux bundles ship and magpie loads at run time (core/src/onnx.rs).
# Microsoft's release needs glibc 2.28, so the app runs on Ubuntu 22.04; the
# build `ort` links statically elsewhere needs glibc 2.38.
# Keep ORT_VERSION at or above the API level `ort` is built for (api-27 today)
# and in step with the version `ort` links on Windows and macOS.
set -euo pipefail

ORT_VERSION=1.28.2
ORT_SHA256=d7209b8751b27b862b0c76332c2e20e203396edb5dab700ecf4bb485cf147415

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/src-tauri/lib/libonnxruntime.so"
if [ -f "$dest" ] && [ "$(cat "$dest.version" 2>/dev/null)" = "$ORT_VERSION" ]; then
  echo "onnxruntime $ORT_VERSION already at $dest"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
url="https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VERSION/onnxruntime-linux-x64-$ORT_VERSION.tgz"
curl -fsSL --retry 3 -o "$work/ort.tgz" "$url"
echo "$ORT_SHA256  $work/ort.tgz" | sha256sum -c -
tar -xzf "$work/ort.tgz" -C "$work"
mkdir -p "$(dirname "$dest")"
# the real file, not the libonnxruntime.so -> .so.1 -> .so.1.x.y symlink chain
cp -L "$work/onnxruntime-linux-x64-$ORT_VERSION/lib/libonnxruntime.so" "$dest"
chmod 755 "$dest"
echo "$ORT_VERSION" > "$dest.version"
echo "onnxruntime $ORT_VERSION -> $dest"
