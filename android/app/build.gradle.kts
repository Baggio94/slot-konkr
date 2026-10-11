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
        // RC1 consolidates the stacked Cart Studio (#3) and native
        // RetroAchievements (#4) branches. The original applicationId and
        // persistent signing certificate remain unchanged for in-place updates.
        versionCode = 43
        versionName = "0.1.0-rc4-beta3"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    // A stable signing key can be provided by GitHub Actions secrets or a
    // local development environment. Never commit the keystore or credentials.
    // Without these variables Gradle falls back to its ephemeral debug key.
    val persistentStore = System.getenv("SLOT_SIGNING_STORE_FILE")
    if (!persistentStore.isNullOrBlank()) {
        signingConfigs {
            create("slotPersistent") {
                storeFile = file(persistentStore)
                storePassword = System.getenv("SLOT_SIGNING_STORE_PASSWORD")
                    ?: error("SLOT_SIGNING_STORE_PASSWORD is required")
                keyAlias = System.getenv("SLOT_SIGNING_KEY_ALIAS")
                    ?: error("SLOT_SIGNING_KEY_ALIAS is required")
                keyPassword = System.getenv("SLOT_SIGNING_KEY_PASSWORD")
                    ?: error("SLOT_SIGNING_KEY_PASSWORD is required")
            }
        }
        buildTypes {
            getByName("debug") {
                signingConfig = signingConfigs.getByName("slotPersistent")
            }
        }
    }

    // Reuse the exact upstream Slot insert/eject PCM assets.
    sourceSets.getByName("main").assets.srcDir("../../crates/slot/assets")
    // Original Slot label font (SIL OFL) for a visually consistent account panel.
    // Include the accompanying OFL license from the same upstream source directory.
    sourceSets.getByName("main").assets.srcDir("../../crates/slot-ui/assets")

    packaging {
        jniLibs { useLegacyPackaging = true }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
