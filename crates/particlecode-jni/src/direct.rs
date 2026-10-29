//! Borrowing the memory of a direct `java.nio.ByteBuffer`: the one place where a JVM-owned
//! address becomes a Rust slice.

#![allow(unsafe_code)] // `slice::from_raw_parts` over `GetDirectBufferAddress`.

use jni::Env;
use jni::objects::JByteBuffer;

use crate::layout::{ERR_NOT_DIRECT, ERR_NULL};

/// The bytes of a direct buffer, from its base address for its full capacity.
///
/// `position` and `limit` are ignored: the view starts at the buffer's own address (a
/// `slice()` or `duplicate()` already has the right base) and covers `capacity()` bytes. The
/// Kotlin side makes sure `position() == 0`.
///
/// `Err(ERR_NULL)` for a null reference, `Err(ERR_NOT_DIRECT)` for a heap buffer (or a JVM
/// without direct-buffer access).
///
/// The slice borrows `buf`, which must stay referenced from Java for as long as the slice is
/// used: native methods keep their arguments reachable until they return, and the caller
/// (`ImageProxy`) does not touch or close the image meanwhile. Camera memory is not
/// written by anyone during that time.
pub(crate) fn direct_slice<'a>(env: &Env<'_>, buf: &'a JByteBuffer<'_>) -> Result<&'a [u8], i32> {
    if buf.is_null() {
        return Err(ERR_NULL);
    }
    let ptr = env
        .get_direct_buffer_address(buf)
        .map_err(|_| ERR_NOT_DIRECT)?;
    let len = env
        .get_direct_buffer_capacity(buf)
        .map_err(|_| ERR_NOT_DIRECT)?;
    if ptr.is_null() {
        return Err(ERR_NOT_DIRECT);
    }
    if len == 0 {
        return Ok(&[]);
    }
    if isize::try_from(len).is_err() {
        return Err(ERR_NOT_DIRECT);
    }
    // SAFETY: `ptr` is non-null and is the start of the buffer's native memory, which is
    // `len` bytes long (`GetDirectBufferCapacity`). The memory is owned by the JVM object
    // `buf` that this borrow is tied to, so it is not freed while the slice lives, and
    // nothing writes to camera planes while a frame is being analyzed. `len` fits in
    // `isize` (checked above) and `u8` has no alignment requirement.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}
