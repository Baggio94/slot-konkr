plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "fyi.slot.konkr"
    compileSdk = 35

    defaultConfig {
        applicationId = "fyi.slot.konkr"
        minSdk = 31
        targetSdk = 32
        versionCode = 6
        versionName = "0.0.5-dev1"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    // Reuse the exact upstream Slot insert/eject PCM assets.
    sourceSets.getByName("main").assets.srcDir("../../crates/slot/assets")

    packaging {
        jniLibs { useLegacyPackaging = true }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
