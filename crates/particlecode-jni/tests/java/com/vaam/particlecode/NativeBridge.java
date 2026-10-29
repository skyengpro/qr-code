package com.vaam.particlecode;

import java.nio.ByteBuffer;

/**
 * Test twin of the Kotlin {@code object NativeBridge}: the same fully qualified name and the
 * same native method signatures, so the JVM resolves the same {@code Java_com_vaam_...}
 * symbols in {@code libparticlecode_jni}. Keep in step with NativeBridge.kt.
 */
public final class NativeBridge {
    static {
        System.load(System.getProperty("particlecode.jni"));
    }

    private NativeBridge() {}

    public static native long nativeNew();

    public static native void nativeFree(long handle);

    public static native int nativeReset(long handle);

    public static native int nativeFeedYuv(
            long handle, int width, int height,
            ByteBuffer y, ByteBuffer u, ByteBuffer v,
            int yRowStride, int uvRowStride, int uvPixelStride,
            int[] out, float[] outGeom);

    public static native int nativeFeedYuvBytes(
            long handle, int width, int height,
            byte[] y, byte[] u, byte[] v,
            int yRowStride, int uvRowStride, int uvPixelStride,
            int[] out, float[] outGeom);

    public static native byte[] nativeTakeReady(long handle);

    public static native byte[] nativePayload(long handle);

    public static native String nativeRejectReason(long handle);

    public static native String nativeHint(long handle);

    public static native int nativeSessionId(long handle);

    public static native boolean nativeVerifyLiveness(byte[] key, int rolling, long nowUnix, long skew);
}
