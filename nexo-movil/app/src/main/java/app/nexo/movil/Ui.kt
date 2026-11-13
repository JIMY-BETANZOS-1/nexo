package app.nexo.movil

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.cos
import kotlin.math.sin

/** Paleta "cabina mecha" (la misma que NEXO para Windows). */
object C {
    const val BG = 0xFF03070D.toInt()
    const val PANEL = 0xFF070F1A.toInt()
    const val CYAN = 0xFF38C8FF.toInt()
    const val ORANGE = 0xFFFF8A1E.toInt()
    const val MAGENTA = 0xFFFF48C4.toInt()
    const val GREEN = 0xFF3CFFB4.toInt()
    const val AMBER = 0xFFFFC430.toInt()
    const val RED = 0xFFFF3048.toInt()
    const val TEXT = 0xFFD4ECFF.toInt()
    const val MUTED = 0xFF6C88A6.toInt()
    fun a(c: Int, alpha: Int) = (c and 0x00FFFFFF) or (alpha.coerceIn(0, 255) shl 24)
    fun level(frac: Float) = when {
        frac >= 0.9f -> RED
        frac >= 0.75f -> AMBER
        else -> CYAN
    }
    fun score(s: Int) = when {
        s >= 85 -> GREEN
        s >= 70 -> CYAN
        s >= 55 -> AMBER
        else -> RED
    }
}

const val MATCH = ViewGroup.LayoutParams.MATCH_PARENT
const val WRAP = ViewGroup.LayoutParams.WRAP_CONTENT

val TITLE_FONT: Typeface = Typeface.create("sans-serif-condensed", Typeface.BOLD)
val BODY_FONT: Typeface = Typeface.create("sans-serif", Typeface.NORMAL)

fun Context.dp(v: Float) = v * resources.displayMetrics.density
fun Context.dpi(v: Int) = (v * resources.displayMetrics.density).toInt()

/**
 * Fondo de panel de cabina: esquinas superior-izquierda e inferior-derecha cortadas,
 * marcos en las otras dos y, si se pide, franja de advertencia arriba.
 */
class HudDrawable(
    private val ctx: Context,
    private val fill: Int,
    private val stroke: Int,
    private val accent: Int,
    private val cut: Float = 12f,
    private val hazard: Boolean = false,
) : Drawable() {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)
    private val path = android.graphics.Path()

    override fun onBoundsChange(b: Rect) {
        val k = ctx.dp(cut).coerceAtMost(minOf(b.width(), b.height()) / 3f)
        path.reset()
        path.moveTo(b.left + k, b.top.toFloat())
        path.lineTo(b.right.toFloat(), b.top.toFloat())
        path.lineTo(b.right.toFloat(), b.bottom - k)
        path.lineTo(b.right - k, b.bottom.toFloat())
        path.lineTo(b.left.toFloat(), b.bottom.toFloat())
        path.lineTo(b.left.toFloat(), b.top + k)
        path.close()
    }

    override fun draw(c: Canvas) {
        val b = bounds
        p.style = Paint.Style.FILL; p.color = fill
        c.drawPath(path, p)
        p.style = Paint.Style.STROKE; p.strokeWidth = ctx.dp(1f); p.color = stroke
        c.drawPath(path, p)
        p.strokeWidth = ctx.dp(2f); p.color = accent
        val l = ctx.dp(14f)
        c.drawLine(b.right - l, b.top + 1f, b.right - 1f, b.top + 1f, p)
        c.drawLine(b.right - 1f, b.top + 1f, b.right - 1f, b.top + l, p)
        c.drawLine(b.left + 1f, b.bottom - l, b.left + 1f, b.bottom - 1f, p)
        c.drawLine(b.left + 1f, b.bottom - 1f, b.left + l, b.bottom - 1f, p)
        if (hazard) {
            p.style = Paint.Style.FILL; p.color = C.a(accent, 170)
            val h = ctx.dp(4f)
            val top = b.top + ctx.dp(5f)
            var x = b.right - ctx.dp(120f)
            while (x < b.right - ctx.dp(22f)) {
                val q = android.graphics.Path()
                q.moveTo(x, top + h); q.lineTo(x + h, top + h); q.lineTo(x + 2 * h, top); q.lineTo(x + h, top); q.close()
                c.drawPath(q, p)
                x += h * 2.2f
            }
        }
    }

    override fun setAlpha(alpha: Int) {}
    override fun setColorFilter(cf: ColorFilter?) {}
    @Deprecated("Deprecated in Java")
    override fun getOpacity() = PixelFormat.TRANSLUCENT
}

