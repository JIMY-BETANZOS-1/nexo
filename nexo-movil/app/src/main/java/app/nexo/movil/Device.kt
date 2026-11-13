package app.nexo.movil

import android.app.ActivityManager
import android.app.AppOpsManager
import android.app.KeyguardManager
import android.app.admin.DevicePolicyManager
import android.app.usage.StorageStatsManager
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ApplicationInfo
import android.net.Uri
import android.os.BatteryManager
import android.os.Build
import android.os.Environment
import android.os.Process
import android.os.StatFs
import android.os.storage.StorageManager
import android.provider.Settings
import java.text.SimpleDateFormat
import java.util.Locale

/** Lecturas del celular. Todo local; nada sale del teléfono. */
object Device {
    fun name(): String = "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"
    val brand: String get() = Build.MANUFACTURER.lowercase()

    fun storage(): Pair<Long, Long> {
        val s = StatFs(Environment.getDataDirectory().path)
        return s.availableBytes to s.totalBytes
    }

    fun memory(ctx: Context): ActivityManager.MemoryInfo {
        val mi = ActivityManager.MemoryInfo()
        ctx.getSystemService(ActivityManager::class.java).getMemoryInfo(mi)
        return mi
    }

    fun isLowRam(ctx: Context) = ctx.getSystemService(ActivityManager::class.java).isLowRamDevice

    class Bat(
        val level: Int, val tempC: Float, val voltMv: Int, val health: Int, val charging: Boolean,
        val plug: String, val tech: String, val designMah: Double?, val estFullMah: Double?,
    ) {
        val healthText get() = when (health) {
            BatteryManager.BATTERY_HEALTH_GOOD -> "Buena"
            BatteryManager.BATTERY_HEALTH_OVERHEAT -> "Sobrecalentada"
            BatteryManager.BATTERY_HEALTH_DEAD -> "Agotada"
            BatteryManager.BATTERY_HEALTH_OVER_VOLTAGE -> "Sobretensión"
            BatteryManager.BATTERY_HEALTH_COLD -> "Demasiado fría"
            else -> "Sin datos"
        }
    }

