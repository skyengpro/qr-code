group = "com.vaam.particlecode.flutter"
version = "1.0-SNAPSHOT"

buildscript {
    val kotlinVersion = "2.4.0"
    repositories {
        google()
        mavenCentral()
    }

    dependencies {
        classpath("com.android.tools.build:gradle:9.1.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:$kotlinVersion")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

plugins {
    id("com.android.library")
}

// The Kotlin scanner library and its native libraries live in the monorepo's android/particlecode
// module. This plugin compiles those sources and packages those jniLibs directly (no copy, no AAR).
// From flutter/particle_code/android the repository root is ../../.. .
val particleCodeLibrary = file("../../../android/particlecode")
val particleCodeJniLibs = file("../../../android/particlecode/src/main/jniLibs")

android {
    namespace = "com.vaam.particlecode.flutter"

    compileSdk = 36

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            java.srcDirs(
                "src/main/kotlin",
                // com.vaam.particlecode: NativeBridge, ParticleScanner, ParticleAnalyzer.
                "$particleCodeLibrary/src/main/kotlin",
            )
            // libparticlecode_jni.so per ABI, written by scripts/build-android.sh.
            jniLibs.srcDirs(particleCodeJniLibs)
        }
    }

    defaultConfig {
        minSdk = 24
        // JNI looks up NativeBridge by name; a minified app must not rename it.
        consumerProguardFiles("$particleCodeLibrary/consumer-rules.pro")
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    // Keep in step with android/particlecode/build.gradle.kts (camera-core 1.4.1).
    val cameraX = "1.4.1"
    implementation("androidx.camera:camera-core:$cameraX")
    implementation("androidx.camera:camera-camera2:$cameraX")
    implementation("androidx.camera:camera-lifecycle:$cameraX")
    implementation("androidx.camera:camera-view:$cameraX") // PreviewView

    implementation("androidx.core:core:1.13.1") // ActivityCompat, ContextCompat
    implementation("androidx.lifecycle:lifecycle-common:2.8.7") // LifecycleOwner
    implementation("androidx.lifecycle:lifecycle-livedata-core:2.8.7") // CameraState observer
}

// The .so files are not checked in: scripts/build-android.sh builds them from the Rust crates.
// Without them the app would build fine and crash with UnsatisfiedLinkError on the first scan,
// so fail early with instructions.
val checkParticleCodeNativeLibs = tasks.register("checkParticleCodeNativeLibs") {
    val dir = particleCodeJniLibs
    doLast {
        val found = dir.walkTopDown().filter { it.isFile && it.name == "libparticlecode_jni.so" }.toList()
        if (found.isEmpty()) {
            throw GradleException(
                "particle_code: no libparticlecode_jni.so under $dir.\n" +
                    "Build the native libraries first, from the repository root:\n" +
                    "    cargo install --locked cargo-ndk      # once\n" +
                    "    scripts/build-android.sh              # add --debug for a faster build\n" +
                    "(needs the Android NDK; see android/README.md).",
            )
        }
        val abis = found.map { it.parentFile.name }.toSet()
        val missing = listOf("arm64-v8a", "armeabi-v7a", "x86_64").filter { it !in abis }
        if (missing.isNotEmpty()) {
            logger.warn(
                "particle_code: libparticlecode_jni.so is missing for ABI(s) $missing; " +
                    "apps for them will crash when scanning.",
            )
        }
    }
}

tasks.matching { it.name == "preBuild" }.configureEach {
    dependsOn(checkParticleCodeNativeLibs)
}
