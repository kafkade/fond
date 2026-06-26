#!/usr/bin/env bash
#
# Build the Swift bindings and Fond.xcframework from the `fond-ffi` crate.
#
# Produces, under apple/FondKit/:
#   - Sources/FondKit/fond_ffi.swift   (generated Swift bindings)
#   - xcframework/Fond.xcframework      (static libs + headers for all slices)
#
# Both are git-ignored build artifacts. Run this once before opening the
# FondApp Xcode project (it depends on the local FondKit package).
#
# Requirements: Rust toolchain, Xcode (xcodebuild/lipo), and the Apple Rust
# targets (the script adds them for you).

set -euo pipefail

# ── Locations ─────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CRATE="fond-ffi"
LIB_NAME="libfond_ffi.a"
PROFILE="release"

KIT_DIR="${SCRIPT_DIR}/FondKit"
SWIFT_OUT="${KIT_DIR}/Sources/FondKit"
XCF_DIR="${KIT_DIR}/xcframework"
XCFRAMEWORK="${XCF_DIR}/Fond.xcframework"

BUILD_TMP="${SCRIPT_DIR}/.build-tmp"
HEADERS_DIR="${BUILD_TMP}/headers"

# ── Target sets ───────────────────────────────────────────────────
# iOS device is arm64 only. Simulator + macOS get universal (arm64+x86_64)
# libraries when both Rust targets are installable.
IOS_DEVICE_TARGETS=(aarch64-apple-ios)
IOS_SIM_TARGETS=(aarch64-apple-ios-sim x86_64-apple-ios)
MACOS_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)

ALL_TARGETS=("${IOS_DEVICE_TARGETS[@]}" "${IOS_SIM_TARGETS[@]}" "${MACOS_TARGETS[@]}")

echo "▸ Adding Rust Apple targets (best effort)…"
for t in "${ALL_TARGETS[@]}"; do
  rustup target add "$t" >/dev/null 2>&1 || echo "  (could not add $t — skipping its slice)"
done

# Only keep targets that are actually installed.
installed() { rustup target list --installed | grep -qx "$1"; }

build_targets=()
for t in "${ALL_TARGETS[@]}"; do
  if installed "$t"; then
    build_targets+=("$t")
  fi
done

echo "▸ Building ${CRATE} (${PROFILE}) for: ${build_targets[*]}"
for t in "${build_targets[@]}"; do
  echo "  • $t"
  ( cd "${REPO_ROOT}" && cargo build -p "${CRATE}" --lib --${PROFILE} --target "$t" )
done

lib_path() { echo "${REPO_ROOT}/target/$1/${PROFILE}/${LIB_NAME}"; }

# ── Generate Swift bindings ───────────────────────────────────────
echo "▸ Generating Swift bindings…"
GEN_FROM="$(lib_path "${build_targets[0]}")"
rm -rf "${BUILD_TMP}"
mkdir -p "${BUILD_TMP}" "${HEADERS_DIR}" "${SWIFT_OUT}" "${XCF_DIR}"

( cd "${REPO_ROOT}" && cargo run -q -p "${CRATE}" --features bindgen --bin uniffi-bindgen -- \
    generate --library "${GEN_FROM}" --language swift --out-dir "${BUILD_TMP}" )

# Headers + a module map named `module.modulemap` (xcframework convention).
cp "${BUILD_TMP}/fond_ffiFFI.h" "${HEADERS_DIR}/"
cp "${BUILD_TMP}/fond_ffiFFI.modulemap" "${HEADERS_DIR}/module.modulemap"
# The .swift file is what the FondKit target compiles.
cp "${BUILD_TMP}/fond_ffi.swift" "${SWIFT_OUT}/fond_ffi.swift"

# ── Combine per-platform slices with lipo ─────────────────────────
# A single xcframework slice cannot contain two libs for the same platform,
# so simulator/macos arches are merged into one universal static lib.
merge() {
  # $1 = output path, rest = source targets
  local out="$1"; shift
  local srcs=()
  for t in "$@"; do
    if installed "$t"; then srcs+=("$(lib_path "$t")"); fi
  done
  mkdir -p "$(dirname "$out")"
  if [ "${#srcs[@]}" -eq 1 ]; then
    cp "${srcs[0]}" "$out"
  else
    lipo -create "${srcs[@]}" -output "$out"
  fi
}

XCF_ARGS=()

# iOS device (arm64)
if installed "aarch64-apple-ios"; then
  merge "${BUILD_TMP}/ios/${LIB_NAME}" aarch64-apple-ios
  XCF_ARGS+=(-library "${BUILD_TMP}/ios/${LIB_NAME}" -headers "${HEADERS_DIR}")
fi

# iOS simulator (arm64 [+ x86_64])
if installed "aarch64-apple-ios-sim" || installed "x86_64-apple-ios"; then
  merge "${BUILD_TMP}/ios-sim/${LIB_NAME}" "${IOS_SIM_TARGETS[@]}"
  XCF_ARGS+=(-library "${BUILD_TMP}/ios-sim/${LIB_NAME}" -headers "${HEADERS_DIR}")
fi

# macOS (arm64 [+ x86_64])
if installed "aarch64-apple-darwin" || installed "x86_64-apple-darwin"; then
  merge "${BUILD_TMP}/macos/${LIB_NAME}" "${MACOS_TARGETS[@]}"
  XCF_ARGS+=(-library "${BUILD_TMP}/macos/${LIB_NAME}" -headers "${HEADERS_DIR}")
fi

# ── Assemble the xcframework ───────────────────────────────────────
echo "▸ Assembling Fond.xcframework…"
rm -rf "${XCFRAMEWORK}"
xcodebuild -create-xcframework "${XCF_ARGS[@]}" -output "${XCFRAMEWORK}"

rm -rf "${BUILD_TMP}"

echo
echo "✓ Done."
echo "  Swift bindings: ${SWIFT_OUT}/fond_ffi.swift"
echo "  Framework:      ${XCFRAMEWORK}"
echo
echo "Next: cd apple/FondApp && xcodegen generate && open FondApp.xcodeproj"
