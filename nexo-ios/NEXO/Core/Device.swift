import LocalAuthentication
import UIKit
import UserNotifications

/// Información del iPhone que iOS permite leer sin permisos especiales.
enum Device {

    struct Storage {
        let free: Int64
        let total: Int64
        var usedFraction: Double { total > 0 ? 1 - Double(free) / Double(total) : 0 }
    }

    static func storage() -> Storage {
        let url = URL(fileURLWithPath: NSHomeDirectory())
        let v = try? url.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey, .volumeTotalCapacityKey])
        return Storage(free: v?.volumeAvailableCapacityForImportantUsage ?? 0, total: Int64(v?.volumeTotalCapacity ?? 0))
    }

    static var ramTotal: Int64 { Int64(ProcessInfo.processInfo.physicalMemory) }

    static var model: String {
        var sys = utsname()
        uname(&sys)
        let id = withUnsafeBytes(of: &sys.machine) { p in String(decoding: p.prefix(while: { $0 != 0 }), as: UTF8.self) }
        return "\(UIDevice.current.name) · \(id)"
    }

    static var iosVersion: String { UIDevice.current.systemVersion }

    struct Battery {
        let level: Float // 0…1, o −1 si no se sabe (simulador)
        let state: UIDevice.BatteryState
        let lowPower: Bool
        let thermal: ProcessInfo.ThermalState
    }

    @MainActor
    static func battery() -> Battery {
        UIDevice.current.isBatteryMonitoringEnabled = true
        return Battery(level: UIDevice.current.batteryLevel, state: UIDevice.current.batteryState,
                       lowPower: ProcessInfo.processInfo.isLowPowerModeEnabled, thermal: ProcessInfo.processInfo.thermalState)
    }

    /// Indicios de un iPhone con jailbreak (protecciones de iOS desactivadas).
    static func jailbroken() -> Bool {
        #if targetEnvironment(simulator)
        return false
        #else
        let paths = ["/Applications/Cydia.app", "/Applications/Sileo.app", "/bin/bash", "/usr/sbin/sshd", "/etc/apt", "/private/var/lib/apt", "/var/jb"]
        if paths.contains(where: { FileManager.default.fileExists(atPath: $0) }) { return true }
        let probe = "/private/nexo_prueba.txt"
        if (try? "x".write(toFile: probe, atomically: true, encoding: .utf8)) != nil {
            try? FileManager.default.removeItem(atPath: probe)
            return true
        }
        return false
        #endif
    }

    /// Comprobaciones de seguridad (solo lectura).
    @MainActor
    static func securityChecks() -> [Rules.Check] {
        let ctx = LAContext()
        let passcode = ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil)
        let biometric = LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
        let major = Int(iosVersion.split(separator: ".").first ?? "0") ?? 0
        let st = storage()
        var c: [Rules.Check] = []
        c.append(passcode
            ? .init(state: 0, title: "Código de bloqueo", detail: "Activado: tus datos están cifrados si pierdes el iPhone")
            : .init(state: 2, title: "Sin código de bloqueo", detail: "Cualquiera puede abrir tu iPhone. Actívalo en Ajustes › Face ID y código."))
        c.append(biometric
            ? .init(state: 0, title: "Face ID / Touch ID", detail: "Configurado")
            : .init(state: 3, title: "Face ID / Touch ID", detail: "Consejo: configúralo para desbloquear rápido sin bajar la seguridad."))
        c.append(major >= 17
            ? .init(state: 0, title: "Versión de iOS", detail: "iOS \(iosVersion): recibe parches de seguridad")
            : .init(state: 1, title: "iOS \(iosVersion)", detail: "Actualiza en Ajustes › General › Actualización de software."))
        c.append(jailbroken()
            ? .init(state: 2, title: "Protecciones de iOS desactivadas (jailbreak)", detail: "Las apps pueden saltarse la seguridad de iOS.")
            : .init(state: 0, title: "Integridad del sistema", detail: "Sin indicios de jailbreak"))
        let freePct = st.total > 0 ? Double(st.free) / Double(st.total) : 1
        c.append(freePct < 0.10
            ? .init(state: 1, title: "Almacenamiento casi lleno", detail: "Quedan \(Fmt.bytes(st.free)). Con poco espacio, iOS no puede instalar actualizaciones.")
            : .init(state: 0, title: "Espacio para actualizaciones", detail: "\(Fmt.bytes(st.free)) libres"))
        return c
    }
}

/// Recordatorios locales (iOS no permite que una app limpie la galería en segundo plano).
enum Reminders {
    static let id = "nexo.recordatorio"

    static func schedule(weekday: Int, hour: Int, completion: @escaping (Bool) -> Void) {
        let center = UNUserNotificationCenter.current()
        center.requestAuthorization(options: [.alert, .sound]) { ok, _ in
            guard ok else { return DispatchQueue.main.async { completion(false) } }
            let content = UNMutableNotificationContent()
            content.title = "NEXO"
            content.body = "Hora de revisar capturas y fotos duplicadas. Nada se borra sin tu confirmación."
            var when = DateComponents()
            when.weekday = weekday
            when.hour = hour
            let req = UNNotificationRequest(identifier: id, content: content, trigger: UNCalendarNotificationTrigger(dateMatching: when, repeats: true))
            center.removePendingNotificationRequests(withIdentifiers: [id])
            center.add(req) { err in DispatchQueue.main.async { completion(err == nil) } }
        }
    }

    static func cancel() {
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: [id])
    }
}
