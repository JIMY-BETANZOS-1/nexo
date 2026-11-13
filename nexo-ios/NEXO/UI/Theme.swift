import SwiftUI

/// Estética HUD de neón, la misma de NEXO en escritorio y Android.
enum C {
    static let bg = Color(red: 0.012, green: 0.027, blue: 0.051)
    static let panel = Color(red: 0.027, green: 0.059, blue: 0.102)
    static let cyan = Color(red: 0.22, green: 0.78, blue: 1.0)
    static let orange = Color(red: 1.0, green: 0.54, blue: 0.12)
    static let magenta = Color(red: 1.0, green: 0.28, blue: 0.77)
    static let green = Color(red: 0.24, green: 1.0, blue: 0.71)
    static let amber = Color(red: 1.0, green: 0.77, blue: 0.19)
    static let red = Color(red: 1.0, green: 0.19, blue: 0.28)
    static let text = Color(red: 0.83, green: 0.93, blue: 1.0)
    static let muted = Color(red: 0.42, green: 0.53, blue: 0.65)

    static func state(_ s: Int) -> Color {
        switch s {
        case 0: return green
        case 1: return amber
        case 2: return red
        default: return cyan
        }
    }
}

/// Rectángulo con las esquinas superior izquierda e inferior derecha biseladas.
struct Chamfer: Shape {
    var cut: CGFloat = 12
    func path(in r: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: r.minX + cut, y: r.minY))
        p.addLine(to: CGPoint(x: r.maxX, y: r.minY))
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY - cut))
        p.addLine(to: CGPoint(x: r.maxX - cut, y: r.maxY))
        p.addLine(to: CGPoint(x: r.minX, y: r.maxY))
        p.addLine(to: CGPoint(x: r.minX, y: r.minY + cut))
        p.closeSubpath()
        return p
    }
}

struct HudCard<Content: View>: View {
    var tint: Color = C.cyan
    @ViewBuilder var content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: 8) { content }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Chamfer().fill(C.panel))
            .overlay(Chamfer().stroke(tint.opacity(0.45), lineWidth: 1))
    }
}

/// Título de sección: «▌LIMPIEZA INTELIGENTE» con franja de advertencia.
struct Kicker: View {
    let text: String
    var color: Color = C.orange
    var body: some View {
        HStack(spacing: 8) {
            Rectangle().fill(color).frame(width: 18, height: 4)
            Text(text.uppercased()).font(.system(size: 12, weight: .bold, design: .monospaced)).foregroundColor(color).tracking(2)
        }
    }
}

struct Header: View {
    let kicker: String
    let title: String
    let sub: String
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Kicker(text: kicker)
            Text(title.uppercased()).font(.system(size: 26, weight: .heavy, design: .rounded)).foregroundColor(C.text)
            Text(sub).font(.subheadline).foregroundColor(C.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Anillo segmentado con porcentaje (como el del escritorio).
struct Ring: View {
    let fraction: Double
    var color: Color = C.cyan
    var label: String
    var sub: String = ""
    var body: some View {
        ZStack {
            ForEach(0..<36, id: \.self) { i in
                let on = Double(i) / 36 < fraction
                Capsule()
                    .fill(on ? color : color.opacity(0.15))
                    .frame(width: 4, height: 12)
                    .offset(y: -62)
                    .rotationEffect(.degrees(Double(i) * 10))
            }
            VStack(spacing: 2) {
                Text(label).font(.system(size: 30, weight: .bold, design: .rounded)).foregroundColor(C.text)
                if !sub.isEmpty { Text(sub.uppercased()).font(.caption2).foregroundColor(color).tracking(1) }
            }
        }
        .frame(width: 140, height: 140)
    }
}

/// Barra de progreso por bloques.
struct BlockBar: View {
    let fraction: Double
    var color: Color = C.cyan
    var body: some View {
        GeometryReader { g in
            let n = 30
            HStack(spacing: 3) {
                ForEach(0..<n, id: \.self) { i in
                    Rectangle().fill(Double(i) / Double(n) < fraction ? color : color.opacity(0.15))
                }
            }
            .frame(width: g.size.width)
        }
        .frame(height: 8)
    }
}

struct NeonButton: View {
    let title: String
    var color: Color = C.cyan
    var enabled: Bool = true
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text(title.uppercased())
                .font(.system(size: 15, weight: .bold, design: .rounded))
                .tracking(1.5)
                .frame(maxWidth: .infinity, minHeight: 48)
                .foregroundColor(enabled ? color : C.muted)
                .background(Chamfer(cut: 10).fill(color.opacity(enabled ? 0.12 : 0.04)))
                .overlay(Chamfer(cut: 10).stroke(enabled ? color : C.muted.opacity(0.4), lineWidth: 1.2))
        }
        .disabled(!enabled)
    }
}

struct Badge: View {
    let text: String
    var color: Color
    var body: some View {
        Text(text.uppercased())
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .padding(.horizontal, 6).padding(.vertical, 2)
            .foregroundColor(color)
            .overlay(Rectangle().stroke(color, lineWidth: 1))
    }
}
