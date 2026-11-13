import Photos
import UIKit

/// Acceso a la galería (PhotoKit). Lee la lista de fotos y videos, genera miniaturas y
/// envía a «Eliminado recientemente» solo lo que el usuario confirma.
@MainActor
final class Library: ObservableObject {

    enum State: Equatable {
        case idle
        case denied
        case scanning(Double)
        case done
    }

    @Published var state: State = .idle
    @Published var groups: [Rules.Group: [Rules.MediaItem]] = [:]
    @Published var selected = Set<String>()
    @Published var started = Date()

    private var assets: [String: PHAsset] = [:]
    private let imageManager = PHCachingImageManager()

    func size(of g: Rules.Group) -> Int64 {
        (groups[g] ?? []).reduce(0) { $0 + $1.bytes }
    }

    var selectedBytes: Int64 {
        groups.values.flatMap { $0 }.filter { selected.contains($0.id) }.reduce(0) { $0 + $1.bytes }
    }

    var selectedHasVideos: Bool {
        (groups[.largeVideos] ?? []).contains { selected.contains($0.id) }
    }

    func requestAndScan() {
        PHPhotoLibrary.requestAuthorization(for: .readWrite) { status in
            Task { @MainActor in
                if status == .authorized || status == .limited {
                    self.scan()
                } else {
                    self.state = .denied
                }
            }
        }
    }

    func scan() {
        state = .scanning(0)
        started = Date()
        selected = []
        Task.detached(priority: .userInitiated) {
            let opts = PHFetchOptions()
            opts.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: true)]
            let result = PHAsset.fetchAssets(with: opts)
            let total = max(result.count, 1)
            var items: [Rules.MediaItem] = []
            var map: [String: PHAsset] = [:]
            items.reserveCapacity(result.count)
            result.enumerateObjects { asset, i, _ in
                let kind: Rules.MediaKind
                if asset.mediaType == .video {
                    kind = .video
                } else if asset.mediaSubtypes.contains(.photoScreenshot) {
                    kind = .screenshot
                } else if asset.mediaType == .image {
                    kind = .photo
                } else {
                    return
                }
                // Tamaño real del archivo original (si el iPhone lo tiene localmente).
                let res = PHAssetResource.assetResources(for: asset).first
                let bytes = (res?.value(forKey: "fileSize") as? NSNumber)?.int64Value ?? 0
                items.append(Rules.MediaItem(
                    id: asset.localIdentifier, kind: kind,
                    width: asset.pixelWidth, height: asset.pixelHeight,
                    created: asset.creationDate ?? .distantPast, bytes: bytes,
                    isFavorite: asset.isFavorite))
                map[asset.localIdentifier] = asset
                if i % 200 == 0 {
                    let f = Double(i) / Double(total) * 0.95
                    Task { @MainActor in self.state = .scanning(f) }
                }
            }
            let g: [Rules.Group: [Rules.MediaItem]] = [
                .screenshots: Rules.screenshots(items),
                .duplicates: Rules.duplicateExtras(items),
                .largeVideos: Rules.largeVideos(items),
            ]
            await MainActor.run {
                self.assets = map
                self.groups = g
                self.state = .done
            }
        }
    }

    func thumbnail(_ id: String, size: CGFloat, done: @escaping (UIImage?) -> Void) {
        guard let a = assets[id] else { return done(nil) }
        let o = PHImageRequestOptions()
        o.deliveryMode = .opportunistic
        o.isNetworkAccessAllowed = false
        let s = CGSize(width: size * UIScreen.main.scale, height: size * UIScreen.main.scale)
        imageManager.requestImage(for: a, targetSize: s, contentMode: .aspectFill, options: o) { img, _ in done(img) }
    }

    /// Envía lo seleccionado a «Eliminado recientemente». iOS muestra su propia
    /// confirmación antes de hacerlo. Devuelve (elementos, bytes) liberables.
    func deleteSelected(completion: @escaping (Bool, Int, Int64) -> Void) {
        let ids = selected
        let bytes = selectedBytes
        let list = ids.compactMap { assets[$0] }
        PHPhotoLibrary.shared().performChanges({
            PHAssetChangeRequest.deleteAssets(list as NSArray)
        }) { ok, _ in
            Task { @MainActor in
                if ok {
                    for g in Rules.Group.allCases {
                        self.groups[g]?.removeAll { ids.contains($0.id) }
                    }
                    self.selected = []
                    History.add(items: list.count, bytes: bytes)
                }
                completion(ok, list.count, bytes)
            }
        }
    }
}

/// Historial mínimo de limpiezas (UserDefaults).
enum History {
    private static let key = "historial"

    struct Entry: Codable {
        let date: Date
        let items: Int
        let bytes: Int64
    }

    static func all() -> [Entry] {
        guard let d = UserDefaults.standard.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([Entry].self, from: d)) ?? []
    }

    static func add(items: Int, bytes: Int64) {
        var a = all()
        a.append(Entry(date: Date(), items: items, bytes: bytes))
        if a.count > 100 { a.removeFirst(a.count - 100) }
        if let d = try? JSONEncoder().encode(a) {
            UserDefaults.standard.set(d, forKey: key)
        }
    }
}
