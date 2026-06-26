//! Standalone UniFFI binding generator.
//!
//! Built with `--features bindgen` and invoked by
//! `apple/build-xcframework.sh` to emit the Swift bindings from the
//! compiled library's metadata.

fn main() {
    uniffi::uniffi_bindgen_main()
}
