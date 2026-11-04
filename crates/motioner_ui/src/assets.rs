//! The assets panel model: one row per project asset, list or grid.
//!
//! The window cannot read the project registry (`motioner_ui` must not depend
//! on `motioner_io`), so [`crate::ProjectHost`] fills [`AssetSummary`] values
//! and this module only sorts, filters and lays them out. Row text is built
//! once per refresh and reused every frame, so the panel adds no steady-state
//! allocations.

use std::fmt::Write as _;
use std::path::PathBuf;

/// Media kind shown in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKindUi {
    /// A raster image.
    Image,
    /// A static font face.
    Font,
    /// An audio clip.
    Audio,
    /// A video clip.
    Video,
}

impl AssetKindUi {
    /// The row label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Font => "font",
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }
}

/// One asset as the panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetSummary {
    /// Stable registry key.
    pub key: String,
    /// Original file name.
    pub original: String,
    /// Media kind.
    pub kind: AssetKindUi,
    /// Blob size in bytes.
    pub size_bytes: u64,
    /// Duration in seconds for audio and video.
    pub duration: Option<f64>,
    /// Number of layers that refer to it.
    pub used_by: usize,
    /// Waveform peaks in `0.0..=1.0` (audio only).
    pub peaks: Vec<f32>,
}

/// List or grid presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetView {
    /// One row per asset with columns.
    List,
    /// Two-column tiles with a waveform/kind strip.
    Grid,
}

impl AssetView {
    /// The opposite view.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::List => Self::Grid,
            Self::Grid => Self::List,
        }
    }
}

/// The assets panel state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AssetsModel {
    /// Every asset, sorted by key.
    pub entries: Vec<AssetSummary>,
    /// List or grid.
    pub view: Option<AssetView>,
    /// Selected row in `entries`.
    pub selected: Option<usize>,
    /// Row text cache, rebuilt when `generation` changes.
    detail: Vec<String>,
    /// Generation the cache belongs to.
    generation: u64,
}

impl AssetsModel {
    /// Replaces the entries and rebuilds the row text.
    pub fn set_entries(&mut self, mut entries: Vec<AssetSummary>, generation: u64) {
        entries.sort_by(|left, right| {
            left.key
                .to_ascii_lowercase()
                .cmp(&right.key.to_ascii_lowercase())
        });
        self.entries = entries;
        self.generation = generation;
        self.detail.clear();
        self.detail.reserve(self.entries.len());
        for entry in &self.entries {
            self.detail.push(detail_line(entry));
        }
        if self
            .selected
            .is_some_and(|index| index >= self.entries.len())
        {
            self.selected = None;
        }
    }

    /// The cached detail line of `index`.
    #[must_use]
    pub fn detail(&self, index: usize) -> &str {
        self.detail.get(index).map_or("", String::as_str)
    }

    /// The view (list by default).
    #[must_use]
    pub fn view(&self) -> AssetView {
        self.view.unwrap_or(AssetView::List)
    }

    /// Switches list/grid.
    pub fn toggle_view(&mut self) {
        self.view = Some(self.view().toggled());
    }

    /// Asset index under a virtualised list row.
    #[must_use]
    pub fn list_index(&self, row: usize, filter: &str) -> Option<usize> {
        self.visible(filter).nth(row)
    }

    /// Asset indices that pass `filter`, in registry order.
    pub fn visible<'a>(&'a self, filter: &'a str) -> impl Iterator<Item = usize> + 'a {
        let needle = filter.trim();
        self.entries
            .iter()
            .enumerate()
            .filter(move |(_, entry)| {
                needle.is_empty()
                    || contains_ignore_ascii_case(&entry.key, needle)
                    || contains_ignore_ascii_case(&entry.original, needle)
                    || contains_ignore_ascii_case(entry.kind.label(), needle)
            })
            .map(|(index, _)| index)
    }

    /// Count of assets that pass `filter`.
    #[must_use]
    pub fn visible_count(&self, filter: &str) -> usize {
        self.visible(filter).count()
    }

    /// The asset selected, when any.
    #[must_use]
    pub fn selected_asset(&self) -> Option<&AssetSummary> {
        self.selected.and_then(|index| self.entries.get(index))
    }

    /// Path to open when the user asks for the blob (Reveal).
    #[must_use]
    pub fn selected_path(&self, root: Option<&PathBuf>) -> Option<PathBuf> {
        let selected = self.selected_asset()?;
        let root = root?;
        Some(root.join(format!("assets/{}", selected.key)))
    }
}

/// Case-insensitive ASCII substring search that allocates nothing.
#[must_use]
pub fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// The detail column: size, duration and usage.
#[must_use]
pub fn detail_line(entry: &AssetSummary) -> String {
    let mut text = crate::format_size(entry.size_bytes);
    if let Some(duration) = entry.duration {
        let _ = write!(text, "  {duration:.1}s");
    }
    if entry.used_by == 0 {
        let _ = write!(text, "  unused");
    } else {
        let _ = write!(text, "  used by {}", entry.used_by);
    }
    text
}

/// Formats bytes without allocating more than one string (used by the host
/// and by tests).
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    #[allow(clippy::cast_precision_loss)] // display only
    let value = bytes as f64;
    #[allow(clippy::cast_precision_loss)]
    if bytes >= GIB {
        format!("{:.1} GB", value / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MB", value / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KB", value / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, used_by: usize) -> AssetSummary {
        AssetSummary {
            key: key.to_owned(),
            original: format!("{key}.png"),
            kind: AssetKindUi::Image,
            size_bytes: 2048,
            duration: None,
            used_by,
            peaks: Vec::new(),
        }
    }

    #[test]
    fn entries_sort_and_filter_and_keep_the_selection_valid() {
        let mut model = AssetsModel::default();
        model.set_entries(vec![entry("zebra", 0), entry("logo", 2)], 1);
        assert_eq!(model.entries[0].key, "logo");
        assert_eq!(model.visible_count(""), 2);
        assert_eq!(model.visible_count("zebra"), 1);
        assert_eq!(model.list_index(0, "logo"), Some(0));
        model.selected = Some(1);
        model.set_entries(vec![entry("logo", 2)], 2);
        assert_eq!(model.selected, None);
    }

    #[test]
    fn the_detail_line_names_usage() {
        assert_eq!(detail_line(&entry("logo", 2)), "2 KB  used by 2");
        assert!(detail_line(&entry("logo", 0)).ends_with("unused"));
    }
}
