import ParticleCodeFFI

/// Keeps the Rust library's C entry points in the linked app.
///
/// Dart reaches the sender through flutter_rust_bridge, whose C symbols it looks up with
/// `DynamicLibrary.process()`; nothing in native code calls them, and the linker is free to
/// drop them: an archive member nobody references is not loaded, and `-dead_strip` removes
/// unreachable functions from the ones that are. `keep()` calls `pc_abi_version()`, which
/// reads the Rust-side table in `crates/particlecode-ffi/src/frb_keep.rs` that holds every
/// bridge symbol Dart needs. It also references every `pc_*` function declared in
/// `particlecode.h`, so the C ABI stays available to native code in the app.
///
/// The references sit behind `isEnabled`, a public mutable flag the optimizer cannot fold,
/// and it is `false`, so none of these calls ever runs. Calls with `nil`/`0` arguments are
/// used instead of function-pointer casts because they only depend on each function's arity,
/// not on how Swift imports its C types.
///
/// KEEP THIS LIST IN STEP WITH `crates/particlecode-ffi/include/particlecode.h`:
/// `ios/tool/check-symbols.sh` fails when a function is missing here.
public enum ParticleCodeSymbols {
    /// Always `false`. Public and mutable only so that the compiler cannot prove it.
    public static var isEnabled = false

    /// Call at registration. Returns the ABI version of the linked library, which is
    /// also a cheap check that the xcframework is really linked.
    @discardableResult
    @inline(never)
    public static func keep() -> UInt32 {
        if isEnabled { referenceAll() }
        return pc_abi_version()
    }

    @inline(never)
    private static func referenceAll() {
        _ = pc_abi_version()
        pc_string_free(nil)
        pc_bytes_free(nil, 0)
        _ = pc_liveness_verify(nil, 0, 0, 0, 0)

        _ = pc_encoder_new(nil, 0, nil, 0, nil)
        pc_encoder_free(nil)
        _ = pc_encoder_frame_json(nil, 0)
        _ = pc_encoder_frame_json_at(nil, 0, 0)
        _ = pc_encoder_info_json(nil)
        _ = pc_encoder_frames_per_pass(nil)
        _ = pc_encoder_key(nil, nil)

        _ = pc_decoder_new()
        pc_decoder_free(nil)
        _ = pc_decoder_feed_symbols(nil, nil, nil, 0)
        _ = pc_decoder_feed_yuv(nil, 0, 0, nil, 0, nil, 0, nil, 0, 0, 0, 0, nil)
        _ = pc_decoder_feed_rgba(nil, 0, 0, nil, 0, nil)
        _ = pc_decoder_hint(nil)
        _ = pc_decoder_progress(nil, nil)
        _ = pc_decoder_reject_reason(nil)
        _ = pc_decoder_take_ready(nil, nil, nil)
        _ = pc_decoder_session_id(nil, nil)
    }
}