/** Franja diagonal de advertencia. */
class HazardView(ctx: Context, private val color: Int, wDp: Int = 30, hDp: Int = 8) : View(ctx) {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color }

    init {
        layoutParams = LinearLayout.LayoutParams(ctx.dpi(wDp), ctx.dpi(hDp)).apply { marginEnd = ctx.dpi(8) }
    }

    override fun onDraw(c: Canvas) {
        val h = height.toFloat()
        var x = -h
        while (x < width) {
            val q = android.graphics.Path()
            q.moveTo(x, h); q.lineTo(x + h * 0.6f, h); q.lineTo(x + h * 1.6f, 0f); q.lineTo(x + h, 0f); q.close()
            c.drawPath(q, p)
            x += h * 1.3f
        }
    }
}

/** Constructor de vistas: todo se dibuja en código, sin librerías (APK pequeño). */
class Kit(val ctx: Context) {
    fun vbox() = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL }
    fun hbox() = LinearLayout(ctx).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }

    fun card(stroke: Int = C.a(C.CYAN, 60), fill: Int = C.a(C.PANEL, 232), accent: Int = C.a(C.CYAN, 200), hazard: Boolean = false) = vbox().apply {
        background = HudDrawable(ctx, fill, stroke, accent, hazard = hazard)
        setPadding(ctx.dpi(16), ctx.dpi(14), ctx.dpi(16), ctx.dpi(14))
        layoutParams = LinearLayout.LayoutParams(MATCH, WRAP).apply { setMargins(0, 0, 0, ctx.dpi(10)) }
    }

    fun tinted(color: Int) = card(C.a(color, 140), blend(C.a(C.PANEL, 235), C.a(color, 26)), color, hazard = true)

    private fun blend(base: Int, over: Int): Int {
        val a = (over ushr 24) / 255f
        fun ch(s: Int) = (((base shr s) and 0xFF) * (1 - a) + ((over shr s) and 0xFF) * a).toInt()
        return (base and 0xFF000000.toInt()) or (ch(16) shl 16) or (ch(8) shl 8) or ch(0)
    }

    fun text(s: CharSequence, size: Float = 14f, color: Int = C.TEXT, bold: Boolean = false) = TextView(ctx).apply {
        text = s; textSize = size; setTextColor(color)
        typeface = if (bold) TITLE_FONT else BODY_FONT
        setLineSpacing(0f, 1.1f)
    }

    fun kicker(s: String) = hbox().apply {
        addView(HazardView(ctx, C.ORANGE))
        addView(text(s.uppercase(), 11f, C.ORANGE, true).apply { letterSpacing = 0.16f })
    }
    fun title(s: String, size: Float = 24f) = text(s.uppercase(), size, C.TEXT, true).apply { letterSpacing = 0.04f }
    fun muted(s: String, size: Float = 12.5f) = text(s, size, C.MUTED)

    fun button(label: String, color: Int = C.CYAN, enabled: Boolean = true, onClick: () -> Unit) = TextView(ctx).apply {
        val c = if (enabled) color else C.MUTED
        text = label.uppercase(); textSize = 14f; setTextColor(c); typeface = TITLE_FONT
        letterSpacing = 0.1f; gravity = Gravity.CENTER
        setPadding(ctx.dpi(14), ctx.dpi(13), ctx.dpi(14), ctx.dpi(13))
        val shape = HudDrawable(ctx, C.a(c, 26), C.a(c, if (enabled) 220 else 80), c, cut = 9f)
        background = RippleDrawable(ColorStateList.valueOf(C.a(c, 90)), shape, null)
        isEnabled = enabled
        setOnClickListener { onClick() }
        layoutParams = LinearLayout.LayoutParams(MATCH, WRAP).apply { topMargin = ctx.dpi(8) }
    }

    fun link(label: String, color: Int = C.CYAN, onClick: () -> Unit) = text(label, 13.5f, color, true).apply {
        setPadding(0, ctx.dpi(6), 0, ctx.dpi(6)); setOnClickListener { onClick() }
    }

    fun badge(s: String, color: Int) = text(s.uppercase(), 9.5f, color, true).apply {
        letterSpacing = 0.08f
        setPadding(ctx.dpi(9), ctx.dpi(2), ctx.dpi(6), ctx.dpi(2))
        // Etiqueta de cabina: rectángulo con pestaña de color a la izquierda.
        val tab = GradientDrawable().apply { setColor(color) }
        val body = GradientDrawable().apply { setColor(C.a(color, 28)); setStroke(ctx.dpi(1), C.a(color, 120)) }
        background = android.graphics.drawable.LayerDrawable(arrayOf(body, tab)).apply {
            setLayerWidth(1, ctx.dpi(3)); setLayerGravity(1, Gravity.START or Gravity.FILL_VERTICAL)
        }
        layoutParams = LinearLayout.LayoutParams(WRAP, WRAP).apply { marginEnd = ctx.dpi(6) }
    }

    fun section(title: String, sub: String, color: Int = C.CYAN) = vbox().apply {
        setPadding(ctx.dpi(2), ctx.dpi(10), 0, ctx.dpi(8))
        addView(hbox().apply {
            addView(HazardView(ctx, color, 18, 8))
            addView(text(title.uppercase(), 13f, color, true).apply { letterSpacing = 0.1f })
        })
        if (sub.isNotEmpty()) addView(muted(sub, 11.5f))
    }

    /** Fila "izquierda (crece) | derecha". */
    fun row(left: View, right: View?) = hbox().apply {
        addView(left, LinearLayout.LayoutParams(0, WRAP, 1f))
        if (right != null) addView(right)
    }

    fun space(h: Int) = View(ctx).apply { layoutParams = LinearLayout.LayoutParams(MATCH, ctx.dpi(h)) }
}

