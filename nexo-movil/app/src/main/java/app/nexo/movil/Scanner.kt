package app.nexo.movil

import android.content.ContentUris
import android.content.Context
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import java.io.File
import java.io.RandomAccessFile
import java.security.MessageDigest

/**
 * Reglas de limpieza del celular (las mismas que en Windows):
 *  - SEGURO: basura real. Se borra definitivamente. Única clase del modo automático.
 *  - REVISAR: archivos tuyos que suelen sobrar. Se muestran y van a la Papelera.
 *  - PROTEGIDO: videos y documentos. Nunca automático; solo los que marques, a la Papelera.
 */
enum class Kind { SAFE, REVIEW, PROTECTED }

class Item(val file: File, val size: Long, var selected: Boolean, val note: String = "")

class Category(val id: String, val name: String, val desc: String, val kind: Kind) {
    val items = ArrayList<Item>()
    var size = 0L
    var selected = kind == Kind.SAFE
    var expanded = false
    val count get() = items.size

    fun add(f: File, len: Long, note: String = "") {
        if (items.size >= 20000) return
        size += len
        items.add(Item(f, len, kind == Kind.SAFE, note))
    }

    fun selectedSize(): Long = if (kind == Kind.SAFE) (if (selected) size else 0L) else items.filter { it.selected }.sumOf { it.size }
    fun selectedCount(): Int = if (kind == Kind.SAFE) (if (selected) count else 0) else items.count { it.selected }
}

object Rules {
    val VIDEO = setOf("mp4", "m4v", "mkv", "avi", "mov", "wmv", "flv", "webm", "3gp", "3g2", "mpg", "mpeg", "ts")
    val DOCS = setOf("pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "csv", "md", "epub")
    val IMAGES = setOf("jpg", "jpeg", "png", "webp", "gif", "heic", "bmp")
    fun ext(f: File) = f.extension.lowercase()
    fun isVideo(f: File) = ext(f) in VIDEO
    fun isProtected(f: File) = ext(f) in VIDEO || ext(f) in DOCS

    /** Carpetas de primer nivel que Android crea y que nunca se tocan aunque estén vacías. */
    val STANDARD_DIRS = setOf("alarms", "audiobooks", "dcim", "documents", "download", "movies", "music", "notifications", "pictures", "podcasts", "recordings", "ringtones", "android")
}

private const val DAY = 86_400_000L

class Scanner(private val ctx: Context, private val onProgress: (String, Int, Float) -> Unit) {
    @Volatile var cancelled = false
    private val root: File = Environment.getExternalStorageDirectory()
    private var seen = 0

