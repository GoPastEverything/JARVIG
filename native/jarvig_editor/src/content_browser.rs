//! Content Browser view model. The panel paints this. It does not walk the disk.

use jarvig_core::{AssetId, RegistryAsset};

pub const FILTERS: [&str; 8] = ["All", "Textures", "Models", "Materials", "Levels", "Source", "Config", "Other"];

#[derive(Clone)]
pub struct Thumb {
    pub id: AssetId,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct BrowserModel {
    pub assets: Vec<RegistryAsset>,
    pub filter: usize,
    pub query: String,
    pub selected: Option<AssetId>,
    pub scroll: i32,
    pub load_us: u64,
    pub scan_us: u64,
    pub reused: u32,
    pub thumbnail_bytes: u64,
    pub from_cache: bool,
    pub thumbs: Vec<Thumb>,
}

impl Default for BrowserModel {
    fn default() -> Self {
        Self {
            assets: Vec::new(),
            filter: 0,
            query: String::new(),
            selected: None,
            scroll: 0,
            load_us: 0,
            scan_us: 0,
            reused: 0,
            thumbnail_bytes: 0,
            from_cache: false,
            thumbs: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct BrowserRow {
    pub index: usize,
    pub y: i32,
}

impl BrowserModel {
    pub fn visible(&self) -> Vec<usize> {
        let query = self.query.to_ascii_lowercase();
        self.assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| kind_matches(self.filter, &asset.kind) && (query.is_empty() || asset.name.to_ascii_lowercase().contains(&query) || asset.path.to_ascii_lowercase().contains(&query) || asset.subtype.to_ascii_lowercase().contains(&query)))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn rows(&self, top: i32, row_h: i32) -> Vec<BrowserRow> {
        self.visible().into_iter().enumerate().map(|(slot, index)| BrowserRow { index, y: top + slot as i32 * row_h - self.scroll }).collect()
    }

    pub fn footer(&self) -> String {
        let selected = self.selected.and_then(|id| self.assets.iter().find(|asset| asset.id == id));
        let detail = selected
            .map(|asset| {
                if asset.kind == "Texture" {
                    format!("{} | {} | {} | {} bytes", asset.kind, asset.subtype, asset.format, asset.bytes)
                } else if asset.triangles > 0 {
                    format!("{} | {} | {} triangles | {} bytes", asset.kind, asset.subtype, asset.triangles, asset.bytes)
                } else {
                    format!("{} | {} | {} | {}", asset.kind, asset.subtype, asset.format, asset.path)
                }
            })
            .unwrap_or_else(|| {
                format!(
                    "{} assets | registry {} us | scan {} us | reused {} | thumbnails {} bytes{}",
                    self.assets.len(),
                    self.load_us,
                    self.scan_us,
                    self.reused,
                    self.thumbnail_bytes,
                    if self.from_cache { " | cache" } else { "" }
                )
            });
        detail
    }

    pub fn thumb(&self, id: AssetId) -> Option<&Thumb> {
        self.thumbs.iter().find(|thumb| thumb.id == id)
    }
}

pub fn kind_matches(filter: usize, kind: &str) -> bool {
    match FILTERS.get(filter).copied().unwrap_or("All") {
        "All" => true,
        "Textures" => kind == "Texture",
        "Models" => kind == "Model",
        "Materials" => kind == "Material",
        "Levels" => kind == "Level",
        "Source" => kind == "Source" || kind == "Shader",
        "Config" => kind == "Config",
        "Other" => !matches!(kind, "Texture" | "Model" | "Material" | "Level" | "Source" | "Shader" | "Config"),
        _ => true,
    }
}

pub fn drop_caption(asset: &RegistryAsset, over_viewport: bool) -> String {
    if !over_viewport {
        return format!("{}  cannot drop here", asset.path);
    }
    match asset.kind.as_str() {
        "Model" => format!("{}  + Create Mesh Actor", asset.name),
        "Prefab" => format!("{}  + Place Prefab", asset.name),
        "Character" => format!("{}  open in Character Editor, not spawned", asset.name),
        "Level" => format!("{}  open level, not spawned", asset.name),
        "Source" | "Config" | "Shader" => format!("{}  does not enter the world", asset.name),
        "Material" => format!("{}  assign is not wired in this slice", asset.name),
        "Texture" => format!("{}  assign is not wired in this slice", asset.name),
        _ => format!("{}  cannot drop here", asset.name),
    }
}

pub fn drop_places_actor(asset: &RegistryAsset, over_viewport: bool) -> bool {
    over_viewport && (asset.kind == "Model" || asset.kind == "Prefab")
}
