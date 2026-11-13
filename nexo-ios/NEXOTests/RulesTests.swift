import XCTest
@testable import NEXO

/// Pruebas de las reglas de NEXO para iPhone (las mismas garantías que en escritorio).
final class RulesTests: XCTestCase {

    private func item(_ id: String, _ kind: Rules.MediaKind, w: Int = 3024, h: Int = 4032, t: Double = 0, bytes: Int64 = 2_000_000, fav: Bool = false) -> Rules.MediaItem {
        Rules.MediaItem(id: id, kind: kind, width: w, height: h, created: Date(timeIntervalSince1970: 1_700_000_000 + t), bytes: bytes, isFavorite: fav)
    }

    func testVideosEstanProtegidos() {
        XCTAssertTrue(Rules.isProtected(.video))
        XCTAssertFalse(Rules.isProtected(.photo))
        XCTAssertFalse(Rules.isProtected(.screenshot))
        XCTAssertTrue(Rules.Group.largeVideos.isProtected)
    }

    func testCapturasNoIncluyenFavoritasNiFotos() {
        let items = [item("a", .screenshot), item("b", .screenshot, fav: true), item("c", .photo)]
        XCTAssertEqual(Rules.screenshots(items).map(\.id), ["a"])
    }

    func testVideosGrandesSoloSobreElUmbral() {
        let items = [item("v1", .video, bytes: 50_000_000), item("v2", .video, bytes: 300_000_000), item("p", .photo, bytes: 400_000_000)]
        XCTAssertEqual(Rules.largeVideos(items).map(\.id), ["v2"])
    }

    func testDuplicadosConservanUnaCopia() {
        let items = [item("a", .photo, t: 0), item("b", .photo, t: 1), item("c", .photo, t: 2)]
        let extras = Rules.duplicateExtras(items)
        XCTAssertEqual(Set(extras.map(\.id)), ["b", "c"])
    }

    func testDuplicadosConservanLaFavorita() {
        let items = [item("a", .photo, t: 0), item("b", .photo, t: 1, fav: true)]
        XCTAssertEqual(Rules.duplicateExtras(items).map(\.id), ["a"])
    }

    func testFotosDistintasNoSonDuplicadas() {
        let items = [item("a", .photo, t: 0), item("b", .photo, t: 10), item("c", .photo, w: 1080, h: 1920, t: 1), item("d", .photo, t: 2, bytes: 999)]
        XCTAssertTrue(Rules.duplicateExtras(items).isEmpty)
    }

    func testLosVideosNuncaSonDuplicadosABorrar() {
        let items = [item("a", .video, t: 0), item("b", .video, t: 1)]
        XCTAssertTrue(Rules.duplicateExtras(items).isEmpty)
    }

    func testNotaDeSeguridad() {
        let ok = Rules.Check(state: 0, title: "", detail: "")
        let consejo = Rules.Check(state: 3, title: "", detail: "")
        let aviso = Rules.Check(state: 1, title: "", detail: "")
        let grave = Rules.Check(state: 2, title: "", detail: "")
        XCTAssertEqual(Rules.securityScore([ok, consejo]), 100) // los consejos no restan
        XCTAssertEqual(Rules.securityScore([ok, aviso]), 88)
        XCTAssertEqual(Rules.securityScore([grave, grave, grave]), 0)
        XCTAssertEqual(Rules.grade(95), "Excelente")
        XCTAssertEqual(Rules.grade(60), "Mejorable")
    }

    func testFormatoEnEspanol() {
        XCTAssertEqual(Fmt.bytes(0), "0 B")
        XCTAssertEqual(Fmt.bytes(1536), "1,50 KB")
        XCTAssertEqual(Fmt.bytes(15 * 1024 * 1024), "15,0 MB")
        XCTAssertEqual(Fmt.count(1_234_567), "1.234.567")
    }
}