    fun scan(): List<Category> {
        val thumbs = Category("thumbs", "Miniaturas", "Vistas previas que la galería vuelve a crear sola.", Kind.SAFE)
        val temp = Category("temp", "Temporales y registros", "Archivos .tmp, .temp y .log que dejan las apps.", Kind.SAFE)
        val statuses = Category("statuses", "Estados de WhatsApp vistos", "Fotos de estados que WhatsApp descarga solo.", Kind.SAFE)
        val empty = Category("empty", "Carpetas vacías", "Carpetas sin nada dentro, casi siempre de apps desinstaladas.", Kind.SAFE)
        val apks = Category("apks", "Instaladores APK", "Archivos .apk descargados. Si la app ya está instalada, sobran.", Kind.REVIEW)
        val shots = Category("shots", "Capturas de pantalla", "Imágenes de la carpeta de capturas.", Kind.REVIEW)
        val dups = Category("dups", "Fotos duplicadas", "Copias exactas. Siempre se conserva la original.", Kind.REVIEW)
        val sent = Category("sent", "Fotos enviadas por WhatsApp", "Copias de lo que enviaste, de más de 30 días.", Kind.REVIEW)
        val voice = Category("voice", "Notas de voz antiguas", "Audios de WhatsApp de más de 90 días.", Kind.REVIEW)
        val big = Category("big", "Archivos grandes", "Más de 100 MB (sin contar videos ni documentos).", Kind.REVIEW)
        val lost = Category("lost", "LOST.DIR", "Fragmentos recuperados tras errores de la tarjeta.", Kind.REVIEW)
        val prot = Category("protected", "Videos y documentos", "Protegidos: nunca se borran solos. Solo los que tú marques.", Kind.PROTECTED)

        val now = System.currentTimeMillis()
        val bySize = HashMap<Long, ArrayList<File>>()
        val installed = ctx.packageManager.getInstalledPackages(0).associate { it.packageName to it.longVersionCode }
        // Estimación del total: los archivos que Android tiene registrados (+15 % de ocultos).
        val estimate = try {
            ctx.contentResolver.query(MediaStore.Files.getContentUri("external"), arrayOf(MediaStore.MediaColumns._ID), null, null, null)?.use { it.count } ?: 0
        } catch (e: Exception) {
            0
        }.let { (it * 1.15).toInt().coerceAtLeast(500) }

        // Memoria interna + tarjeta microSD (el Realme C11 tiene ranura).
        val roots = buildList {
            add(root)
            try {
                ctx.getSystemService(android.os.storage.StorageManager::class.java).storageVolumes
                    .filter { it.isRemovable }
                    .mapNotNull { it.directory }
                    .filter { it.canRead() }
                    .forEach { add(it) }
            } catch (e: Exception) {
            }
        }

        // Recorrido iterativo (sin recursión): poca memoria y sin riesgo de desbordar la pila.
        val stack = ArrayDeque<Triple<File, Int, String>>()
        roots.forEach { stack.add(Triple(it, 0, it.absolutePath)) }
        while (stack.isNotEmpty() && !cancelled) {
            val (dir, depth, base) = stack.removeLast()
            val rel = dir.absolutePath.removePrefix(base).lowercase()
            if (rel == "/android/data" || rel == "/android/obb") continue
            val children = dir.listFiles() ?: continue
            // Las carpetas vacías de apps (Android/...) no se tocan: algunas apps las esperan.
            if (children.isEmpty() && depth >= 2 && !rel.startsWith("/dcim/camera") && !rel.startsWith("/android/")) {
                empty.add(dir, 0)
                continue
            }
            for (f in children) {
                // Ya están en la Papelera de Android (o a medio guardar): no se tocan ni se listan.
                if (f.name.startsWith(".trashed-") || f.name.startsWith(".pending-")) continue
                if (f.isDirectory) {
                    stack.add(Triple(f, depth + 1, base))
                    continue
                }
                seen++
                if (seen % 200 == 0) onProgress(f.parentFile?.name ?: "", seen, (seen.toFloat() / estimate).coerceAtMost(0.97f))
                classify(f, rel + "/", now, installed, thumbs, temp, statuses, apks, shots, sent, voice, big, lost, prot, bySize)
            }
        }
        onProgress("buscando duplicados", seen, 0.98f)
        findDuplicates(bySize, dups, prot)
        onProgress("", seen, 1f)
        return listOf(thumbs, temp, statuses, empty, apks, shots, dups, sent, voice, big, lost, prot)
    }

    private fun classify(
        f: File, dirRel: String, now: Long, installed: Map<String, Long>,
        thumbs: Category, temp: Category, statuses: Category, apks: Category, shots: Category,
        sent: Category, voice: Category, big: Category, lost: Category, prot: Category,
        bySize: HashMap<Long, ArrayList<File>>,
    ) {
        val len = f.length()
        val ext = Rules.ext(f)
        val age = now - f.lastModified()
        val wa = dirRel.contains("/whatsapp/media/")
        when {
            dirRel.contains("/.thumbnails/") -> thumbs.add(f, len)
            dirRel.contains("/.statuses/") ->
                if (Rules.isVideo(f)) prot.add(f, len, "Estado de WhatsApp") else statuses.add(f, len)
            ext == "tmp" || ext == "temp" || (ext == "log" && !dirRel.startsWith("/documents/")) -> temp.add(f, len)
            ext == "apk" -> {
                val info = try { ctx.packageManager.getPackageArchiveInfo(f.absolutePath, 0) } catch (e: Exception) { null }
                val inst = info?.let { installed[it.packageName] }
                val note = if (info != null && inst != null && inst >= info.longVersionCode) "ya instalada" else ""
                apks.add(f, len, note)
            }
            dirRel.contains("/lost.dir/") -> lost.add(f, len)
            (dirRel.startsWith("/dcim/screenshots/") || dirRel.startsWith("/pictures/screenshots/")) && ext in Rules.IMAGES -> shots.add(f, len)
            wa && dirRel.contains("/sent/") && age > 30 * DAY ->
                if (Rules.isProtected(f)) prot.add(f, len, "Enviado por WhatsApp") else if (ext in Rules.IMAGES) sent.add(f, len)
            wa && dirRel.contains("whatsapp voice notes") && age > 90 * DAY -> voice.add(f, len)
            Rules.isProtected(f) -> {
                val oldDownload = (dirRel.startsWith("/download/") || wa) && age > 60 * DAY
                if (oldDownload || len > 100L * 1024 * 1024) prot.add(f, len, if (wa) "WhatsApp" else if (dirRel.startsWith("/download/")) "Descargas" else "")
            }
            len > 100L * 1024 * 1024 -> big.add(f, len)
        }
        if (ext in Rules.IMAGES && len > 50 * 1024 && !dirRel.contains("/.thumbnails/") && !dirRel.contains("/.statuses/")) {
            bySize.getOrPut(len) { ArrayList(2) }.add(f)
        }
    }

