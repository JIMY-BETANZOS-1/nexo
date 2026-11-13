import Foundation

/// Reglas de NEXO para iPhone. Es lógica pura (sin acceso a Fotos) para poder probarla.
///
/// Las mismas garantías que en Windows, Linux, macOS y Android:
/// - Nada se borra sin que el usuario lo vea y lo confirme.
/// - Lo borrado va a «Eliminado recientemente» de Fotos (recuperable durante 30 días).
/// - Los videos están protegidos: nunca vienen marcados y piden una confirmación extra.
enum Rules {

    enum MediaKind: String, Codable {
        case photo, screenshot, video
    }

    struct MediaItem: Identifiable, Hashable {
        let id: String
        let kind: MediaKind
        let width: Int
        let height: Int
        let created: Date
        let bytes: Int64
        let isFavorite: Bool
    }

    /// Grupos que muestra la pantalla Limpiar, del más seguro al más delicado.
    enum Group: String, CaseIterable, Identifiable {
        case screenshots = "Capturas de pantalla"
        case duplicates = "Fotos duplicadas"
        case largeVideos = "Videos grandes"
        var id: String { rawValue }

        var detail: String {
            switch self {
            case .screenshots: return "Imágenes de capturas. Van a «Eliminado recientemente» (recuperables 30 días)."
            case .duplicates: return "Copias de la misma foto. Se conserva una (la favorita o la más antigua)."
            case .largeVideos: return "PROTEGIDOS: nunca se marcan solos. Solo si tú los eliges, y con doble confirmación."
            }
        }

        var isProtected: Bool { self == .largeVideos }
    }

    /// Un video de más de 100 MB se considera grande.
    static let largeVideoBytes: Int64 = 100 * 1024 * 1024

    /// Los videos nunca se seleccionan automáticamente.
    static func isProtected(_ kind: MediaKind) -> Bool {
        kind == .video
    }

    static func screenshots(_ items: [MediaItem]) -> [MediaItem] {
        items.filter { $0.kind == .screenshot && !$0.isFavorite }.sorted { $0.bytes > $1.bytes }
    }

    static func largeVideos(_ items: [MediaItem]) -> [MediaItem] {
        items.filter { $0.kind == .video && $0.bytes >= largeVideoBytes && !$0.isFavorite }.sorted { $0.bytes > $1.bytes }
    }

    /// Fotos duplicadas: mismas dimensiones, mismo tamaño en bytes (si se conoce) y tomadas
    /// con menos de 3 segundos de diferencia. Devuelve solo las copias sobrantes de cada
    /// grupo: se conserva la favorita o, si no hay, la más antigua.
    static func duplicateExtras(_ items: [MediaItem]) -> [MediaItem] {
        let photos = items.filter { $0.kind == .photo }.sorted { $0.created < $1.created }
        var extras: [MediaItem] = []
        var used = Set<String>()
        for (i, a) in photos.enumerated() where !used.contains(a.id) {
            var group = [a]
            for b in photos[(i + 1)...] {
                if b.created.timeIntervalSince(a.created) > 3 { break }
                if !used.contains(b.id) && isDuplicate(a, b) {
                    group.append(b)
                }
            }
            guard group.count > 1 else { continue }
            group.forEach { used.insert($0.id) }
            let keep = group.first(where: { $0.isFavorite }) ?? group[0]
            extras.append(contentsOf: group.filter { $0.id != keep.id && !$0.isFavorite })
        }
        return extras
    }

    static func isDuplicate(_ a: MediaItem, _ b: MediaItem) -> Bool {
        guard a.width == b.width, a.height == b.height else { return false }
        guard abs(a.created.timeIntervalSince(b.created)) <= 3 else { return false }
        if a.bytes > 0 && b.bytes > 0 { return a.bytes == b.bytes }
        return true
    }

    // MARK: Escudo

    /// 0 correcto · 1 conviene revisar · 2 importante · 3 consejo / sin datos.
    struct Check: Identifiable {
        let id = UUID()
        let state: Int
        let title: String
        let detail: String
    }

    /// Nota de 0 a 100: −35 por cada problema importante, −12 por cada aviso;
    /// los consejos no restan (igual que en la versión de escritorio).
    static func securityScore(_ checks: [Check]) -> Int {
        var s = 100
        for c in checks {
            switch c.state {
            case 2: s -= 35
            case 1: s -= 12
            default: break
            }
        }
        return max(0, min(100, s))
    }

    static func grade(_ score: Int) -> String {
        switch score {
        case 90...: return "Excelente"
        case 75..<90: return "Bueno"
        case 55..<75: return "Mejorable"
        default: return "Necesita atención"
        }
    }
}

/// Formatos en español: «1,50 KB», «2,40 GB», «1.234».
enum Fmt {
    static func bytes(_ b: Int64) -> String {
        let units = ["B", "KB", "MB", "GB", "TB"]
        var v = Double(b)
        var i = 0
        while v >= 1024 && i < units.count - 1 {
            v /= 1024
            i += 1
        }
        let s: String
        if i == 0 {
            s = "\(b) B"
        } else if v >= 100 {
            s = String(format: "%.0f %@", v, units[i])
        } else if v >= 10 {
            s = String(format: "%.1f %@", v, units[i])
        } else {
            s = String(format: "%.2f %@", v, units[i])
        }
        return s.replacingOccurrences(of: ".", with: ",")
    }

    static func count(_ n: Int) -> String {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        f.groupingSeparator = "."
        f.usesGroupingSeparator = true
        return f.string(from: NSNumber(value: n)) ?? "\(n)"
    }
}
