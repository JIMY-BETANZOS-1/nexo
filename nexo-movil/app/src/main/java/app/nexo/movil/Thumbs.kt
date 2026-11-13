package app.nexo.movil

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.pdf.PdfRenderer
import android.media.ThumbnailUtils
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.util.LruCache
import android.util.Size
import android.widget.ImageView
import java.io.File
import java.util.concurrent.Executors

/**
 * Miniaturas de fotos, videos, PDF e instaladores APK.
 * Se crean en pocos hilos, nunca en el de la pantalla, y se guardan en una
 * caché con tope de memoria (1/16 de la que Android da a la app).
 */
object Thumbs {
    private val cache = object : LruCache<String, Bitmap>((Runtime.getRuntime().maxMemory() / 1024 / 16).toInt()) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount / 1024
    }
    private val pool = Executors.newFixedThreadPool(Runtime.getRuntime().availableProcessors().coerceIn(2, 3))
    private val main = Handler(Looper.getMainLooper())

    /** Libera las miniaturas guardadas (cuando Android pide memoria). */
    fun clear() = cache.evictAll()

    /** Carga la miniatura en `iv`; si la fila se recicla para otro archivo, se ignora. */
    fun load(ctx: Context, iv: ImageView, f: File, px: Int) {
        val key = "${f.path}#$px"
        iv.tag = key
        cache.get(key)?.let { iv.setImageBitmap(it); return }
        iv.setImageDrawable(null)
        pool.execute {
            val b = make(ctx.applicationContext, f, px)
            if (b != null) cache.put(key, b)
            main.post { if (iv.tag == key && b != null) iv.setImageBitmap(b) }
        }
    }

    fun make(ctx: Context, f: File, px: Int): Bitmap? = try {
        val ext = f.extension.lowercase()
        val s = Size(px, px)
        when {
            ext in Rules.IMAGES -> ThumbnailUtils.createImageThumbnail(f, s, null)
            ext in Rules.VIDEO -> ThumbnailUtils.createVideoThumbnail(f, s, null)
            ext == "pdf" -> pdfPage(f, px)
            ext == "apk" -> apkIcon(ctx, f, px)
            else -> null
        }
    } catch (e: Throwable) {
        null
    }

    private fun pdfPage(f: File, px: Int): Bitmap? =
        ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY).use { pfd ->
            PdfRenderer(pfd).use { r ->
                if (r.pageCount == 0) return null
                r.openPage(0).use { page ->
                    val k = px.toFloat() / maxOf(page.width, page.height)
                    val bm = Bitmap.createBitmap((page.width * k).toInt().coerceAtLeast(1), (page.height * k).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
                    bm.eraseColor(Color.WHITE)
                    page.render(bm, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                    bm
                }
            }
        }

    private fun apkIcon(ctx: Context, f: File, px: Int): Bitmap? {
        val pm = ctx.packageManager
        val ai = pm.getPackageArchiveInfo(f.path, 0)?.applicationInfo ?: return null
        ai.sourceDir = f.path
        ai.publicSourceDir = f.path
        val d = ai.loadIcon(pm)
        val bm = Bitmap.createBitmap(px, px, Bitmap.Config.ARGB_8888)
        val c = Canvas(bm)
        d.setBounds(0, 0, px, px)
        d.draw(c)
        return bm
    }
}
