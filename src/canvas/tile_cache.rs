/// Implementa un sistema de almacenamiento en caché por cuadrículas (tiles).
/// Permite reutilizar porciones de la imagen renderizada si la escena no ha cambiado en esa zona.

use std::collections::HashMap;

/// Cache de tiles renderizados para evitar re-renderizar tiles sin cambios.
/// Almacena datos RGBA indexados por coordenadas de tile y hash de la escena.
#[derive(Clone)]
pub struct TileCache {
    pub tile_size: usize,
    pub tiles: HashMap<(usize, usize, u64), Vec<u8>>, // (x, y, scene_hash) -> rgba data
    pub max_tiles: usize,
}

impl TileCache {
    pub fn new(tile_size: usize, max_tiles: usize) -> Self {
        Self {
            tile_size,
            tiles: HashMap::new(),
            max_tiles,
        }
    }

    pub fn get(&self, x: usize, y: usize, hash: u64) -> Option<&[u8]> {
        self.tiles.get(&(x, y, hash)).map(|v| v.as_slice())
    }

    pub fn insert(&mut self, x: usize, y: usize, hash: u64, data: Vec<u8>) {
        if self.tiles.len() >= self.max_tiles {
            // LRU simple: remover primer elemento
            if let Some(key) = self.tiles.keys().next().cloned() {
                self.tiles.remove(&key);
            }
        }
        self.tiles.insert((x, y, hash), data);
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }
}
