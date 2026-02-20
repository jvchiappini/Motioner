/// Proporciona una estructura de datos de hash espacial para optimizar colisiones y renderizado.
/// Divide el espacio en celdas para realizar búsquedas rápidas de objetos cercanos.

#[allow(dead_code)]
use std::collections::HashMap;

/// Bounding box en coordenadas normalizadas (0..1) para cálculos de colisión y culling.
#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl BoundingBox {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub fn from_circle(x: f32, y: f32, radius: f32) -> Self {
        Self {
            min_x: (x - radius).max(0.0),
            min_y: (y - radius).max(0.0),
            max_x: (x + radius).min(1.0),
            max_y: (y + radius).min(1.0),
        }
    }

    pub fn from_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min_x: x.max(0.0),
            min_y: y.max(0.0),
            max_x: (x + w).min(1.0),
            max_y: (y + h).min(1.0),
        }
    }
}

/// Grid de hash espacial para culling eficiente de formas durante el renderizado.
#[derive(Clone)]
pub struct SpatialHashGrid {
    pub tile_size: f32,
    pub width: u32,
    pub height: u32,
    /// Mapea coordenadas de tile a una lista de índices de formas.
    pub grid: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHashGrid {
    pub fn new(width: u32, height: u32, tile_size: f32) -> Self {
        Self {
            tile_size,
            width,
            height,
            grid: HashMap::new(),
        }
    }

    /// Inserta una forma en el grid basada en su bounding box.
    pub fn insert(&mut self, shape_idx: usize, bbox: BoundingBox) {
        let min_x = ((bbox.min_x * self.width as f32) / self.tile_size).floor() as i32;
        let max_x = ((bbox.max_x * self.width as f32) / self.tile_size).ceil() as i32;
        let min_y = ((bbox.min_y * self.height as f32) / self.tile_size).floor() as i32;
        let max_y = ((bbox.max_y * self.height as f32) / self.tile_size).ceil() as i32;

        for tx in min_x..=max_x {
            for ty in min_y..=max_y {
                self.grid.entry((tx, ty)).or_default().push(shape_idx);
            }
        }
    }

    /// Consulta qué formas podrían intersectar una posición de pixel (normalizada 0..1).
    pub fn query(&self, x: f32, y: f32) -> &[usize] {
        let tx = ((x * self.width as f32) / self.tile_size).floor() as i32;
        let ty = ((y * self.height as f32) / self.tile_size).floor() as i32;
        self.grid
            .get(&(tx, ty))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.grid.clear();
    }
}
