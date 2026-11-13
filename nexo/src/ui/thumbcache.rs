//! Miniaturas para la interfaz: se generan en varios hilos y se guardan como texturas.
//!
//! - Solo se piden las filas visibles; las más recientes se atienden primero, así
//!   al desplazarte rápido no se pierde tiempo en lo que ya no estás viendo.
//! - Memoria acotada: como mucho 300 miniaturas pequeñas y 24 vistas grandes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub const SMALL: u32 = 64;
pub const BIG: u32 = 360;
const MAX_SMALL: usize = 300;
const MAX_BIG: usize = 24;
const MAX_QUEUE: usize = 160;

type Key = (PathBuf, u32);

struct Queue {
    jobs: Mutex<Vec<Key>>,
    signal: Condvar,
}

pub struct ThumbCache {
    queue: Arc<Queue>,
    rx: Receiver<(Key, Option<ColorImage>)>,
    tex: HashMap<Key, Option<TextureHandle>>,
    pending: HashSet<Key>,
    order_small: VecDeque<Key>,
    order_big: VecDeque<Key>,
}

impl ThumbCache {
    pub fn new() -> Self {
        let queue = Arc::new(Queue { jobs: Mutex::new(Vec::new()), signal: Condvar::new() });
        let (tx, rx) = channel();
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4);
        for _ in 0..workers {
            let q = queue.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                crate::thumbs::init_thread();
                crate::sys::interactive_thread();
                loop {
                    let key = {
                        let mut jobs = q.jobs.lock().unwrap_or_else(|e| e.into_inner());
                        while jobs.is_empty() {
                            jobs = q.signal.wait(jobs).unwrap_or_else(|e| e.into_inner());
                        }
                        jobs.pop() // la última pedida = la que se ve ahora
                    };
                    let Some(key) = key else { continue };
                    let img = crate::thumbs::shell_thumbnail(&key.0, key.1)
                        .map(|(w, h, px)| ColorImage::from_rgba_premultiplied([w, h], &px));
                    if tx.send((key, img)).is_err() {
                        break;
                    }
                }
            });
        }
        ThumbCache {
            queue,
            rx,
            tex: HashMap::new(),
            pending: HashSet::new(),
            order_small: VecDeque::new(),
            order_big: VecDeque::new(),
        }
    }

    /// Recoge las miniaturas terminadas. Devuelve si aún hay trabajo pendiente.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
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

    /// La miniatura si ya está lista; si no, la pide y devuelve None (aún cargando).
    pub fn get(&mut self, path: &Path, size: u32) -> Option<Option<TextureHandle>> {
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
