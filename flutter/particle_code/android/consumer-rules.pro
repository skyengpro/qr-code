# JNI resolves DirectBuffers.address by name: keep the class and its native method un-renamed.
-keep class com.vaam.particlecode.flutter.DirectBuffers { *; }
-keepclasseswithmembernames class * { native <methods>; }
