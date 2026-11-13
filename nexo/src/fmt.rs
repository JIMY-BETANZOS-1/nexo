use std::time::{SystemTime, UNIX_EPOCH};

pub fn bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    let s = if i == 0 {
        format!("{} B", b)
    } else if v >= 100.0 {
        format!("{:.0} {}", v, U[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, U[i])
    } else {
        format!("{:.2} {}", v, U[i])
    };
    s.replace('.', ",")
}

pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn date(ts: u64) -> String {
    let (y, mo, d, h, mi) = crate::sys::local_time(ts);
    format!("{:02}/{:02}/{} · {:02}:{:02}", d, mo, y, h, mi)
}

pub fn ago(ts: u64) -> String {
    let s = now().saturating_sub(ts);
    match s {
        0..=59 => "hace un momento".into(),
        60..=3599 => format!("hace {} min", s / 60),
        3600..=86_399 => format!("hace {} h", s / 3600),
        _ => {
            let d = s / 86_400;
            if d == 1 { "hace 1 día".into() } else { format!("hace {} días", d) }
        }
    }
}

/// Recorta una ruta larga dejando el final visible: "…\carpeta\archivo.tmp".
pub fn short_path(p: &std::path::Path, max: usize) -> String {
    let s = p.display().to_string();
    let n = s.chars().count();
    if n <= max {
        s
    } else {
        let tail: String = s.chars().skip(n - (max - 1)).collect();
        format!("…{}", tail)
    }
}

/// 75.0 → "1 min 15 s"; 8.4 → "8 s".
pub fn secs(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    if s < 60 {
        format!("{} s", s)
    } else if s < 3600 {
        format!("{} min {:02} s", s / 60, s % 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}
