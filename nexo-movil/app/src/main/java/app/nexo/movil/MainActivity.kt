package app.nexo.movil

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.content.res.ColorStateList
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.widget.CheckBox
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

class MainActivity : Activity() {
    private lateinit var k: Kit
    private lateinit var content: FrameLayout
    private val tabs = ArrayList<TextView>()
    private var tab = 0
    private var scroll: ScrollView? = null
    private val main = Handler(Looper.getMainLooper())

    // Limpieza
    private var cats: List<Category>? = null
    private var scanner: Scanner? = null
    private var scanning = false
    private var cleaning = false
    private var seen = 0
    private var progressText = ""
    private var report: String? = null
    private var selLabel: TextView? = null
    private var safeResult = 0L to 0
    private var pendingDirect: List<Item> = emptyList()
    private var pendingTrash: List<Item> = emptyList()

    // Escudo (se calcula en segundo plano)
    private var shield: Pair<List<Device.Check>, List<Device.AppFlag>>? = null
    private var shieldLoading = false

    // Datos lentos de calcular (caché por app, uso de apps): se cargan en otro hilo,
    // se guardan un minuto y la pantalla nunca se congela esperándolos.
    private val loaded = HashMap<String, Pair<Long, Any>>()
    private val loading = HashSet<String>()

    @Suppress("UNCHECKED_CAST")
    private fun <T : Any> cachedAsync(key: String, maxAgeMs: Long, tabIdx: Int, compute: () -> T): T? {
        val hit = loaded[key]
        val stale = hit == null || System.currentTimeMillis() - hit.first > maxAgeMs
        if (stale && key !in loading) {
            loading += key
            Thread {
                val v = compute()
                main.post {
                    loaded[key] = System.currentTimeMillis() to v
                    loading -= key
                    if (tab == tabIdx && !scanning && !cleaning) show(tab, keepScroll = true)
                }
            }.start()
        }
        return hit?.second as T?
    }

    companion object {
        private const val REQ_TRASH = 41
        private val TAB_NAMES = arrayOf("PANEL", "LIMPIAR", "ENERGÍA", "ESCUDO", "AUTO")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        k = Kit(this)
        window.statusBarColor = C.BG
        window.navigationBarColor = C.BG

        val root = k.vbox().apply { background = GridDrawable(this@MainActivity) }
        root.addView(header())
        content = FrameLayout(this)
        root.addView(content, LinearLayout.LayoutParams(MATCH, 0, 1f))
        root.addView(tabBar())
        setContentView(root)
        // Si el sistema borró la tarea (cierre forzado, "limpiar recientes"), se vuelve a programar.
        if (Prefs.autoEnabled(this) && !AutoCleanJob.isScheduled(this)) AutoCleanJob.schedule(this, Prefs.autoDays(this))
        show(0)
    }

