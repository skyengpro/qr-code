//! Keeps the `flutter_rust_bridge` entry points in iOS release links.
//!
//! On iOS the library is a static archive, and Dart finds the bridge's C functions with
//! `DynamicLibrary.process()`, so nothing native references them: the linker would leave
//! their archive members out, and `-dead_strip` would drop what remained. [`FRB_SYMBOLS`]
//! takes the address of every symbol the Dart side looks up; `#[used]` marks it
//! no-dead-strip, and `pc_abi_version` (which the Swift `SymbolKeeper` calls) reads it, so
//! the object holding it is always linked. Android ships a `cdylib`, which exports these
//! symbols anyway.
//!
//! The list is the `flutter_rust_bridge` 2.13 runtime (looked up by the Dart package
//! `flutter_rust_bridge`) plus the generated `frbgen_*` functions (looked up by
//! `flutter/particle_code/lib/src/rust/frb_generated.io.dart`). The test below fails when a
//! regeneration adds a `frbgen_*` lookup that is missing here; check the runtime list when
//! bumping `flutter_rust_bridge`.

// The declarations only take addresses; the real signatures live in flutter_rust_bridge
// and in frb_generated.rs, which is why they clash and why nothing here is ever called.
#![allow(unsafe_code, clashing_extern_declarations)]

macro_rules! keep {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(fn $name();)*
        }

        /// Addresses of every bridge symbol Dart looks up (see the module docs).
        #[used]
        pub(crate) static FRB_SYMBOLS: [unsafe extern "C" fn(); keep!(@count $($name)*)] =
            [$($name),*];

        /// Names of [`FRB_SYMBOLS`], in the same order.
        #[cfg(test)]
        pub(crate) const FRB_SYMBOL_NAMES: &[&str] = &[$(stringify!($name)),*];
    };
    (@count) => { 0 };
    (@count $head:ident $($tail:ident)*) => { 1 + keep!(@count $($tail)*) };
}

keep!(
    // flutter_rust_bridge 2.13 runtime.
    frb_create_shutdown_callback,
    frb_dart_fn_deliver_output,
    frb_dart_opaque_dart2rust_encode,
    frb_dart_opaque_drop_thread_box_persistent_handle,
    frb_dart_opaque_rust2dart_decode,
    frb_free_wire_sync_rust2dart_dco,
    frb_free_wire_sync_rust2dart_sse,
    frb_get_rust_content_hash,
    frb_init_frb_dart_api_dl,
    frb_pde_ffi_dispatcher_primary,
    frb_pde_ffi_dispatcher_sync,
    frb_rust_vec_u8_free,
    frb_rust_vec_u8_new,
    frb_rust_vec_u8_resize,
    store_dart_post_cobject,
    // Generated for the opaque ParticleEncoder / ParticleDecoder handles.
    frbgen_particle_code_rust_arc_decrement_strong_count_RustOpaque_flutter_rust_bridgefor_generatedRustAutoOpaqueInnerParticleDecoder,
    frbgen_particle_code_rust_arc_decrement_strong_count_RustOpaque_flutter_rust_bridgefor_generatedRustAutoOpaqueInnerParticleEncoder,
    frbgen_particle_code_rust_arc_increment_strong_count_RustOpaque_flutter_rust_bridgefor_generatedRustAutoOpaqueInnerParticleDecoder,
    frbgen_particle_code_rust_arc_increment_strong_count_RustOpaque_flutter_rust_bridgefor_generatedRustAutoOpaqueInnerParticleEncoder,
);

#[cfg(test)]
mod tests {
    use super::FRB_SYMBOL_NAMES;

    /// Every `frbgen_*` function the generated Dart code looks up is kept.
    #[test]
    fn keeps_every_generated_lookup() {
        let dart =
            include_str!("../../../flutter/particle_code/lib/src/rust/frb_generated.io.dart");
        let mut found = 0;
        for chunk in dart.split('\'').skip(1).step_by(2) {
            if chunk.starts_with("frbgen_") {
                found += 1;
                assert!(
                    FRB_SYMBOL_NAMES.contains(&chunk),
                    "{chunk} is not in FRB_SYMBOLS"
                );
            }
        }
        assert!(
            found > 0,
            "no frbgen_ lookups found; did the generated file move?"
        );
    }
}