/** Anillo de medición con brillo y marcas de escala. */
class RingView(ctx: Context, sizeDp: Int = 140) : View(ctx) {
    var frac = 0f
    var color = C.CYAN
    var big = ""
    var small = ""
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)

    init {
        layoutParams = LinearLayout.LayoutParams(ctx.dpi(sizeDp), ctx.dpi(sizeDp))
    }

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val cx = w / 2f
        val cy = height / 2f
        val st = w * 0.06f
        val r = w / 2f - st * 2.6f
        val oval = RectF(cx - r, cy - r, cx + r, cy + r)
        p.style = Paint.Style.STROKE
        p.strokeCap = Paint.Cap.BUTT
        p.strokeWidth = st
        // 40 segmentos, como un medidor de cabina.
        val segs = 40
        val on = (frac.coerceIn(0f, 1f) * segs).toInt()
        val step = 360f / segs
        for (i in 0 until segs) {
            p.color = if (i < on) color else C.a(color, 28)
            c.drawArc(oval, -90f + i * step + step * 0.12f, step * 0.76f, false, p)
        }
        p.strokeWidth = context.dp(1f)
        p.color = C.a(color, 70)
        c.drawCircle(cx, cy, r - st * 1.3f, p)
        for (i in 0 until 12) {
            val ang = Math.toRadians(i * 30.0)
            val r1 = r + st * 0.9f
            val r2 = r1 + st * 0.7f
            c.drawLine(cx + cos(ang).toFloat() * r1, cy + sin(ang).toFloat() * r1, cx + cos(ang).toFloat() * r2, cy + sin(ang).toFloat() * r2, p)
        }
        // Arcos naranjas exteriores.
        val ro = r + st * 1.9f
        p.color = C.a(C.ORANGE, 200); p.strokeWidth = context.dp(2f)
        c.drawArc(RectF(cx - ro, cy - ro, cx + ro, cy + ro), 20f, 50f, false, p)
        c.drawArc(RectF(cx - ro, cy - ro, cx + ro, cy + ro), 200f, 50f, false, p)
        p.style = Paint.Style.FILL
        p.textAlign = Paint.Align.CENTER
        p.typeface = TITLE_FONT
        p.color = C.TEXT
        p.textSize = w * 0.2f
        c.drawText(big, cx, cy + w * 0.05f, p)
        p.textSize = w * 0.072f
        p.color = color
        p.letterSpacing = 0.12f
        c.drawText(small, cx, cy + w * 0.19f, p)
        p.letterSpacing = 0f
    }
}