    // En celulares con poca RAM (como el Realme C11), soltar miniaturas al primer aviso.
    @Deprecated("Deprecated in Java")
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= android.content.ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) Thumbs.clear()
    }

    override fun onResume() {
        super.onResume()
        // Al volver de Ajustes (permisos concedidos), se refresca la pestaña.
        if (!scanning && !cleaning) show(tab, keepScroll = true)
    }

    private fun header() = k.hbox().apply {
        setPadding(dpi(18), dpi(14), dpi(18), dpi(8))
        addView(LogoView(this@MainActivity, 38))
        addView(k.vbox().apply {
            addView(k.text("N E X O", 20f, C.TEXT, true).apply { letterSpacing = 0.2f })
            addView(k.text("OPTIMIZACIÓN INTELIGENTE · 100 % LOCAL", 9.5f, C.CYAN, true).apply { letterSpacing = 0.12f })
        })
    }

    private fun tabBar() = k.hbox().apply {
        setBackgroundColor(C.a(C.PANEL, 240))
        setPadding(0, dpi(4), 0, dpi(6))
        TAB_NAMES.forEachIndexed { i, name ->
            val t = k.text(name, 11.5f, C.MUTED, true).apply {
                gravity = Gravity.CENTER
                letterSpacing = 0.1f
                setPadding(0, dpi(12), 0, dpi(12))
                setOnClickListener { show(i) }
            }
            tabs += t
            addView(t, LinearLayout.LayoutParams(0, WRAP, 1f))
        }
    }

    private fun show(i: Int, keepScroll: Boolean = false) {
        val y = if (keepScroll && i == tab) scroll?.scrollY ?: 0 else 0
        tab = i
        tabs.forEachIndexed { j, t ->
            val on = j == i
            t.setTextColor(if (on) C.CYAN else C.MUTED)
            t.background = if (on) HudDrawable(this, C.a(C.CYAN, 26), C.a(C.CYAN, 120), C.ORANGE, cut = 8f) else null
        }
        content.removeAllViews()
        val body = k.vbox().apply { setPadding(dpi(16), dpi(6), dpi(16), dpi(24)) }
        when (i) {
            0 -> panel(body)
            1 -> clean(body)
            2 -> energy(body)
            3 -> shieldTab(body)
            else -> auto(body)
        }
        val sv = ScrollView(this).apply { isVerticalScrollBarEnabled = false; addView(body) }
        scroll = sv
        content.addView(sv)
        if (y > 0) sv.post { sv.scrollTo(0, y) }
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_LONG).show()

    private fun permissionsCard(body: LinearLayout, files: Boolean, usage: Boolean) {
        if ((!files || Device.hasAllFiles()) && (!usage || Device.hasUsage(this))) return
        val c = k.tinted(C.AMBER)
        c.addView(k.text("PERMISOS NECESARIOS", 12f, C.AMBER, true).apply { letterSpacing = 0.12f })
        if (files && !Device.hasAllFiles()) {
            c.addView(k.muted("Acceso a archivos: para encontrar basura, duplicados y capturas. NEXO no envía nada fuera del celular."))
            c.addView(k.button("Permitir acceso a archivos", C.AMBER) {
                startActivity(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName")))
            })
        }
        if (usage && !Device.hasUsage(this)) {
            c.addView(k.space(6))
            c.addView(k.muted("Acceso de uso: para ver qué apps gastan más y cuánta caché tienen."))
            c.addView(k.button("Permitir acceso de uso", C.AMBER) { startActivity(Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS)) })
        }
        body.addView(c)
    }

    // ═══════════════════════════════ PANEL ═══════════════════════════════

    private fun ringCol(frac: Float, color: Int, big: String, small: String) = k.vbox().apply {
        gravity = Gravity.CENTER_HORIZONTAL
        addView(RingView(this@MainActivity, 104).also { it.frac = frac; it.color = color; it.big = big; it.small = small })
    }

    private fun panel(body: LinearLayout) {
        body.addView(k.kicker("Centro de control"))
        body.addView(k.title(Device.name()))
        body.addView(k.muted("Android ${Build.VERSION.RELEASE}" + if (Device.isLowRam(this)) " · edición Go" else ""))
        body.addView(k.space(10))

        val (free, total) = Device.storage()
        val used = if (total > 0) 1f - free.toFloat() / total else 0f
        val mi = Device.memory(this)
        val ram = 1f - mi.availMem.toFloat() / mi.totalMem.coerceAtLeast(1)
        val bat = Device.battery(this)
        val batCol = when { bat.level < 20 -> C.RED; bat.level < 40 -> C.AMBER; else -> C.GREEN }
        val rings = k.card()
        rings.addView(k.hbox().apply {
            addView(ringCol(used, C.level(used), "${(used * 100).toInt()}%", "ALMAC."), LinearLayout.LayoutParams(0, WRAP, 1f))
            addView(ringCol(ram, C.level(ram), "${(ram * 100).toInt()}%", "RAM"), LinearLayout.LayoutParams(0, WRAP, 1f))
            addView(ringCol(bat.level / 100f, batCol, "${bat.level}%", "BATERÍA"), LinearLayout.LayoutParams(0, WRAP, 1f))
        })
        rings.addView(k.muted("${Fmt.bytes(free)} libres de ${Fmt.bytes(total)} · ${Fmt.bytes(mi.availMem)} de RAM libre").apply { gravity = Gravity.CENTER; setPadding(0, dpi(8), 0, 0) })
        body.addView(rings)

        permissionsCard(body, files = true, usage = true)

        val act = k.card()
        act.addView(k.text("LIMPIEZA INTELIGENTE", 12f, C.CYAN, true).apply { letterSpacing = 0.12f })
        act.addView(k.muted("Busca temporales, miniaturas, duplicados, APK sobrantes y más. Primero solo mira: tú decides qué borrar."))
        act.addView(k.button("Escanear basura") { show(1); startScan() })
        body.addView(act)

        val g = k.tinted(C.MAGENTA)
        g.addView(k.text("GARANTÍAS DE NEXO", 12f, C.MAGENTA, true).apply { letterSpacing = 0.12f })
        listOf(
            "Videos y documentos: jamás se borran solos.",
            "Lo tuyo (capturas, duplicados) va a la Papelera: recuperable.",
            "Siempre ves qué se borra antes de confirmar.",
            "Sin anuncios, sin internet, sin procesos en segundo plano.",
        ).forEach { g.addView(k.text("✓  $it", 13f)) }
        body.addView(g)

        val last = Prefs.lastTs(this)
        val h = k.card()
        h.addView(k.text("ÚLTIMA LIMPIEZA", 11f, C.MUTED, true).apply { letterSpacing = 0.12f })
        if (last > 0) {
            h.addView(k.text("${Fmt.bytes(Prefs.lastFreed(this))} liberados", 20f, C.GREEN, true))
            h.addView(k.muted("${if (Prefs.lastAuto(this)) "Automática" else "Manual"} · ${Fmt.ago(last)} · total con NEXO: ${Fmt.bytes(Prefs.totalFreed(this))}"))
        } else {
            h.addView(k.muted("Aún no hay limpiezas."))
        }
        body.addView(h)
    }

    // ═══════════════════════════════ LIMPIAR ═══════════════════════════════

    private fun startScan() {
        if (scanning || !Device.hasAllFiles()) return
        scanning = true; seen = 0; report = null; cats = null
        workStarted = System.currentTimeMillis()
        workFrac = 0f
        val sc = Scanner(this) { dir, n, f -> main.post { seen = n; progressText = dir; setProgress(f, "$n archivos analizados · $dir") } }
        scanner = sc
        show(1)
        Thread {
            val r = sc.scan()
            main.post {
                scanning = false
                if (!sc.cancelled) cats = r
                if (tab == 1) show(1)
            }
        }.start()
    }

    private var progressView: TextView? = null
    private var progressBar: BarView? = null
    private var progressPct: TextView? = null
    private var workStarted = 0L
    private var workFrac = 0f

    /** Actualiza la barra visible (si la hay) sin reconstruir la pantalla. */
    private fun setProgress(f: Float, detail: String) {
        workFrac = f
        progressBar?.let { it.frac = f; it.invalidate() }
        progressPct?.text = "${(f * 100).toInt()} %"
        val secs = (System.currentTimeMillis() - workStarted) / 1000
        val eta = if (f > 0.05f && secs > 1) " · quedan ≈ ${((secs * (1 - f) / f).toLong())} s" else ""
        progressView?.text = "$detail\n$secs s transcurridos$eta"
    }

    /** Bloque de progreso: porcentaje grande, barra por bloques y detalle. */
    private fun progressBlock(c: LinearLayout, color: Int, detail: String) {
        progressPct = k.text("${(workFrac * 100).toInt()} %", 30f, C.TEXT, true).apply { gravity = Gravity.CENTER }
        c.addView(progressPct)
        progressBar = BarView(this, workFrac, color).also { c.addView(it) }
        progressView = k.muted(detail).apply { gravity = Gravity.CENTER }
        c.addView(progressView)
    }

    private fun clean(body: LinearLayout) {
        body.addView(k.kicker("Limpieza inteligente"))
        body.addView(k.title("Buscar y eliminar basura"))
        body.addView(k.muted("Nada se borra sin que lo veas primero. Videos y documentos quedan protegidos."))
        body.addView(k.space(10))
        if (!Device.hasAllFiles()) {
            permissionsCard(body, files = true, usage = false)
            return
        }
        when {
            scanning || cleaning -> {
                val c = k.card()
                val sv = ScannerView(this).apply { color = if (cleaning) C.GREEN else C.CYAN; running = true }
                c.addView(sv)
                c.addView(k.text(if (cleaning) "LIMPIANDO…" else "ESCANEANDO…", 22f, if (cleaning) C.GREEN else C.CYAN, true).apply { gravity = Gravity.CENTER })
                progressBlock(c, if (cleaning) C.GREEN else C.CYAN, if (cleaning) "Si Android te pide confirmar la Papelera, acéptalo." else "$seen archivos analizados")
                if (scanning) c.addView(k.button("Cancelar", C.RED) { scanner?.cancelled = true })
                body.addView(c)
            }
            cats == null -> {
                report?.let { r ->
                    val c = k.tinted(C.GREEN)
                    c.addView(k.text("✓ LIMPIEZA COMPLETADA", 14f, C.GREEN, true))
                    c.addView(k.text(r, 13.5f))
                    body.addView(c)
                }
                val c = k.card()
                c.addView(ScannerView(this).apply { color = C.a(C.CYAN, 170) })
                c.addView(k.text("Listo para escanear", 20f, C.TEXT, true).apply { gravity = Gravity.CENTER })
                c.addView(k.muted("El escaneo solo mira: no borra nada.").apply { gravity = Gravity.CENTER })
                c.addView(k.button("Iniciar escaneo") { startScan() })
                body.addView(c)
            }
            else -> results(body, cats!!)
        }
        appCachesSection(body)
    }

    private fun results(body: LinearLayout, all: List<Category>) {
        val safe = all.filter { it.kind == Kind.SAFE }.sumOf { it.size }
        val review = all.filter { it.kind == Kind.REVIEW }.sumOf { it.size }
        val prot = all.first { it.kind == Kind.PROTECTED }
        val sum = k.card()
        sum.addView(k.row(k.text("BASURA ELIMINABLE", 11.5f, C.MUTED, true), k.text(Fmt.bytes(safe), 18f, C.GREEN, true)))
        sum.addView(k.row(k.text("PARA REVISAR", 11.5f, C.MUTED, true), k.text(Fmt.bytes(review), 18f, C.AMBER, true)))
        sum.addView(k.row(k.text("VIDEOS Y DOCUMENTOS", 11.5f, C.MUTED, true), k.text("${Fmt.bytes(prot.size)} · protegidos", 15f, C.MAGENTA, true)))
        body.addView(sum)

        for ((kind, t, s, col) in listOf(
            Quad(Kind.SAFE, "Limpieza segura", "basura del sistema · se borra definitivamente", C.GREEN),
            Quad(Kind.REVIEW, "Para revisar", "van a la Papelera · recuperables", C.AMBER),
            Quad(Kind.PROTECTED, "Protegidos", "videos y documentos · nunca automático", C.MAGENTA),
        )) {
            body.addView(k.section(t, s, col))
            all.filter { it.kind == kind }.forEach { body.addView(categoryCard(it, col)) }
        }

        val bar = k.card(C.a(C.GREEN, 120))
        selLabel = k.text("", 18f, C.GREEN, true)
        bar.addView(k.row(k.text("SELECCIONADO", 11.5f, C.MUTED, true), selLabel))
        bar.addView(k.button("Limpiar selección", C.GREEN) { confirmClean(all) })
        bar.addView(k.link("⟳ Volver a escanear") { startScan() })
        body.addView(bar)
        updateSel()
    }

    private data class Quad(val kind: Kind, val t: String, val s: String, val col: Int)

    private fun updateSel() {
        val all = cats ?: return
        selLabel?.text = Fmt.bytes(all.sumOf { it.selectedSize() })
    }

    private fun check(on: Boolean, onChange: (Boolean) -> Unit) = CheckBox(this).apply {
        isChecked = on
        buttonTintList = ColorStateList.valueOf(C.CYAN)
        setOnCheckedChangeListener { _, v -> onChange(v) }
    }

    private fun categoryCard(c: Category, color: Int): View {
        val card = k.card(if (c.selectedSize() > 0) C.a(color, 130) else C.a(C.CYAN, 45))
        val has = c.count > 0
        val top = k.hbox()
        if (has) {
            val all = c.kind == Kind.SAFE
            top.addView(check(if (all) c.selected else c.items.isNotEmpty() && c.items.all { it.selected }) { v ->
                if (all) c.selected = v else c.items.forEach { it.selected = v }
                updateSel()
                if (!all && c.expanded) show(1, keepScroll = true)
            })
        }
        val info = k.vbox()
        info.addView(k.text(c.name, 15f, C.TEXT, true))
        info.addView(k.muted(c.desc, 12f))
        top.addView(info, LinearLayout.LayoutParams(0, WRAP, 1f))
        top.addView(k.vbox().apply {
            gravity = Gravity.END
            if (has) {
                addView(k.text(Fmt.bytes(c.size), 15f, color, true))
                addView(k.muted("${c.count} elem.", 11f))
            } else {
                addView(k.text("✓ LIMPIO", 12f, C.a(C.GREEN, 200), true))
            }
        })
        card.addView(top)
        if (has) {
            card.addView(k.link(if (c.expanded) "‹ Ocultar archivos" else if (c.kind == Kind.SAFE) "Ver archivos ›" else "Ver y elegir archivos ›") {
                c.expanded = !c.expanded
                show(1, keepScroll = true)
            })
            if (c.expanded) {
                card.addView(k.muted("Toca un archivo para verlo en grande antes de decidir.", 11.5f))
                c.items.take(150).forEach { it ->
                    val row = k.hbox().apply { setPadding(0, dpi(3), 0, dpi(3)) }
                    var box: CheckBox? = null
                    if (c.kind != Kind.SAFE) box = check(it.selected) { v -> it.selected = v; updateSel() }.also { row.addView(it) }
                    row.addView(thumbBox(it.file, 46))
                    val info = k.vbox().apply { setPadding(dpi(10), 0, dpi(6), 0) }
                    val name = k.text(it.file.name, 12.5f, C.TEXT)
                    name.maxLines = 1
                    name.ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
                    info.addView(name)
                    info.addView(k.muted(listOf(Fmt.bytes(it.size), it.note).filter { s -> s.isNotEmpty() }.joinToString(" · "), 11f))
                    row.addView(info, LinearLayout.LayoutParams(0, WRAP, 1f))
                    row.setOnClickListener { _ -> preview(it, c.kind != Kind.SAFE) { box?.isChecked = it.selected } }
                    card.addView(row)
                }
                if (c.count > 150) card.addView(k.muted("… y ${c.count - 150} más"))
            }
        }
        return card
    }

    /** Cuadro con la miniatura del archivo (o su tipo mientras carga o si no tiene). */
    private fun thumbBox(f: java.io.File, sizeDp: Int): View {
        val box = FrameLayout(this).apply {
            background = HudDrawable(this@MainActivity, C.a(C.PANEL, 255), C.a(C.CYAN, 90), C.a(C.CYAN, 160), cut = 5f)
            layoutParams = LinearLayout.LayoutParams(dpi(sizeDp), dpi(sizeDp))
        }
        box.addView(k.text(f.extension.uppercase().take(4), 10f, C.MUTED, true).apply { gravity = Gravity.CENTER }, FrameLayout.LayoutParams(MATCH, MATCH))
        val iv = android.widget.ImageView(this).apply { scaleType = android.widget.ImageView.ScaleType.CENTER_CROP; setPadding(dpi(2), dpi(2), dpi(2), dpi(2)) }
        box.addView(iv, FrameLayout.LayoutParams(MATCH, MATCH))
        Thumbs.load(this, iv, f, dpi(sizeDp))
        return box
    }

    /** Vista previa grande: ver el archivo antes de decidir si se borra o se conserva. */
    private fun preview(item: Item, selectable: Boolean, onToggle: () -> Unit) {
        val f = item.file
        val v = k.vbox().apply { setPadding(dpi(18), dpi(14), dpi(18), dpi(4)) }
        val frame = FrameLayout(this).apply {
            background = HudDrawable(this@MainActivity, C.a(C.BG, 255), C.a(C.CYAN, 90), C.CYAN, cut = 10f)
        }
        frame.addView(k.text("Sin vista previa para .${f.extension}", 13f, C.MUTED).apply { gravity = Gravity.CENTER }, FrameLayout.LayoutParams(MATCH, MATCH))
        val iv = android.widget.ImageView(this).apply { scaleType = android.widget.ImageView.ScaleType.FIT_CENTER; setPadding(dpi(4), dpi(4), dpi(4), dpi(4)) }
        frame.addView(iv, FrameLayout.LayoutParams(MATCH, MATCH))
        v.addView(frame, LinearLayout.LayoutParams(MATCH, dpi(300)))
        Thumbs.load(this, iv, f, 900)
        v.addView(k.text(f.name, 14.5f, C.TEXT, true).apply { setPadding(0, dpi(10), 0, 0) })
        val date = java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT).format(java.util.Date(f.lastModified()))
        v.addView(k.muted("${Fmt.bytes(item.size)} · $date"))
        v.addView(k.muted(f.parent ?: "", 11f))
        if (item.note.isNotEmpty()) v.addView(k.text(item.note, 12f, C.MAGENTA))
        val b = AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setView(v)
            .setNegativeButton("Cerrar", null)
            .setNeutralButton("Abrir") { _, _ -> openFile(f) }
        if (selectable) {
            b.setPositiveButton(if (item.selected) "Conservar" else "Marcar para borrar") { _, _ ->
                item.selected = !item.selected
                onToggle()
                updateSel()
            }
        }
        b.show()
    }

    /** Abre el archivo con su app (galería, reproductor, lector de PDF). */
    private fun openFile(f: java.io.File) {
        Thread {
            val uri = Cleaner.mediaUris(this, listOf(f))[f]
            main.post {
                if (uri == null) { toast("No hay una app para abrir este archivo."); return@post }
                val type = contentResolver.getType(uri) ?: "*/*"
                try {
                    startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, type).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                } catch (e: Exception) {
                    toast("No hay una app para abrir este archivo.")
                }
            }
        }.start()
    }

    private fun confirmClean(all: List<Category>) {
        val chosen = all.filter { it.selectedSize() > 0 || (it.kind == Kind.SAFE && it.selected && it.count > 0) }
        if (chosen.isEmpty()) { toast("No hay nada seleccionado."); return }
        val v = k.vbox().apply { setPadding(dpi(20), dpi(12), dpi(20), 0) }
        for (c in chosen) {
            val dest = if (c.kind == Kind.SAFE) "se borra definitivamente" else "${c.selectedCount()} elementos → Papelera"
            v.addView(k.row(k.text(c.name, 14f, C.TEXT, true), k.text(Fmt.bytes(c.selectedSize()), 14f, C.GREEN, true)))
            v.addView(k.muted(dest, 11.5f))
        }
        val prot = chosen.filter { it.kind == Kind.PROTECTED }.sumOf { it.selectedCount() }
        var ack: CheckBox? = null
        if (prot > 0) {
            v.addView(k.space(8))
            v.addView(k.text("⚠ Incluye $prot videos/documentos. Irán a la Papelera (si Android lo permite).", 13f, C.MAGENTA, true))
            ack = check(false) {}.apply { text = "Sí, quiero botarlos"; setTextColor(C.TEXT) }
            v.addView(ack)
        }
        val dlg = AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Confirmar limpieza")
            .setView(v)
            .setNegativeButton("Cancelar", null)
            .setPositiveButton("Confirmar") { _, _ -> doClean(all) }
            .create()
        dlg.show()
        val ok = dlg.getButton(AlertDialog.BUTTON_POSITIVE)
        if (ack != null) {
            ok.isEnabled = false
            ack.setOnCheckedChangeListener { _, c -> ok.isEnabled = c }
        }
    }

    private fun doClean(all: List<Category>) {
        cleaning = true
        show(1)
        Thread {
            main.post { workStarted = System.currentTimeMillis(); setProgress(0f, "Borrando basura segura…") }
            val safe = Cleaner.cleanSafe(all) { f -> main.post { setProgress(f * 0.9f, "Borrando basura segura…") } }
            main.post { setProgress(0.92f, "Preparando la Papelera…") }
            val chosen = all.filter { it.kind != Kind.SAFE }.flatMap { c -> c.items.filter { it.selected } }
            val uris = if (chosen.isNotEmpty()) Cleaner.mediaUris(this, chosen.map { it.file }) else emptyMap()
            main.post {
                safeResult = safe
                pendingTrash = chosen.filter { uris.containsKey(it.file) }
                pendingDirect = chosen.filter { !uris.containsKey(it.file) }
                if (pendingTrash.isNotEmpty()) {
                    try {
                        val pi = MediaStore.createTrashRequest(contentResolver, pendingTrash.map { uris.getValue(it.file) }, true)
                        startIntentSenderForResult(pi.intentSender, REQ_TRASH, null, 0, 0, 0)
                    } catch (e: Exception) {
                        pendingDirect = pendingDirect + pendingTrash
                        pendingTrash = emptyList()
                        afterTrash(false)
                    }
                } else {
                    afterTrash(false)
                }
            }
        }.start()
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQ_TRASH) afterTrash(resultCode == RESULT_OK)
    }

    private fun afterTrash(trashed: Boolean) {
        val tBytes = if (trashed) pendingTrash.sumOf { it.size } else 0L
        val tCount = if (trashed) pendingTrash.size else 0
        if (pendingDirect.isEmpty()) {
            finishClean(tBytes, tCount, 0L, 0)
            return
        }
        val bytes = pendingDirect.sumOf { it.size }
        AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Sin Papelera para ${pendingDirect.size} archivos")
            .setMessage("Android no permite enviarlos a la Papelera (${Fmt.bytes(bytes)}). ¿Borrarlos definitivamente?")
            .setNegativeButton("Conservarlos") { _, _ -> finishClean(tBytes, tCount, 0L, 0) }
            .setPositiveButton("Borrar") { _, _ ->
                val gone = pendingDirect.filter { it.file.delete() }
                finishClean(tBytes, tCount, gone.sumOf { it.size }, gone.size)
            }
            .setCancelable(false)
            .show()
    }

    private fun finishClean(tBytes: Long, tCount: Int, dBytes: Long, dCount: Int) {
        val (sBytes, sCount) = safeResult
        val total = sBytes + tBytes + dBytes
        Prefs.saveRun(this, total, sCount + tCount + dCount, auto = false)
        report = buildString {
            append("${Fmt.bytes(sBytes)} de basura eliminados ($sCount elementos).")
            if (tCount > 0) append("\n$tCount elementos en la Papelera (${Fmt.bytes(tBytes)}): recuperables unos 30 días.")
            if (dCount > 0) append("\n$dCount archivos borrados definitivamente (${Fmt.bytes(dBytes)}).")
        }
        pendingDirect = emptyList(); pendingTrash = emptyList()
        loaded.remove("caches")
        cleaning = false
        cats = null
        show(1)
    }

    private fun appCachesSection(body: LinearLayout) {
        body.addView(k.section("Caché de apps", "Android 11 solo deja borrarla desde el ajuste de cada app"))
        if (!Device.hasUsage(this)) {
            permissionsCard(body, files = false, usage = true)
            return
        }
        val c = k.card()
        val list = cachedAsync("caches", 60_000, 1) { Device.appCaches(this, 8) }
        if (list == null) c.addView(k.muted("Calculando la caché de cada app…"))
        else if (list.isEmpty()) c.addView(k.muted("Ninguna app tiene más de 5 MB de caché."))
        list.orEmpty().forEach { (label, pkg, bytes) ->
            c.addView(k.row(k.text(label, 14f), k.link("${Fmt.bytes(bytes)}  Borrar ›") { startActivity(Device.appSettings(pkg)) }))
        }
        c.addView(k.muted("Toca la app › Almacenamiento › «Borrar caché». No borra tus datos ni tu sesión.", 11.5f))
        body.addView(c)
    }

    // ═══════════════════════════════ ENERGÍA ═══════════════════════════════

    private fun energy(body: LinearLayout) {
        body.addView(k.kicker("Batería y rendimiento"))
        body.addView(k.title("Energía"))
        body.addView(k.space(8))
        val b = Device.battery(this)
        val c = k.card()
        val col = when { b.level < 20 -> C.RED; b.level < 40 -> C.AMBER; else -> C.GREEN }
        c.addView(k.hbox().apply {
            addView(RingView(this@MainActivity, 120).also { it.frac = b.level / 100f; it.color = col; it.big = "${b.level}%"; it.small = if (b.charging) "CARGANDO" else "BATERÍA" })
            addView(k.vbox().apply {
                setPadding(dpi(14), 0, 0, 0)
                val tc = when { b.tempC >= 45 -> C.RED; b.tempC >= 40 -> C.AMBER; else -> C.TEXT }
                addView(k.text("${b.tempC} °C", 20f, tc, true))
                addView(k.muted("Salud: ${b.healthText} · ${b.voltMv / 1000.0} V"))
                addView(k.muted(if (b.charging) "Cargando por ${b.plug}" else "Descargando"))
                if (b.designMah != null && b.estFullMah != null) {
                    val pct = (b.estFullMah / b.designMah * 100).coerceAtMost(100.0)
                    addView(k.muted("Capacidad estimada: ≈${b.estFullMah.toInt()} de ${b.designMah.toInt()} mAh (${pct.toInt()} %)"))
                }
            }, LinearLayout.LayoutParams(0, WRAP, 1f))
        })
        if (b.tempC >= 40) c.addView(k.text("⚠ Está caliente: evita usarlo mientras carga y quita la funda.", 12.5f, C.AMBER))
        c.addView(k.muted("Consejo: cárgalo entre 20 % y 80 % y evita el calor; así la batería dura más años.", 11.5f))
        body.addView(c)

        val mi = Device.memory(this)
        val used = 1f - mi.availMem.toFloat() / mi.totalMem.coerceAtLeast(1)
        val r = k.card()
        r.addView(k.row(k.text("MEMORIA RAM", 11.5f, C.MUTED, true), k.text("${(used * 100).toInt()} %", 18f, C.level(used), true)))
        r.addView(BarView(this, used, C.level(used)))
        r.addView(k.muted("${Fmt.bytes(mi.availMem)} libres de ${Fmt.bytes(mi.totalMem)}" + if (mi.lowMemory) " · memoria baja" else ""))
        r.addView(k.muted("Android libera la RAM solo. Cerrar apps a la fuerza hace que se vuelvan a abrir y gasta MÁS batería: mejor restringe las que no uses.", 11.5f))
        body.addView(r)

        body.addView(k.section("Apps más usadas (24 h)", "toca «Ajustes» para limitar su actividad en segundo plano"))
        if (!Device.hasUsage(this)) {
            permissionsCard(body, files = false, usage = true)
        } else {
            val u = k.card()
            val list = cachedAsync("usage", 60_000, 2) { Device.topUsage(this, 8) }
            if (list == null) u.addView(k.muted("Calculando…"))
            else if (list.isEmpty()) u.addView(k.muted("Sin datos todavía."))
            list.orEmpty().forEach { (label, pkg, ms) ->
                u.addView(k.row(k.text(label, 14f), k.link("${Fmt.duration(ms)}  Ajustes ›") { startActivity(Device.appSettings(pkg)) }))
            }
            body.addView(u)
        }

        body.addView(k.section("Recomendado para tu ${Device.name()}", "ajustes que sí ahorran batería y aceleran el celular"))
        Device.tips(this).forEach { t ->
            val tc = k.card()
            tc.addView(k.text(t.title, 14.5f, C.TEXT, true))
            tc.addView(k.muted(t.detail, 12f))
            t.intent?.let { i -> tc.addView(k.link("Abrir ajuste ›") { try { startActivity(i) } catch (e: Exception) { toast("Este ajuste no está disponible en tu celular.") } }) }
            body.addView(tc)
        }
    }

    // ═══════════════════════════════ ESCUDO ═══════════════════════════════

    private fun shieldTab(body: LinearLayout) {
        body.addView(k.kicker("Protección"))
        body.addView(k.title("Escudo"))
        body.addView(k.muted("Revisa la seguridad del celular sin internet."))
        body.addView(k.space(8))
        val s = shield
        if (s == null) {
            val c = k.card()
            c.addView(ScannerView(this).apply { color = C.ORANGE; running = true })
            if (!shieldLoading) { workStarted = System.currentTimeMillis(); workFrac = 0f }
            progressBlock(c, C.ORANGE, "Revisando apps y ajustes…")
            body.addView(c)
            if (!shieldLoading) {
                shieldLoading = true
                Thread {
                    val r = Device.securityChecks(this) { f -> main.post { setProgress(f, "Revisando permisos de cada app…") } }
                    main.post { shield = r; shieldLoading = false; if (tab == 3) show(3) }
                }.start()
            }
            return
        }
        val (checks, flags) = s
        val score = Device.securityScore(checks, flags)
        val top = k.card()
        top.addView(k.hbox().apply {
            addView(RingView(this@MainActivity, 120).also { it.frac = score / 100f; it.color = C.score(score); it.big = "$score"; it.small = "DE 100" })
            addView(k.vbox().apply {
                setPadding(dpi(14), 0, 0, 0)
                val (t, col) = when { score >= 85 -> "PROTEGIDO" to C.GREEN; score >= 60 -> "REVISAR" to C.AMBER; else -> "EN RIESGO" to C.RED }
                addView(k.text(t, 24f, col, true))
                addView(k.link("⟳ Volver a revisar") { shield = null; show(3) })
            }, LinearLayout.LayoutParams(0, WRAP, 1f))
        })
        body.addView(top)

        body.addView(k.section("Comprobaciones", ""))
        val c = k.card()
        checks.forEach { ch ->
            val (icon, col) = when (ch.state) { 0 -> "✓" to C.GREEN; 1 -> "⚠" to C.AMBER; 2 -> "⚠" to C.RED; else -> "•" to C.CYAN }
            val row = k.hbox().apply { setPadding(0, dpi(4), 0, dpi(4)) }
            row.addView(k.text(icon, 16f, col, true).apply { setPadding(0, 0, dpi(10), 0) })
            row.addView(k.vbox().apply {
                addView(k.text(ch.title, 14f, C.TEXT, true))
                addView(k.text(ch.detail, 12f, if (ch.state == 0) C.MUTED else col))
            }, LinearLayout.LayoutParams(0, WRAP, 1f))
            ch.intent?.let { i -> if (ch.state != 0) row.addView(k.link("Abrir ›") { try { startActivity(i) } catch (e: Exception) {} }) }
            c.addView(row)
        }
        body.addView(c)

        if (flags.isNotEmpty()) {
            body.addView(k.section("Apps para revisar", "si no reconoces alguna, quítale el permiso o desinstálala", C.AMBER))
            val f = k.card()
            flags.groupBy { it.reason }.forEach { (reason, apps) ->
                f.addView(k.text(reason.uppercase(), 11f, C.AMBER, true).apply { setPadding(0, dpi(6), 0, dpi(2)) })
                apps.forEach { a -> f.addView(k.row(k.text(a.label, 13.5f), k.link("Revisar ›") { startActivity(Device.appSettings(a.pkg)) })) }
            }
            body.addView(f)
        }
    }

    // ═══════════════════════════════ AUTO ═══════════════════════════════

    private fun auto(body: LinearLayout) {
        body.addView(k.kicker("Piloto automático"))
        body.addView(k.title("Limpieza automática"))
        body.addView(k.muted("Android la lanza solo mientras el celular se carga. El resto del tiempo NEXO no ocupa nada."))
        body.addView(k.space(8))
        val on = Prefs.autoEnabled(this)
        val c = k.card()
        c.addView(k.hbox().apply {
            addView(ToggleView(this@MainActivity, on) { v ->
                if (v && !Device.hasAllFiles()) {
                    toast("Primero permite el acceso a archivos.")
                    show(4); return@ToggleView
                }
                if (v) {
                    if (Build.VERSION.SDK_INT >= 33) requestPermissions(arrayOf("android.permission.POST_NOTIFICATIONS"), 7)
                    Prefs.setAutoEnabled(this@MainActivity, true)
                    AutoCleanJob.schedule(this@MainActivity, Prefs.autoDays(this@MainActivity))
                    toast("Limpieza automática activada")
                } else {
                    Prefs.setAutoEnabled(this@MainActivity, false)
                    AutoCleanJob.cancel(this@MainActivity)
                    toast("Limpieza automática desactivada")
                }
                show(4, keepScroll = true)
            })
            addView(k.text("  Limpieza automática", 16f, C.TEXT, true), LinearLayout.LayoutParams(0, WRAP, 1f))
            addView(k.badge(if (on) "programada" else "apagada", if (on) C.GREEN else C.MUTED))
        })
        c.addView(k.space(8))
        c.addView(k.text("FRECUENCIA", 11f, C.MUTED, true))
        val days = Prefs.autoDays(this)
        c.addView(k.hbox().apply {
            listOf(1 to "Diaria", 7 to "Semanal").forEach { (d, label) ->
                addView(k.button(label, if (d == days) C.CYAN else C.MUTED) {
                    Prefs.setAutoDays(this@MainActivity, d)
                    if (Prefs.autoEnabled(this@MainActivity)) AutoCleanJob.schedule(this@MainActivity, d)
                    show(4, keepScroll = true)
                }.apply { layoutParams = LinearLayout.LayoutParams(0, WRAP, 1f).apply { marginEnd = dpi(8); topMargin = dpi(6) } })
            }
        })
        body.addView(c)

        // Realme y Honor detienen las tareas programadas de apps "optimizadas".
        val pm = getSystemService(android.os.PowerManager::class.java)
        if (!pm.isIgnoringBatteryOptimizations(packageName)) {
            val b = k.tinted(C.AMBER)
            b.addView(k.text("QUE EL SISTEMA NO LA BLOQUEE", 12f, C.AMBER, true))
            b.addView(k.muted("${Device.name()} puede frenar la limpieza programada para ahorrar batería. Busca NEXO y elige «No optimizar» (o «Permitir actividad en segundo plano»). NEXO no corre en segundo plano: solo se despierta mientras cargas."))
            b.addView(k.button("Abrir optimización de batería", C.AMBER) {
                try { startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) } catch (e: Exception) { toast("Búscalo en Ajustes › Batería.") }
            })
            body.addView(b)
        }

        val g = k.tinted(C.MAGENTA)
        g.addView(k.text("TUS ARCHIVOS ESTÁN A SALVO", 12f, C.MAGENTA, true))
        g.addView(k.text("En automático solo se borra basura segura (miniaturas, temporales, estados vistos, carpetas vacías).", 13f))
        g.addView(k.muted("Capturas, duplicados, videos y documentos nunca se tocan: solo te avisa para que decidas."))
        body.addView(g)

        val last = Prefs.lastTs(this)
        if (last > 0) {
            val h = k.card()
            h.addView(k.text("ÚLTIMA LIMPIEZA", 11f, C.MUTED, true))
            h.addView(k.text("${Fmt.bytes(Prefs.lastFreed(this))} · ${Fmt.ago(last)}", 16f, C.GREEN, true))
            body.addView(h)
        }

        body.addView(k.section("Conectar con la laptop", "modo potenciado: NEXO de Windows controla este celular por WiFi"))
        val l = k.card()
        listOf(
            "1. Ajustes › Acerca del teléfono › toca 7 veces «Número de compilación» (en realme: «Versión»).",
            "2. Opciones de desarrollador › Depuración inalámbrica › «Vincular dispositivo con código».",
            "3. En la laptop: NEXO › Celulares › escribe la IP, el puerto y el código que aparecen aquí.",
            "Celular y laptop deben estar en la misma red WiFi. No usa internet.",
        ).forEach { l.addView(k.text(it, 13f).apply { setPadding(0, dpi(2), 0, dpi(2)) }) }
        l.addView(k.button("Abrir opciones de desarrollador", C.ORANGE) {
            try { startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS)) } catch (e: Exception) { toast("Primero activa las opciones de desarrollador (paso 1).") }
        })
        body.addView(l)
        body.addView(k.muted("NEXO móvil 1.0 · sin anuncios · sin internet").apply { gravity = Gravity.CENTER; setPadding(0, dpi(12), 0, 0) })
    }

    private fun dpi(v: Int) = (v * resources.displayMetrics.density).toInt()
    private fun dp(v: Float) = v * resources.displayMetrics.density
}
