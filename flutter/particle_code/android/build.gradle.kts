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

android {
    namespace = "com.vaam.particlecode.flutter"

    compileSdk = 36

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        minSdk = 24
        // JNI looks up DirectBuffers.address by name; a minified app must not rename it.
        consumerProguardFiles("consumer-rules.pro")
    }

    // libparticle_code_jni.so: a few lines of C that give Kotlin the address of a direct
    // ByteBuffer. No Rust here; the decoder is reached from Dart through flutter_rust_bridge.
    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    val cameraX = "1.4.1"
    implementation("androidx.camera:camera-core:$cameraX")
    implementation("androidx.camera:camera-camera2:$cameraX")
    implementation("androidx.camera:camera-lifecycle:$cameraX")
    implementation("androidx.camera:camera-view:$cameraX") // PreviewView

    implementation("androidx.core:core:1.13.1") // ActivityCompat, ContextCompat
    implementation("androidx.lifecycle:lifecycle-common:2.8.7") // LifecycleOwner
    implementation("androidx.lifecycle:lifecycle-livedata-core:2.8.7") // CameraState observer
}
