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
        // Public Android candidate built from the consolidated Multiplayer Lab.
        // Same package ID and signing key: install -r must preserve user data.
        // U1 QA: monotonically higher than public RC1, normal package and signer.
        versionCode = 92
        versionName = "0.1.0-rc.2-u1pause"
        // One byte-identical APK on ARM64 KONKR and x86_64 Mac AVD.
        // Native libraries for BOTH ABIs are built/pinned by the same CI.
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    // This APK is distributed directly for KONKR Android 12 (not via Google
    // Play). Keep targetSdk 32 to preserve the validated system/input behavior;
    // suppress only Play's expired-target submission check, not other lint.
    lint {
        disable += "ExpiredTargetSdkVersion"
    }

    // An install-alongside standalone QA app. This NEVER replaces the
    // physical console's fyi.slot.konkr beta or reads its app-private saves.
    // Its own package is fyi.slot.konkr.lab, with a separate HOME-free manifest.
    buildTypes {
        create("multiplayerLab") {
            initWith(getByName("debug"))
            applicationIdSuffix = ".lab"
            versionNameSuffix = "-sidecar"
            matchingFallbacks += listOf("debug")
        }
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
            getByName("multiplayerLab") {
                // Same update-stable certificate when secrets are available,
                // but a distinct app ID even if those secrets are absent.
                signingConfig = signingConfigs.getByName("slotPersistent")
            }
            getByName("release") {
                // Release candidates must be non-debuggable while preserving
                // the same update-compatible signing certificate as debug RCs.
                isDebuggable = false
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
