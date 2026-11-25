// Proyecto: NEXO
// Archivo: thumbcache.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

//! Miniaturas para la interfaz: se generan en varios hilos y se guardan como texturas.
//!
//! - Solo se piden las filas visibles; las más recientes se atienden primero, así
//!   al desplazarte rápido no se pierde tiempo en lo que ya no estás viendo.
//! - Memoria acotada: como mucho 300 miniaturas pequeñas y 24 vistas grandes.

// jimy-betanzos - dependencia: biblioteca estándar de Rust (collections)
use std::collections::{HashMap, HashSet, VecDeque};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::mpsc::{channel, Receiver};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::{Arc, Condvar, Mutex};

// jimy-betanzos - dependencia: ventana e interfaz gráfica (eframe/egui)
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub const SMALL: u32 = 64;
pub const BIG: u32 = 360;
const MAX_SMALL: usize = 300;
const MAX_BIG: usize = 24;
const MAX_QUEUE: usize = 160;

/// jimy-betanzos: Clave de una miniatura: archivo y tamaño.
type JbKey = (PathBuf, u32);

/// jimy-betanzos: Cola de miniaturas pendientes.
struct JbQueue {
    jobs: Mutex<Vec<JbKey>>,
    signal: Condvar,
}

/// jimy-betanzos: Caché de miniaturas de la interfaz.
pub struct JbThumbCache {
    queue: Arc<JbQueue>,
    rx: Receiver<(JbKey, Option<ColorImage>)>,
    tex: HashMap<JbKey, Option<TextureHandle>>,
    pending: HashSet<JbKey>,
    order_small: VecDeque<JbKey>,
    order_big: VecDeque<JbKey>,
}

/// jimy-betanzos: Métodos de ThumbCache.
impl JbThumbCache {
    /// jimy-betanzos: Crea la caché y sus hilos de trabajo.
    pub fn jb_new() -> Self {
        let queue = Arc::new(JbQueue { jobs: Mutex::new(Vec::new()), signal: Condvar::new() });
        let (tx, rx) = channel();
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4);
        for _ in 0..workers {
            let q = queue.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                crate::thumbs::jb_init_thread();
                crate::sys::jb_interactive_thread();
                loop {
                    let key = {
                        let mut jobs = q.jobs.lock().unwrap_or_else(|e| e.into_inner());
                        while jobs.is_empty() {
                            jobs = q.signal.wait(jobs).unwrap_or_else(|e| e.into_inner());
                        }
                        jobs.pop() // la última pedida = la que se ve ahora
                    };
                    let Some(key) = key else { continue };
                    let img = crate::thumbs::jb_shell_thumbnail(&key.0, key.1)
                        .map(|(w, h, px)| ColorImage::from_rgba_premultiplied([w, h], &px));
                    if tx.send((key, img)).is_err() {
                        break;
                    }
                }
            });
        }
        JbThumbCache {
            queue,
            rx,
            tex: HashMap::new(),
            pending: HashSet::new(),
            order_small: VecDeque::new(),
            order_big: VecDeque::new(),
        }
    }

    /// jimy-betanzos: Recoge las miniaturas terminadas. Devuelve si aún hay trabajo pendiente.
    pub fn jb_poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok((key, img)) = self.rx.try_recv() {
            self.pending.remove(&key);
            let t = img.map(|i| ctx.load_texture(format!("th{}:{}", key.1, key.0.display()), i, TextureOptions::LINEAR));
            let big = key.1 > SMALL;
            self.tex.insert(key.clone(), t);
            let (order, max) = if big { (&mut self.order_big, MAX_BIG) } else { (&mut self.order_small, MAX_SMALL) };
            order.push_back(key);
            while order.len() > max {
                if let Some(old) = order.pop_front() {
                    self.tex.remove(&old);
                }
            }
        }
        !self.pending.is_empty()
    }

    /// jimy-betanzos: La miniatura si ya está lista; si no, la pide y devuelve None (aún cargando).
    pub fn jb_get(&mut self, path: &Path, size: u32) -> Option<Option<TextureHandle>> {
        let key = (path.to_path_buf(), size);
        if let Some(t) = self.tex.get(&key) {
            return Some(t.clone());
        }
        if self.pending.insert(key.clone()) {
            let mut jobs = self.queue.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.push(key);
            if jobs.len() > MAX_QUEUE {
                // Lo más viejo de la cola ya no está en pantalla: se descarta.
                let extra = jobs.len() - MAX_QUEUE;
                for k in jobs.drain(0..extra) {
                    self.pending.remove(&k);
                }
            }
            self.queue.signal.notify_one();
        }
        None
    }
}
