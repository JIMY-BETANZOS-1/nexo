import SwiftUI

// MARK: Panel

struct PanelView: View {
    @State private var storage = Device.storage()
    @State private var battery: Device.Battery?

    var body: some View {
        Screen {
            HStack(spacing: 10) {
                Image(systemName: "hexagon").font(.system(size: 34)).foregroundColor(C.cyan)
                VStack(alignment: .leading) {
                    Text("N E X O").font(.system(size: 24, weight: .heavy, design: .rounded)).foregroundColor(C.text)
                    Text("OPTIMIZACIÓN INTELIGENTE · 100 % LOCAL").font(.system(size: 11, weight: .bold, design: .monospaced)).foregroundColor(C.cyan)
                }
            }
            Header(kicker: "Centro de control", title: "Tu iPhone", sub: Device.model)
            HudCard {
                HStack {
                    Spacer()
                    Ring(fraction: storage.usedFraction, color: storage.usedFraction > 0.9 ? C.red : C.cyan,
                         label: "\(Int(storage.usedFraction * 100))%", sub: "almacenamiento usado")
                    Spacer()
                }
                Text("\(Fmt.bytes(storage.free)) libres de \(Fmt.bytes(storage.total))").foregroundColor(C.muted).frame(maxWidth: .infinity)
            }
            HStack(spacing: 12) {
                HudCard(tint: C.green) {
                    Text("BATERÍA").font(.caption).foregroundColor(C.green)
                    Text(batteryText).font(.title3.bold()).foregroundColor(C.text)
                }
                HudCard(tint: C.cyan) {
                    Text("MEMORIA").font(.caption).foregroundColor(C.cyan)
                    Text(Fmt.bytes(Device.ramTotal)).font(.title3.bold()).foregroundColor(C.text)
                }
            }
            HudCard(tint: C.magenta) {
                Text("GARANTÍAS DE NEXO").font(.caption.bold()).foregroundColor(C.magenta)
                ForEach(["Tus videos nunca se marcan solos.", "Siempre ves lo que se va a borrar antes de confirmar.",
                         "Lo borrado va a «Eliminado recientemente» (30 días).", "100 % local: no envía nada a internet."], id: \.self) { g in
                    Label(g, systemImage: "checkmark").font(.footnote).foregroundColor(C.text)
                }
            }
            if let last = History.all().last {
                HudCard(tint: C.green) {
                    Text("ÚLTIMA LIMPIEZA").font(.caption).foregroundColor(C.muted)
                    Text("\(Fmt.bytes(last.bytes)) · \(last.items) elementos").font(.headline).foregroundColor(C.green)
                    Text(last.date.formatted(date: .abbreviated, time: .shortened)).font(.caption).foregroundColor(C.muted)
                }
            }
        }
        .onAppear {
            storage = Device.storage()
            battery = Device.battery()
        }
    }

    var batteryText: String {
        guard let b = battery, b.level >= 0 else { return "—" }
        let charging = b.state == .charging || b.state == .full
        return "\(Int(b.level * 100)) %\(charging ? " ⚡" : "")"
    }
}

// MARK: Limpiar

struct CleanView: View {
    @EnvironmentObject var lib: Library
    @State private var confirmVideos = false
    @State private var message: String?

