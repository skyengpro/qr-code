//! # particlecode-jni
//!
//! The Android JNI shim of Particle Code. A native `CameraX` analyzer hands the three
//! `ImageProxy` planes (direct `ByteBuffer`s) straight to Rust on its background thread;
//! the decoder reads them in place and only progress numbers come back, through two
//! preallocated arrays. No frame ever crosses into Dart or allocates on the JVM heap.
//!
//! The exports are the static native methods of the Kotlin `object
//! com.vaam.particlecode.NativeBridge` (`@JvmStatic external fun ...`; a Java class of the
//! same name with `static native` methods yields the same symbols). They are thin: the
//! decoding is `particlecode_ffi::native`.
//!
//! * [`layout`] — status codes and the layout of the `out` / `outGeom` arrays.
//! * `registry` — handles are table keys, so a null or freed handle is an error code, not
//!   undefined behaviour.
//! * [`scanner`] — a decoder plus the polled state (counts, reject reason, hint, payload).
//! * `direct` and `exports` — the only modules with `unsafe`: borrowing a direct buffer and
//!   exporting the symbols.
//!
//! Threading: a handle is used by one thread at a time. The registry serializes calls on a
//! handle anyway, but the Kotlin wrapper holds a lock and that is the contract.

mod direct;
pub mod exports;
pub mod layout;
mod registry;
pub mod scanner;
