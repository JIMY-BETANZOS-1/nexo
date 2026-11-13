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
        versionCode = 3
        versionName = "1.1"
    }

    // Firma de publicación: la llave llega por variables de entorno (secretos de GitHub o
    // claves/ en la PC). Sin ellas se firma con la llave de depuración.
    val llave = System.getenv("NEXO_KEYSTORE")?.let { file(it) }?.takeIf { it.exists() }
    signingConfigs {
        if (llave != null) {
            create("publicacion") {
                storeFile = llave
                storePassword = System.getenv("NEXO_KEYSTORE_PASS")
                keyAlias = System.getenv("NEXO_KEY_ALIAS") ?: "nexo"
                keyPassword = System.getenv("NEXO_KEYSTORE_PASS")
            }
        }
    }

    buildTypes {
        release {
            // Código mínimo y recursos recortados: APK pequeño, menos RAM.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.findByName("publicacion") ?: signingConfigs.getByName("debug")
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
