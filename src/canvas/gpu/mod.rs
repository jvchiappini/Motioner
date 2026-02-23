//! Este es el módulo principal encargado de la aceleración por hardware (GPU).
//! Divide las responsabilidades en tipos, recursos, computación y renderizado.

pub mod compute;
pub mod render;
pub mod resources;
pub mod types;
pub mod utils;

// Re-exports para mantener la compatibilidad con el resto del código
pub use resources::*;
pub use types::*;
// La re-exportación de compute ya no es necesaria si no se usa externamente
// pub use compute::*;
pub use render::*;
pub use utils::*;
