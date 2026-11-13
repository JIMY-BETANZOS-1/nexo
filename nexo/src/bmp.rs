//! Codificador BMP común a todas las plataformas.

/// Codifica píxeles RGBA (fila a fila, de arriba abajo) como un BMP de 32 bits.
/// Lo usan la comprobación `--miniatura` y el modo de capturas de la documentación.
pub fn bmp(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(54 + rgba.len());
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&((54 + rgba.len()) as u32).to_le_bytes());
    b.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(-(h as i32)).to_le_bytes());
    b.extend_from_slice(&[1, 0, 32, 0, 0, 0, 0, 0]);
    b.extend_from_slice(&(rgba.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0u8; 16]);
    for p in rgba.as_chunks::<4>().0 {
        b.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    b
}
