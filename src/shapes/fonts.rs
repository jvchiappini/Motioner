use std::path::{Path, PathBuf};
use std::fs;
use eframe::egui;

pub fn list_system_fonts() -> Vec<(String, PathBuf)> {
    let mut fonts = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let font_dir = Path::new("C:\\Windows\\Fonts");
        if let Ok(entries) = fs::read_dir(font_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext == "ttf" || ext == "otf" {
                        if let Some(name) = path.file_stem() {
                            fonts.push((name.to_string_lossy().to_string(), path));
                        }
                    }
                }
            }
        }
    }
    // TODO: Add Linux/macOS paths if needed
    
    // Fallback?
    
    fonts.sort_by(|a, b| a.0.cmp(&b.0));
    fonts.dedup_by(|a, b| a.0 == b.0);
    fonts
}

pub fn list_workspace_fonts(project_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut fonts = Vec::new();
    scan_fonts_recursive(project_dir, &mut fonts);
    fonts.sort_by(|a, b| a.0.cmp(&b.0));
    fonts.dedup_by(|a, b| a.0 == b.0);
    fonts
}

fn scan_fonts_recursive(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_fonts_recursive(&path, out);
            } else if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "ttf" || ext == "otf" {
                    if let Some(name) = path.file_stem() {
                        out.push((name.to_string_lossy().to_string(), path));
                    }
                }
            }
        }
    }
}

pub fn load_font(font_def: &mut egui::FontDefinitions, name: &str, path: &std::path::Path) -> bool {
    // Check if already loaded
    if font_def.font_data.contains_key(name) {
        return false;
    }

    if let Ok(font_data) = fs::read(path) {
        font_def.font_data.insert(
            name.to_owned(),
            egui::FontData::from_owned(font_data),
        );
        
        
        // Also register as its own family name so FontFamily::Name("FontName") works
        font_def.families.insert(
            egui::FontFamily::Name(name.to_owned().into()),
            vec![name.to_owned()],
        );

        println!("[motioner] Loaded font data for: {} from {:?}", name, path);
        return true;
    }
    false
}

pub fn load_font_arc(path: &std::path::Path) -> Option<ab_glyph::FontArc> {
    if let Ok(font_data) = fs::read(path) {
        if let Ok(font) = ab_glyph::FontArc::try_from_vec(font_data) {
            return Some(font);
        }
    }
    None
}
