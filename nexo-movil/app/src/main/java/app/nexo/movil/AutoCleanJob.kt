package app.nexo.movil

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.content.Intent

/**
 * Limpieza automática. La lanza Android solo mientras el celular se carga: 0 MB y
 * 0 batería el resto del tiempo. (No se exige "sin usar": en Realme y Honor ese
 * estado casi nunca llega y la tarea no se ejecutaría.)
 * Solo borra basura SEGURA. Capturas, descargas, videos y documentos: solo avisa.
 */
class AutoCleanJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        Thread {
            run(this)
            jobFinished(params, false)
        }.start()
        return true
    }

    override fun onStopJob(params: JobParameters) = true

    companion object {
        private const val JOB_ID = 7001

        fun schedule(ctx: Context, days: Int) {
            val js = ctx.getSystemService(JobScheduler::class.java)
            js.schedule(
                JobInfo.Builder(JOB_ID, ComponentName(ctx, AutoCleanJob::class.java))
                    .setRequiresCharging(true)
                    .setRequiresBatteryNotLow(true)
                    .setPeriodic(days * 86_400_000L)
                    .setPersisted(true)
                    .build()
            )
        }

        fun cancel(ctx: Context) = ctx.getSystemService(JobScheduler::class.java).cancel(JOB_ID)

        fun isScheduled(ctx: Context) = ctx.getSystemService(JobScheduler::class.java).getPendingJob(JOB_ID) != null

        fun run(ctx: Context) {
            if (!Device.hasAllFiles()) return
            val cats = Scanner(ctx) { _, _, _ -> }.scan()
            cats.forEach { it.selected = it.kind == Kind.SAFE }
            val (freed, files) = Cleaner.cleanSafe(cats)
            val review = cats.filter { it.kind != Kind.SAFE }.sumOf { it.size }
            Prefs.saveRun(ctx, freed, files, auto = true)
            val line2 = if (review > 0) "Para revisar (no se tocó): ${Fmt.bytes(review)} en capturas, duplicados, videos y documentos."
            else "Tus videos y documentos no se tocaron."
            notify(ctx, "Limpieza automática", "Liberados ${Fmt.bytes(freed)} ($files elementos de basura). $line2")

            val (free, total) = Device.storage()
            if (total > 0 && free * 100 / total < 10) {
                notify(ctx, "Almacenamiento casi lleno", "Quedan ${Fmt.bytes(free)}. Abre NEXO › Limpiar para revisar lo que sobra.", 2)
            }
            val bat = Device.battery(ctx)
            if (bat.tempC >= 45f) {
                notify(ctx, "La batería se calienta al cargar", "Está a ${bat.tempC} °C. Quita la funda y evita usarlo mientras carga.", 3)
            }
        }

        fun notify(ctx: Context, title: String, text: String, id: Int = 1) {
            val nm = ctx.getSystemService(NotificationManager::class.java)
            nm.createNotificationChannel(NotificationChannel("nexo", "Avisos de NEXO", NotificationManager.IMPORTANCE_DEFAULT))
            val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
            nm.notify(
                id,
                Notification.Builder(ctx, "nexo")
                    .setSmallIcon(R.drawable.ic_stat)
                    .setColor(C.CYAN)
                    .setContentTitle(title)
                    .setContentText(text)
                    .setStyle(Notification.BigTextStyle().bigText(text))
                    .setContentIntent(open)
                    .setAutoCancel(true)
                    .build()
            )
        }
    }
}

object Prefs {
    private fun p(ctx: Context) = ctx.getSharedPreferences("nexo", Context.MODE_PRIVATE)
    fun autoDays(ctx: Context) = p(ctx).getInt("auto_days", 7)
    // Lo que eligió el usuario (la tarea del sistema puede perderse si Android fuerza el cierre).
    fun autoEnabled(ctx: Context) = p(ctx).getBoolean("auto_enabled", false)
    fun setAutoEnabled(ctx: Context, on: Boolean) = p(ctx).edit().putBoolean("auto_enabled", on).apply()
    fun setAutoDays(ctx: Context, d: Int) = p(ctx).edit().putInt("auto_days", d).apply()
    fun saveRun(ctx: Context, freed: Long, files: Int, auto: Boolean) = p(ctx).edit()
        .putLong("last_ts", System.currentTimeMillis()).putLong("last_freed", freed)
        .putInt("last_files", files).putBoolean("last_auto", auto)
        .putLong("total_freed", p(ctx).getLong("total_freed", 0) + freed).apply()
    fun lastTs(ctx: Context) = p(ctx).getLong("last_ts", 0)
    fun lastFreed(ctx: Context) = p(ctx).getLong("last_freed", 0)
    fun lastAuto(ctx: Context) = p(ctx).getBoolean("last_auto", false)
    fun totalFreed(ctx: Context) = p(ctx).getLong("total_freed", 0)
}