    var body: some View {
        Screen {
            Header(kicker: "Limpieza inteligente", title: "Galería", sub: "Nada se borra sin que lo veas primero. Tus videos están protegidos.")
            switch lib.state {
            case .idle:
                HudCard {
                    Text("NEXO revisa tu galería en el iPhone: capturas, fotos duplicadas y videos grandes.").foregroundColor(C.text)
                    NeonButton(title: "Iniciar escaneo") { lib.requestAndScan() }
                }
            case .denied:
                HudCard(tint: C.amber) {
                    Text("Sin acceso a Fotos. Actívalo en Ajustes › NEXO › Fotos.").foregroundColor(C.amber)
                    NeonButton(title: "Abrir Ajustes", color: C.amber) {
                        if let u = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(u) }
                    }
                }
            case .scanning(let f):
                HudCard {
                    Text("\(Int(f * 100)) %").font(.system(size: 34, weight: .bold, design: .rounded)).foregroundColor(C.text).frame(maxWidth: .infinity)
                    BlockBar(fraction: f)
                    Text(eta(f)).font(.caption).foregroundColor(C.muted).frame(maxWidth: .infinity)
                }
            case .done:
                ForEach(Rules.Group.allCases) { g in groupCard(g) }
                HudCard(tint: C.green) {
                    Text("SELECCIONADO").font(.caption).foregroundColor(C.muted)
                    Text(Fmt.bytes(lib.selectedBytes)).font(.title.bold()).foregroundColor(C.green)
                    NeonButton(title: "Limpiar selección", color: C.green, enabled: !lib.selected.isEmpty) {
                        if lib.selectedHasVideos { confirmVideos = true } else { delete() }
                    }
                    Button("Volver a escanear") { lib.scan() }.font(.footnote).foregroundColor(C.cyan)
                }
            }
            if let m = message {
                Text(m).font(.footnote).foregroundColor(C.green)
            }
        }
        .alert("Incluiste videos", isPresented: $confirmVideos) {
            Button("Cancelar", role: .cancel) {}
            Button("Sí, enviarlos a Eliminado recientemente", role: .destructive) { delete() }
        } message: {
            Text("Los videos están protegidos. Irán a «Eliminado recientemente» y podrás recuperarlos durante 30 días.")
        }
    }

    func eta(_ f: Double) -> String {
        let s = Date().timeIntervalSince(lib.started)
        guard f > 0.05 else { return "\(Int(s)) s transcurridos" }
        return "\(Int(s)) s transcurridos · quedan ≈ \(Int(s * (1 - f) / f)) s"
    }

    func delete() {
        lib.deleteSelected { ok, n, bytes in
            message = ok ? "✔ \(n) elementos enviados a «Eliminado recientemente» · \(Fmt.bytes(bytes))" : "No se borró nada."
        }
    }

    @ViewBuilder
    func groupCard(_ g: Rules.Group) -> some View {
        let items = lib.groups[g] ?? []
        let color: Color = g.isProtected ? C.magenta : (g == .screenshots ? C.amber : C.cyan)
        HudCard(tint: color) {
            HStack {
                Text(g.rawValue).font(.headline).foregroundColor(C.text)
                Badge(text: g.isProtected ? "protegido" : "revisar", color: color)
                Spacer()
                Text(Fmt.bytes(lib.size(of: g))).font(.headline).foregroundColor(color)
            }
            Text(g.detail).font(.caption).foregroundColor(C.muted)
            if items.isEmpty {
                Text("✓ Nada que limpiar").font(.caption).foregroundColor(C.green)
            } else {
                if !g.isProtected {
                    Button(items.allSatisfy { lib.selected.contains($0.id) } ? "Quitar todo" : "Marcar todo (\(Fmt.count(items.count)))") {
                        let all = items.allSatisfy { lib.selected.contains($0.id) }
                        for it in items {
                            if all { lib.selected.remove(it.id) } else { lib.selected.insert(it.id) }
                        }
                    }
                    .font(.footnote).foregroundColor(color)
                }
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(spacing: 8) {
                        ForEach(items.prefix(60)) { it in Thumb(item: it, color: color) }
                    }
                }
                .frame(height: 96)
            }
        }
    }
}

struct Thumb: View {
    @EnvironmentObject var lib: Library
    let item: Rules.MediaItem
    let color: Color
    @State private var image: UIImage?