    /** Duplicados exactos: mismo tamaño y misma huella (inicio + final del archivo). */
    private fun findDuplicates(bySize: HashMap<Long, ArrayList<File>>, dups: Category, prot: Category) {
        for ((_, files) in bySize) {
            if (files.size < 2 || cancelled) continue
            val byHash = files.groupBy { quickHash(it) }
            for ((h, group) in byHash) {
                if (h.isEmpty() || group.size < 2) continue
                val sorted = group.sortedBy { it.lastModified() }
                val original = sorted.first()
                for (copy in sorted.drop(1)) {
                    if (Rules.isProtected(copy)) prot.add(copy, copy.length(), "Copia de ${original.name}")
                    else dups.add(copy, copy.length(), "copia de ${original.name}")
                }
            }
        }
    }

    private fun quickHash(f: File): String = try {
        val md = MessageDigest.getInstance("MD5")
        RandomAccessFile(f, "r").use { raf ->
            val n = 64 * 1024
            val buf = ByteArray(n)
            val first = raf.read(buf, 0, minOf(n.toLong(), raf.length()).toInt())
            if (first > 0) md.update(buf, 0, first)
            if (raf.length() > 2L * n) {
                raf.seek(raf.length() - n)
                val last = raf.read(buf)
                if (last > 0) md.update(buf, 0, last)
            }
        }
        md.digest().joinToString("") { "%02x".format(it) }
    } catch (e: Exception) {
        ""
    }
}

object Cleaner {
    /** Borra las categorías SEGURAS marcadas. Doble seguro: nunca videos ni documentos. */
    fun cleanSafe(cats: List<Category>, onProgress: (Float) -> Unit = {}): Pair<Long, Int> {
        var freed = 0L
        var n = 0
        val chosen = cats.filter { it.kind == Kind.SAFE && it.selected }
        val total = chosen.sumOf { it.items.size }.coerceAtLeast(1)
        var done = 0
        for (c in chosen) {
            for (it in c.items) {
                if (++done % 50 == 0) onProgress(done.toFloat() / total)
                if (it.file.isDirectory) {
                    if ((it.file.list()?.isEmpty() == true) && it.file.delete()) n++
                    continue
                }
                if (Rules.isProtected(it.file)) continue
                if (it.file.delete()) {
                    freed += it.size; n++
                }
            }
        }
        return freed to n
    }

    /**
     * Busca cada archivo en MediaStore para poder enviarlo a la Papelera del sistema.
     * Si Android aún no lo tenía indexado (pasa con archivos que dejan algunas apps),
     * se le pide que lo indexe primero: así casi todo puede ir a la Papelera.
     */
    fun mediaUris(ctx: Context, files: List<File>): Map<File, Uri> {
        val found = query(ctx, files)
        val missing = files.filter { it !in found && it.exists() }
        if (missing.isEmpty()) return found
        val scanned = java.util.concurrent.ConcurrentHashMap<File, Uri>()
        val latch = java.util.concurrent.CountDownLatch(missing.size)
        android.media.MediaScannerConnection.scanFile(ctx, missing.map { it.absolutePath }.toTypedArray(), null) { path, uri ->
            if (uri != null) scanned[File(path)] = uri
            latch.countDown()
        }
        latch.await(20, java.util.concurrent.TimeUnit.SECONDS)
        return found + query(ctx, missing.filter { !scanned.containsKey(it) }) + scanned
    }

    private fun query(ctx: Context, files: List<File>): Map<File, Uri> {
        val out = HashMap<File, Uri>()
        val base = MediaStore.Files.getContentUri("external")
        for (chunk in files.chunked(80)) {
            val sel = "${MediaStore.MediaColumns.DATA} IN (${chunk.joinToString(",") { "?" }})"
            val args = chunk.map { it.absolutePath }.toTypedArray()
            ctx.contentResolver.query(base, arrayOf(MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DATA), sel, args, null)?.use { c ->
                while (c.moveToNext()) {
                    out[File(c.getString(1))] = ContentUris.withAppendedId(base, c.getLong(0))
                }
            }
        }
        return out
    }
}
