import SwiftUI

@main
struct NEXOApp: App {
    @StateObject private var library = Library()
    /// Pestaña inicial. «-pestana 2» la elige al abrir (lo usa la integración continua
    /// para sacar capturas de cada pantalla en el simulador).
    @State private var tab: Int = {
        let a = ProcessInfo.processInfo.arguments
        if let i = a.firstIndex(of: "-pestana"), i + 1 < a.count, let n = Int(a[i + 1]) { return n }
        return 0
    }()

    init() {
        let bar = UITabBarAppearance()
        bar.configureWithOpaqueBackground()
        bar.backgroundColor = UIColor(C.panel)
        UITabBar.appearance().standardAppearance = bar
        UITabBar.appearance().scrollEdgeAppearance = bar
    }

    var body: some Scene {
        WindowGroup {
            TabView(selection: $tab) {
                PanelView().tabItem { Label("Panel", systemImage: "gauge.medium") }.tag(0)
                CleanView().tabItem { Label("Limpiar", systemImage: "trash") }.tag(1)
                EnergyView().tabItem { Label("Energía", systemImage: "battery.75") }.tag(2)
                ShieldView().tabItem { Label("Escudo", systemImage: "shield.lefthalf.filled") }.tag(3)
                AutoView().tabItem { Label("Auto", systemImage: "clock.arrow.circlepath") }.tag(4)
            }
            .environmentObject(library)
            .tint(C.cyan)
            .preferredColorScheme(.dark)
        }
    }
}

/// Fondo oscuro con rejilla, común a todas las pantallas.
struct Screen<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        ZStack {
            C.bg.ignoresSafeArea()
            GeometryReader { g in
                Path { p in
                    stride(from: 0, through: g.size.width, by: 32).forEach { x in
                        p.move(to: CGPoint(x: x, y: 0)); p.addLine(to: CGPoint(x: x, y: g.size.height))
                    }
                    stride(from: 0, through: g.size.height, by: 32).forEach { y in
                        p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: g.size.width, y: y))
                    }
                }
                .stroke(C.cyan.opacity(0.05), lineWidth: 1)
            }
            .ignoresSafeArea()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) { content }
                    .padding(16)
            }
        }
    }
}
