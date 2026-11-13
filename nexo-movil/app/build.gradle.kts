plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.nexo.movil"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.nexo.movil"
        minSdk = 30          // Android 11: Realme C11 2021 y Honor X7
        targetSdk = 34
        versionCode = 2
        versionName = "1.0"
    }

    buildTypes {
        release {
            // Código mínimo y recursos recortados: APK pequeño, menos RAM.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}