    fun battery(ctx: Context): Bat {
        val i = ctx.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = i?.let { it.getIntExtra(BatteryManager.EXTRA_LEVEL, 0) * 100 / it.getIntExtra(BatteryManager.EXTRA_SCALE, 100).coerceAtLeast(1) } ?: 0
        val plugged = i?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
        val status = i?.getIntExtra(BatteryManager.EXTRA_STATUS, 0) ?: 0
        val bm = ctx.getSystemService(BatteryManager::class.java)
        val counter = bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CHARGE_COUNTER) // µAh
        val design = designCapacity(ctx)
        // Solo se muestra si el dato es coherente (algunos fabricantes y emuladores dan valores falsos).
        val est = if (counter > 0 && level >= 20) counter / 1000.0 / (level / 100.0) else null
        val sane = design != null && est != null && design >= 1500 && est in design * 0.3..design * 1.15
        return Bat(
            level = level,
            tempC = (i?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, 0) ?: 0) / 10f,
            voltMv = i?.getIntExtra(BatteryManager.EXTRA_VOLTAGE, 0) ?: 0,
            health = i?.getIntExtra(BatteryManager.EXTRA_HEALTH, 0) ?: 0,
            charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL,
            plug = when (plugged) {
                BatteryManager.BATTERY_PLUGGED_AC -> "cargador"
                BatteryManager.BATTERY_PLUGGED_USB -> "USB"
                BatteryManager.BATTERY_PLUGGED_WIRELESS -> "inalámbrica"
                else -> ""
            },
            tech = i?.getStringExtra(BatteryManager.EXTRA_TECHNOLOGY) ?: "",
            designMah = if (sane) design else null,
            estFullMah = if (sane) est else null,
        )
    }

    /** Capacidad de fábrica según el perfil de energía del sistema (si el fabricante lo expone). */
    private var designRead = false
    private var designCache: Double? = null

    private fun designCapacity(ctx: Context): Double? {
        // No cambia nunca: se lee una sola vez (leer el perfil cuesta).
        if (designRead) return designCache
        designCache = try {
            val cls = Class.forName("com.android.internal.os.PowerProfile")
            val pp = cls.getConstructor(Context::class.java).newInstance(ctx)
            (cls.getMethod("getBatteryCapacity").invoke(pp) as Double).takeIf { it > 500 }
        } catch (e: Throwable) {
            null
        }
        designRead = true
        return designCache
    }

    fun hasAllFiles() = Environment.isExternalStorageManager()

    fun hasUsage(ctx: Context): Boolean {
        val ops = ctx.getSystemService(AppOpsManager::class.java)
        return ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ctx.packageName) == AppOpsManager.MODE_ALLOWED
    }

    fun label(ctx: Context, pkg: String): String = try {
        ctx.packageManager.getApplicationLabel(ctx.packageManager.getApplicationInfo(pkg, 0)).toString()
    } catch (e: Exception) {
        pkg
    }

    fun appSettings(pkg: String) = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$pkg"))

    /** Apps más usadas en las últimas 24 h (tiempo en pantalla). */
    fun topUsage(ctx: Context, n: Int): List<Triple<String, String, Long>> {
        if (!hasUsage(ctx)) return emptyList()
        val usm = ctx.getSystemService(UsageStatsManager::class.java)
        val end = System.currentTimeMillis()
        val stats = usm.queryUsageStats(UsageStatsManager.INTERVAL_BEST, end - 86_400_000L, end) ?: return emptyList()
        return stats.groupBy { it.packageName }
            .map { (pkg, l) -> pkg to l.sumOf { it.totalTimeInForeground } }
            .filter { it.second > 60_000 && it.first != ctx.packageName }
            .sortedByDescending { it.second }
            .take(n)
            .map { Triple(label(ctx, it.first), it.first, it.second) }
    }

    /** Apps con más caché (Android 11 no deja borrarla desde otra app: se abre su ajuste). */
    fun appCaches(ctx: Context, n: Int): List<Triple<String, String, Long>> {
        if (!hasUsage(ctx)) return emptyList()
        val ssm = ctx.getSystemService(StorageStatsManager::class.java)
        val user = Process.myUserHandle()
        return ctx.packageManager.getInstalledApplications(0).mapNotNull { ai ->
            try {
                val st = ssm.queryStatsForPackage(StorageManager.UUID_DEFAULT, ai.packageName, user)
                Triple(ctx.packageManager.getApplicationLabel(ai).toString(), ai.packageName, st.cacheBytes)
            } catch (e: Exception) {
                null
            }
        }.filter { it.third > 5L * 1024 * 1024 }.sortedByDescending { it.third }.take(n)
    }

    // ───────────── Protección ─────────────

    class Check(val state: Int, val title: String, val detail: String, val intent: Intent? = null)
    class AppFlag(val label: String, val pkg: String, val reason: String)

    private const val STORE_INSTALLERS = "com.android.vending com.google.android.feedback com.hihonor.appmarket com.huawei.appmarket com.heytap.market com.oppo.market com.xiaomi.market com.sec.android.app.samsungapps com.amazon.venezia"

    fun securityChecks(ctx: Context, onProgress: (Float) -> Unit = {}): Pair<List<Check>, List<AppFlag>> {
        val checks = ArrayList<Check>()
        val flags = ArrayList<AppFlag>()
        val cr = ctx.contentResolver
        val pm = ctx.packageManager

        val secure = ctx.getSystemService(KeyguardManager::class.java).isDeviceSecure
        checks += Check(if (secure) 0 else 2, "Bloqueo de pantalla", if (secure) "Activado (PIN, patrón o huella)" else "SIN bloqueo: cualquiera puede entrar a tu celular", Intent(Settings.ACTION_SECURITY_SETTINGS))

        val patch = Build.VERSION.SECURITY_PATCH
        val months = try {
            val d = SimpleDateFormat("yyyy-MM-dd", Locale.US).parse(patch)!!
            ((System.currentTimeMillis() - d.time) / (30L * 86_400_000L)).toInt()
        } catch (e: Exception) {
            -1
        }
        checks += Check(
            if (months in 0..6) 0 else if (months >= 7) 1 else 3,
            "Parche de seguridad",
            when {
                months < 0 -> patch
                months <= 6 -> "$patch (hace $months meses)"
                else -> "$patch (hace $months meses). Si tu modelo ya no recibe actualizaciones, instala apps solo desde la tienda oficial."
            },
            Intent(Settings.ACTION_DEVICE_INFO_SETTINGS),
        )

        val adb = Settings.Global.getInt(cr, Settings.Global.ADB_ENABLED, 0) == 1
        checks += Check(if (adb) 1 else 0, "Depuración USB", if (adb) "Activada: apágala cuando no uses NEXO desde la laptop" else "Desactivada", Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))

        // Servicios de accesibilidad: pueden leer y tocar la pantalla por ti.
        val acc = Settings.Secure.getString(cr, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES).orEmpty()
        acc.split(':').filter { it.isNotBlank() }.map { it.substringBefore('/') }.distinct().forEach { pkg ->
            if (!isSystem(ctx, pkg)) flags += AppFlag(label(ctx, pkg), pkg, "Accesibilidad: puede leer y controlar la pantalla")
        }
        // Lectores de notificaciones: ven todos tus mensajes.
        val nl = Settings.Secure.getString(cr, "enabled_notification_listeners").orEmpty()
        nl.split(':').filter { it.isNotBlank() }.map { it.substringBefore('/') }.distinct().forEach { pkg ->
            if (!isSystem(ctx, pkg)) flags += AppFlag(label(ctx, pkg), pkg, "Lee todas tus notificaciones")
        }
        // Administradores del dispositivo.
        ctx.getSystemService(DevicePolicyManager::class.java).activeAdmins?.forEach { cn ->
            if (!isSystem(ctx, cn.packageName)) flags += AppFlag(label(ctx, cn.packageName), cn.packageName, "Administrador del dispositivo")
        }

        val ops = ctx.getSystemService(AppOpsManager::class.java)
        val user = pm.getInstalledApplications(0).filter { it.flags and ApplicationInfo.FLAG_SYSTEM == 0 && it.packageName != ctx.packageName }
        var sideloaded = 0
        for ((i, ai) in user.withIndex()) {
            if (i % 5 == 0) onProgress(0.2f + 0.8f * i / user.size.coerceAtLeast(1))
            val lbl = pm.getApplicationLabel(ai).toString()
            fun allowed(op: String) = try {
                ops.unsafeCheckOpNoThrow(op, ai.uid, ai.packageName) == AppOpsManager.MODE_ALLOWED
            } catch (e: Exception) {
                false
            }
            if (allowed("android:request_install_packages")) flags += AppFlag(lbl, ai.packageName, "Puede instalar otras apps")
            if (allowed(AppOpsManager.OPSTR_SYSTEM_ALERT_WINDOW)) flags += AppFlag(lbl, ai.packageName, "Puede mostrarse encima de otras apps")
            val src = try { pm.getInstallSourceInfo(ai.packageName).installingPackageName } catch (e: Exception) { null }
            if (src == null || !STORE_INSTALLERS.contains(src)) {
                sideloaded++
                flags += AppFlag(lbl, ai.packageName, "Instalada fuera de las tiendas oficiales")
            }
        }
        val strong = flags.count { it.reason.startsWith("Accesibilidad") || it.reason.startsWith("Lee todas") || it.reason.startsWith("Administrador") }
        checks += Check(if (strong == 0) 0 else 1, "Permisos delicados", if (strong == 0) "Ninguna app de terceros con accesibilidad, notificaciones o administración" else "$strong apps con permisos muy amplios: revisa que las reconozcas")
        checks += Check(if (sideloaded == 0) 0 else 3, "Apps fuera de tiendas", if (sideloaded == 0) "Todas vienen de tiendas oficiales" else "$sideloaded instaladas por otros medios (como NEXO)")
        return checks to flags.distinctBy { it.pkg + it.reason }
    }

    private fun isSystem(ctx: Context, pkg: String) = try {
        ctx.packageManager.getApplicationInfo(pkg, 0).flags and ApplicationInfo.FLAG_SYSTEM != 0
    } catch (e: Exception) {
        false
    }

    fun securityScore(checks: List<Check>, flags: List<AppFlag>): Int {
        var s = 100
        for (c in checks) s -= when (c.state) { 2 -> 35; 1 -> 12; else -> 0 } // los avisos informativos no restan
        s -= flags.count { it.reason.startsWith("Accesibilidad") || it.reason.startsWith("Administrador") } * 5
        return s.coerceIn(0, 100)
    }

    // ───────────── Consejos por modelo ─────────────

    class Tip(val title: String, val detail: String, val intent: Intent?)

    fun tips(ctx: Context): List<Tip> {
        val battery = Intent(Settings.ACTION_BATTERY_SAVER_SETTINGS)
        val display = Intent(Settings.ACTION_DISPLAY_SETTINGS)
        val dev = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)
        val apps = Intent(Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS)
        val usage = Intent(Intent.ACTION_POWER_USAGE_SUMMARY)
        val common = listOf(
            Tip("Revisa qué gasta la batería", "Ajustes › Batería › Uso: si una app que casi no usas sale arriba, restríngela en segundo plano.", usage),
            Tip("Brillo automático y suspensión en 30 s", "La pantalla es lo que más batería gasta.", display),
        )
        val brandTips = when {
            brand.contains("realme") || brand.contains("oppo") -> listOf(
                Tip("Gestión de apps en segundo plano", "Ajustes › Batería › Más ajustes › Optimizar uso de batería: deja en «Optimizar» las apps que no necesitas recibir al momento.", battery),
                Tip("Animaciones a 0,5x", "Opciones de desarrollador › Escala de animación de ventana/transición/duración en 0,5x: el celular se siente mucho más rápido.", dev),
                Tip("Usa apps Lite o Go", "Facebook Lite, Messenger Lite, Google Go, Maps Go y Galería Go usan mucha menos RAM.", null),
                Tip("Evita fondos animados y widgets en vivo", "Consumen RAM y batería todo el tiempo.", display),
            )
            brand.contains("honor") || brand.contains("huawei") -> listOf(
                Tip("Inicio de aplicaciones", "Ajustes › Batería › Inicio de aplicaciones: gestiona manualmente las que no necesitas (inicio automático y en segundo plano apagados).", battery),
                Tip("Frecuencia de pantalla", "Ajustes › Pantalla y brillo › Frecuencia de actualización: «Inteligente» o 60 Hz ahorra batería frente a 90 Hz.", display),
                Tip("Modo oscuro", "Ajustes › Pantalla y brillo › Modo oscuro: menos consumo y menos cansancio visual.", display),
                Tip("Modo de rendimiento solo para juegos", "Ajustes › Batería: déjalo apagado el resto del tiempo.", battery),
            )
            else -> listOf(Tip("Apps en segundo plano", "Restringe las apps que no necesitan avisarte al momento.", apps))
        }
        val ram = if (isLowRam(ctx)) listOf(Tip("Tu celular es Android Go", "Prioriza apps «Go» y «Lite» y evita tener muchas abiertas a la vez.", null)) else emptyList()
        return brandTips + common + ram
    }
}
