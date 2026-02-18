pub mod circle;
pub mod rect;
pub mod text;
pub mod shapes_manager;
pub mod utilities;
pub mod fonts;

use crate::app_state::AppState;
use eframe::egui;

/// Trait that all shapes must implement to be fully integrated into the system automatically.
pub trait ShapeDescriptor {
    /// The keyword used in the DSL (e.g., "circle", "rect").
    fn dsl_keyword(&self) -> &'static str;
    
    /// Visual icon used in the Scene Graph and toolbars.
    fn icon(&self) -> &'static str;
    
    /// Render the property editor in the Element Modifiers modal.
    fn draw_modifiers(&mut self, ui: &mut egui::Ui, state: &mut AppState);

    /// Generate the DSL representation for this shape.
    fn to_dsl(&self, indent: &str) -> String;

    /// Generate a default instance for the "Add Element" menu.
    fn create_default(name: String) -> shapes_manager::Shape where Self: Sized;
}
