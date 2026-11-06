// Address of a direct java.nio.ByteBuffer. Kotlin cannot read it; the camera planes are handed to
// Dart (and from there to Rust) by address. This file is the whole native side of the plugin.

#include <jni.h>
#include <stdint.h>

JNIEXPORT jlong JNICALL
Java_com_vaam_particlecode_flutter_DirectBuffers_address(JNIEnv *env, jclass clazz, jobject buffer) {
    (void)clazz;
    if (buffer == NULL) return 0;
    void *p = (*env)->GetDirectBufferAddress(env, buffer);
    return (jlong)(intptr_t)p;
}
