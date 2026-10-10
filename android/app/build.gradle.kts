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
        versionCode = 28
        versionName = "0.0.7-dev19"
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