    var body: some View {
        let on = lib.selected.contains(item.id)
        ZStack(alignment: .topTrailing) {
            Group {
                if let i = image { Image(uiImage: i).resizable().scaledToFill() } else { C.panel }
            }
            .frame(width: 80, height: 80)
            .clipped()
            .overlay(Rectangle().stroke(on ? color : C.muted.opacity(0.3), lineWidth: on ? 3 : 1))
            Image(systemName: on ? "checkmark.square.fill" : "square").foregroundColor(on ? color : .white).padding(4)
        }
        .overlay(alignment: .bottomLeading) {
            Text(Fmt.bytes(item.bytes)).font(.system(size: 9, weight: .bold)).padding(2).background(.black.opacity(0.6)).foregroundColor(.white)
        }
        .onTapGesture { if on { lib.selected.remove(item.id) } else { lib.selected.insert(item.id) } }
        .onAppear { lib.thumbnail(item.id, size: 80) { image = $0 } }
    }
}

// MARK: Energía

struct EnergyView: View {
    @State private var b: Device.Battery?

    var body: some View {
        Screen {
            Header(kicker: "Batería y rendimiento", title: "Energía", sub: "Consejos para que la batería dure más y el iPhone vaya fluido.")
            if let b = b {
                HudCard(tint: C.green) {
                    HStack {
                        Ring(fraction: Double(max(b.level, 0)), color: b.level < 0.2 ? C.red : C.green,
                             label: b.level >= 0 ? "\(Int(b.level * 100))%" : "—", sub: stateText(b.state))
                        VStack(alignment: .leading, spacing: 6) {
                            Label(b.lowPower ? "Modo de bajo consumo ACTIVO" : "Modo de bajo consumo apagado", systemImage: "bolt.badge.automatic")
                                .foregroundColor(b.lowPower ? C.amber : C.muted)
                            Label("Temperatura: \(thermalText(b.thermal))", systemImage: "thermometer.medium")
                                .foregroundColor(b.thermal == .nominal || b.thermal == .fair ? C.green : C.red)
                        }
                        .font(.footnote)
                    }
                }
            }
            Kicker(text: "Recomendaciones", color: C.cyan)
            ForEach(tips, id: \.0) { t in
                HudCard(tint: C.cyan) {
                    Text(t.0).font(.headline).foregroundColor(C.text)
                    Text(t.1).font(.footnote).foregroundColor(C.muted)
                }
            }
        }
        .onAppear { b = Device.battery() }
    }

    let tips: [(String, String)] = [
        ("Carga optimizada", "Ajustes › Batería › Carga: activa «Carga optimizada» (o el límite del 80 %) para alargar la vida de la batería."),
        ("Estado de la batería", "Ajustes › Batería › Estado y carga: si la capacidad máxima baja del 80 %, conviene cambiarla."),
        ("Actualización en segundo plano", "Ajustes › General › Actualización en segundo plano: desactívala en las apps que no necesitan estar al día."),
        ("Brillo automático y modo oscuro", "Con pantalla OLED, el modo oscuro y el brillo automático ahorran batería."),
        ("Localización", "Ajustes › Privacidad › Localización: deja «Al usar la app» en lugar de «Siempre»."),
        ("Calor", "Evita cargar el iPhone al sol o jugando: el calor desgasta la batería más que nada."),
    ]

    func stateText(_ s: UIDevice.BatteryState) -> String {
        switch s {
        case .charging: return "cargando"
        case .full: return "completa"
        case .unplugged: return "con batería"
        default: return "batería"
        }
    }

    func thermalText(_ t: ProcessInfo.ThermalState) -> String {
        switch t {
        case .nominal: return "normal"
        case .fair: return "templado"
        case .serious: return "caliente"
        case .critical: return "crítica"
        @unknown default: return "—"
        }
    }
}

// MARK: Escudo

struct ShieldView: View {
    @State private var checks: [Rules.Check] = []
    @State private var progress: Double = 0
    @State private var loading = true

