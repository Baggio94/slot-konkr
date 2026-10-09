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
        versionCode = 3
        versionName = "0.0.3-dev1"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    packaging {
        jniLibs { useLegacyPackaging = true }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
