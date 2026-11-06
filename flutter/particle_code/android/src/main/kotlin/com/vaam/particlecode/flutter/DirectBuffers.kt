package com.vaam.particlecode.flutter

import java.nio.ByteBuffer

/** Address of direct buffers, from `libparticle_code_jni.so` (`src/main/cpp`). */
internal object DirectBuffers {
    /** False when the library could not be loaded; then no frame can be handed to Dart. */
    val available: Boolean = try {
        System.loadLibrary("particle_code_jni")
        true
    } catch (e: UnsatisfiedLinkError) {
        false
    }

    /** Address of the buffer's first byte (index 0, not its position); 0 if it is not direct. */
    @JvmStatic
    external fun address(buffer: ByteBuffer): Long

    /** Address of the buffer's readable bytes (from its position), or 0. */
    fun readableAddress(buffer: ByteBuffer): Long {
        if (!buffer.isDirect) return 0L
        val base = address(buffer)
        return if (base == 0L) 0L else base + buffer.position()
    }
}
