import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing (docs/RELEASING.md): the keystore named by KILN_KEYSTORE, unlocked with
// KILN_KEYSTORE_PASSWORD; the key is KILN_KEY_ALIAS (default `kiln`) with KILN_KEY_PASSWORD
// (default: the keystore's password). Only when KILN_KEYSTORE is not set at all is the release
// APK signed with the debug key (local builds, forks), which installs but cannot update an
// install signed with the release key. A KILN_KEYSTORE that is not a file, or one without its
// password, stops the build: a release must never fall back to the debug key by mistake.
val releaseKeystore: File? = System.getenv("KILN_KEYSTORE")?.let { path ->
    if (path.isBlank()) {
        throw GradleException("KILN_KEYSTORE is set but empty: name the release keystore, or unset it to sign with the debug key.")
    }
    val keystore = file(path)
    if (!keystore.isFile) {
        throw GradleException("KILN_KEYSTORE names $path, which is not a file: the release keystore is missing.")
    }
    if (System.getenv("KILN_KEYSTORE_PASSWORD").isNullOrEmpty()) {
        throw GradleException("KILN_KEYSTORE is set but KILN_KEYSTORE_PASSWORD is empty: the release keystore cannot be opened.")
    }
    keystore
}

android {
    compileSdk = 36
    namespace = "io.github.zahakj.kiln"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "io.github.zahakj.kiln"
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = releaseKeystore
                storePassword = System.getenv("KILN_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KILN_KEY_ALIAS")?.takeIf { it.isNotBlank() } ?: "kiln"
                keyPassword = System.getenv("KILN_KEY_PASSWORD")?.takeIf { it.isNotEmpty() }
                    ?: System.getenv("KILN_KEYSTORE_PASSWORD")
            }
        }
    }
    packaging {
        // The core is one large library: stored compressed, the APK stays within budget B13.
        jniLibs.useLegacyPackaging = true
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