/** Radar animado (solo se anima mientras `running`). */
class ScannerView(ctx: Context, sizeDp: Int = 210) : View(ctx) {
    var color = C.CYAN
    var running = false
        set(v) { field = v; invalidate() }
    private var t = 0.8f
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)

    init {
        layoutParams = LinearLayout.LayoutParams(MATCH, ctx.dpi(sizeDp))
    }

    override fun onDraw(c: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        val r = height * 0.36f
        p.style = Paint.Style.STROKE
        p.strokeWidth = context.dp(1f)
        p.color = C.a(color, 50)
        for (k in floatArrayOf(1f, 0.66f, 0.33f)) c.drawCircle(cx, cy, r * k, p)
        c.drawLine(cx - r, cy, cx + r, cy, p.apply { color = C.a(this@ScannerView.color, 25) })
        c.drawLine(cx, cy - r, cx, cy + r, p)
        p.strokeWidth = context.dp(2f)
        for (k in 0 until 26) {
            val a = t * 2.2 - k * 0.045
            p.color = C.a(color, (140 * (1 - k / 26f)).toInt())
            c.drawLine(cx, cy, cx + cos(a).toFloat() * r, cy + sin(a).toFloat() * r, p)
        }
        p.strokeCap = Paint.Cap.ROUND
        val arcs = arrayOf(floatArrayOf(r + context.dp(14f), 1f, 75f), floatArrayOf(r + context.dp(24f), -0.7f, 50f), floatArrayOf(r * 0.66f, 1.6f, 40f))
        for ((i, a) in arcs.withIndex()) {
            val start = Math.toDegrees((t * a[1] + i).toDouble()).toFloat()
            val o = RectF(cx - a[0], cy - a[0], cx + a[0], cy + a[0])
            p.color = color; p.strokeWidth = context.dp(3f)
            c.drawArc(o, start, a[2], false, p)
            p.color = C.a(color, 130); p.strokeWidth = context.dp(2f)
            c.drawArc(o, start + 180f, a[2] / 2, false, p)
        }
        p.strokeCap = Paint.Cap.BUTT
        p.style = Paint.Style.FILL
        for (i in 0 until 7) {
            val ph = ((t * 0.6f + i * 0.37f) % 1f)
            val ang = i * 2.1 + t * 0.11
            val rad = r * (0.2f + 0.75f * ((i * 37 % 10) / 10f))
            p.color = C.a(color, (255 * (1 - ph)).toInt())
            c.drawCircle(cx + cos(ang).toFloat() * rad, cy + sin(ang).toFloat() * rad, context.dp(3f), p)
        }
        if (running) {
            t += 0.016f
            postInvalidateOnAnimation()
        }
    }
}

class BarView(ctx: Context, var frac: Float, var color: Int) : View(ctx) {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)

    init {
        layoutParams = LinearLayout.LayoutParams(MATCH, ctx.dpi(6)).apply { topMargin = ctx.dpi(6); bottomMargin = ctx.dpi(4) }
    }

    override fun onDraw(c: Canvas) {
        val h = height.toFloat()
        val seg = context.dp(6f)
        val gap = context.dp(2f)
        val n = ((width + gap) / (seg + gap)).toInt().coerceAtLeast(1)
        val on = (frac.coerceIn(0f, 1f) * n).toInt()
        for (i in 0 until n) {
            p.color = if (i < on) color else C.a(color, 28)
            val x = i * (seg + gap)
            c.drawRect(x, 0f, x + seg, h, p)
        }
    }
}

/** Interruptor de neón. */
class ToggleView(ctx: Context, var on: Boolean, val onChange: (Boolean) -> Unit) : View(ctx) {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)

    init {
        layoutParams = LinearLayout.LayoutParams(ctx.dpi(50), ctx.dpi(28))
        setOnClickListener { on = !on; invalidate(); onChange(on) }
    }

    override fun onDraw(c: Canvas) {
        val col = if (on) C.CYAN else C.MUTED
        val h = height.toFloat()
        p.style = Paint.Style.FILL
        p.color = C.a(col, if (on) 60 else 28)
        c.drawRect(0f, 0f, width.toFloat(), h, p)
        p.style = Paint.Style.STROKE; p.strokeWidth = context.dp(1f); p.color = C.a(col, 200)
        c.drawRect(1f, 1f, width - 1f, h - 1f, p)
        p.style = Paint.Style.FILL; p.color = col
        val kw = h * 0.75f
        val x = if (on) width - kw - h * 0.15f else h * 0.15f
        c.drawRect(x, h * 0.2f, x + kw, h * 0.8f, p)
    }
}