    var body: some View {
        Screen {
            Header(kicker: "Escudo", title: "Protección", sub: "Revisa la seguridad de tu iPhone sin internet.")
            if loading {
                HudCard(tint: C.orange) {
                    Text("\(Int(progress * 100)) %").font(.system(size: 34, weight: .bold, design: .rounded)).foregroundColor(C.text).frame(maxWidth: .infinity)
                    BlockBar(fraction: progress, color: C.orange)
                    Text("Revisando ajustes de seguridad…").font(.caption).foregroundColor(C.muted).frame(maxWidth: .infinity)
                }
            } else {
                let score = Rules.securityScore(checks)
                HudCard(tint: score >= 90 ? C.green : C.amber) {
                    HStack {
                        Ring(fraction: Double(score) / 100, color: score >= 90 ? C.green : (score >= 55 ? C.amber : C.red), label: "\(score)", sub: "de 100")
                        VStack(alignment: .leading) {
                            Text(score >= 90 ? "PROTEGIDO" : "EN RIESGO").font(.title2.bold()).foregroundColor(score >= 90 ? C.green : C.red)
                            Text(Rules.grade(score)).foregroundColor(C.muted)
                        }
                    }
                }
                ForEach(checks) { c in
                    HudCard(tint: C.state(c.state)) {
                        HStack {
                            Image(systemName: c.state == 0 ? "checkmark.circle" : (c.state == 3 ? "lightbulb" : "exclamationmark.triangle"))
                                .foregroundColor(C.state(c.state))
                            Text(c.title).font(.headline).foregroundColor(C.text)
                        }
                        Text(c.detail).font(.footnote).foregroundColor(C.muted)
                    }
                }
            }
        }
        .onAppear(perform: run)
    }

    func run() {
        loading = true
        progress = 0
        Task { @MainActor in
            for i in 1...5 {
                try? await Task.sleep(nanoseconds: 120_000_000)
                progress = Double(i) / 5
            }
            checks = Device.securityChecks()
            loading = false
        }
    }
}

// MARK: Auto

struct AutoView: View {
    @AppStorage("recordatorio") private var on = false
    @AppStorage("recordatorio_dia") private var weekday = 1 // domingo
    @AppStorage("recordatorio_hora") private var hour = 12
    @State private var note = ""

    var body: some View {
        Screen {
            Header(kicker: "Piloto automático", title: "Recordatorios", sub: "iOS no permite que una app borre fotos sola: NEXO te avisa para que revises y decidas.")
            HudCard {
                Toggle("Recordatorio semanal", isOn: $on).foregroundColor(C.text).tint(C.cyan)
                Picker("Día", selection: $weekday) {
                    ForEach(1...7, id: \.self) { d in Text(["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"][d - 1]).tag(d) }
                }
                .foregroundColor(C.text)
                Stepper("Hora: \(hour):00", value: $hour, in: 0...23).foregroundColor(C.text)
                NeonButton(title: "Guardar y aplicar") { apply() }
                if !note.isEmpty { Text(note).font(.footnote).foregroundColor(C.green) }
            }
            HudCard(tint: C.magenta) {
                Text("TUS ARCHIVOS ESTÁN A SALVO").font(.caption.bold()).foregroundColor(C.magenta)
                Text("El recordatorio solo te avisa. Nada se borra sin que lo veas y lo confirmes, y los videos nunca se marcan solos.")
                    .font(.footnote).foregroundColor(C.text)
            }
            Kicker(text: "Historial", color: C.cyan)
            ForEach(Array(History.all().reversed().prefix(20).enumerated()), id: \.offset) { _, e in
                HStack {
                    Text(e.date.formatted(date: .abbreviated, time: .shortened)).foregroundColor(C.muted)
                    Spacer()
                    Text("\(Fmt.bytes(e.bytes)) · \(e.items)").foregroundColor(C.green)
                }
                .font(.footnote)
            }
        }
    }

    func apply() {
        if on {
            Reminders.schedule(weekday: weekday, hour: hour) { ok in
                note = ok ? "✔ Te recordaré cada semana." : "Permite las notificaciones de NEXO en Ajustes."
            }
        } else {
            Reminders.cancel()
            note = "Recordatorio desactivado."
        }
    }
}