/** Fondo: brillos suaves y cuadrícula. */
class GridDrawable(private val ctx: Context) : Drawable() {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)
    private var glowA: Shader? = null
    private var glowB: Shader? = null

    // Los degradados se crean una vez por tamaño, no en cada dibujado.
    override fun onBoundsChange(b: Rect) {
        glowA = RadialGradient(b.right - ctx.dp(40f), b.top + ctx.dp(80f), (b.width() * 0.8f).coerceAtLeast(1f), C.a(C.CYAN, 34), 0, Shader.TileMode.CLAMP)
        glowB = RadialGradient(b.left.toFloat(), b.bottom.toFloat(), (b.width() * 0.9f).coerceAtLeast(1f), C.a(C.ORANGE, 22), 0, Shader.TileMode.CLAMP)
    }

    override fun draw(c: Canvas) {
        val b = bounds
        c.drawColor(C.BG)
        p.shader = glowA
        c.drawRect(b, p)
        p.shader = glowB
        c.drawRect(b, p)
        p.shader = null
        p.color = C.a(C.CYAN, 12)
        p.strokeWidth = 1f
        val step = ctx.dp(36f)
        var x = 0f
        while (x < b.right) { c.drawLine(x, 0f, x, b.bottom.toFloat(), p); x += step }
        var y = 0f
        while (y < b.bottom) { c.drawLine(0f, y, b.right.toFloat(), y, p); y += step }
        // Líneas de escaneo.
        p.color = 0x22000000
        val sl = ctx.dp(3f)
        y = 0f
        while (y < b.bottom) { c.drawLine(0f, y, b.right.toFloat(), y, p); y += sl }
    }

    override fun setAlpha(alpha: Int) {}
    override fun setColorFilter(cf: ColorFilter?) {}
    @Deprecated("Deprecated in Java")
    override fun getOpacity() = PixelFormat.OPAQUE
}

/** Logo: hexágono de neón. */
class LogoView(ctx: Context, sizeDp: Int = 40) : View(ctx) {
    private val p = Paint(Paint.ANTI_ALIAS_FLAG)

    init {
        layoutParams = LinearLayout.LayoutParams(ctx.dpi(sizeDp), ctx.dpi(sizeDp)).apply { marginEnd = ctx.dpi(12) }
    }

    override fun onDraw(c: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        val r = width * 0.42f
        val path = android.graphics.Path()
        for (i in 0 until 6) {
            val a = Math.toRadians(i * 60.0 - 90)
            val x = cx + cos(a).toFloat() * r
            val y = cy + sin(a).toFloat() * r
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        path.close()
        p.style = Paint.Style.STROKE
        p.color = C.a(C.CYAN, 50); p.strokeWidth = width * 0.12f
        c.drawPath(path, p)
        p.color = C.CYAN; p.strokeWidth = width * 0.045f
        c.drawPath(path, p)
        p.color = C.ORANGE
        c.drawCircle(cx, cy, r * 0.48f, p)
        p.style = Paint.Style.FILL; p.color = C.CYAN
        c.drawCircle(cx, cy, r * 0.2f, p)
    }
}

object Fmt {
    fun bytes(b: Long): String {
        val u = arrayOf("B", "KB", "MB", "GB", "TB")
        var v = b.toDouble()
        var i = 0
        while (v >= 1024 && i < 4) { v /= 1024; i++ }
        val s = when {
            i == 0 -> "$b B"
            v >= 100 -> String.format("%.0f %s", v, u[i])
            v >= 10 -> String.format("%.1f %s", v, u[i])
            else -> String.format("%.2f %s", v, u[i])
        }
        return s.replace('.', ',')
    }

    fun ago(ts: Long): String {
        val s = (System.currentTimeMillis() - ts) / 1000
        return when {
            s < 60 -> "hace un momento"
            s < 3600 -> "hace ${s / 60} min"
            s < 86400 -> "hace ${s / 3600} h"
            s / 86400 == 1L -> "hace 1 día"
            else -> "hace ${s / 86400} días"
        }
    }

    fun duration(ms: Long): String {
        val m = ms / 60000
        return if (m >= 60) "${m / 60} h ${m % 60} min" else "$m min"
    }
}
