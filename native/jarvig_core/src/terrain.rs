//! Chunked heightfield. The samples are the terrain. Chunk meshes are derived.
//!
//! Einstein detail is a stored record on the same actor. [`TerrainRecord::detail_displacement`]
//! returns nothing. It must not write [`TerrainRecord::heights`]. See `docs/terrain/foundation.md`.

use crate::{
    create_mesh, AuthoringError, FieldId, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshVertexAttribute, MeshVertexFormat, PropertyValue,
    SubmeshDesc, VertexStreamDesc, DETAIL_ERROR_THRESHOLD_PX, FIELD_TERRAIN_CHUNK, FIELD_TERRAIN_CLASS, FIELD_TERRAIN_CLIFF,
    FIELD_TERRAIN_COLLISION, FIELD_TERRAIN_DEBUG, FIELD_TERRAIN_DEBUG_COLORS, FIELD_TERRAIN_DENSITY, FIELD_TERRAIN_DEPTH,
    FIELD_TERRAIN_DISPLACEMENT, FIELD_TERRAIN_DISTANCE, FIELD_TERRAIN_EINSTEIN, FIELD_TERRAIN_EINSTEIN_COLLISION, FIELD_TERRAIN_ERROR,
    FIELD_TERRAIN_HEIGHT, FIELD_TERRAIN_HEIGHT_MAX, FIELD_TERRAIN_HEIGHT_MIN, FIELD_TERRAIN_LOD, FIELD_TERRAIN_MATERIAL, FIELD_TERRAIN_SEED,
    FIELD_TERRAIN_SPACING, FIELD_TERRAIN_WIDTH,
};

pub const TERRAIN_DEFAULT_WIDTH_M: f32 = 512.0;
pub const TERRAIN_DEFAULT_DEPTH_M: f32 = 512.0;
pub const TERRAIN_DEFAULT_SPACING_M: f32 = 1.0;
pub const TERRAIN_DEFAULT_CHUNK_M: f32 = 64.0;
pub const TERRAIN_DEFAULT_HEIGHT: f32 = 0.0;
pub const TERRAIN_DEFAULT_HEIGHT_MIN: f32 = -256.0;
pub const TERRAIN_DEFAULT_HEIGHT_MAX: f32 = 256.0;
pub const TERRAIN_MAX_QUADS: u32 = 2048;
pub const TERRAIN_BRUSH_RADIUS_M: f32 = 4.0;
pub const TERRAIN_BRUSH_DELTA_M: f32 = 0.35;
const SLOPE_FLAT_MAX: f32 = 0.25;
const SLOPE_SLOPE_MAX: f32 = 1.0;

/// How a later detail lookup projects onto the surface. Only horizontal XZ is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliffProjection {
    HorizontalXz,
    /// Reserved. Stored, not sampled, and not a height writer.
    DominantAxis,
}

impl CliffProjection {
    pub fn label(self) -> &'static str {
        match self {
            Self::HorizontalXz => "Horizontal XZ",
            Self::DominantAxis => "Dominant Axis",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "Horizontal XZ" | "HorizontalXZ" => Some(Self::HorizontalXz),
            "Dominant Axis" | "DominantAxis" => Some(Self::DominantAxis),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlopeClass {
    Flat,
    Slope,
    Cliff,
}

impl SlopeClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Slope => "slope",
            Self::Cliff => "cliff",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainBrush {
    Sculpt,
    Smooth,
    Flatten,
    Paint,
}

/// Brush weight from the cursor out to the radius. Not a height sample and not saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TerrainFalloff {
    #[default]
    Smooth,
    Linear,
}

impl TerrainFalloff {
    pub fn label(self) -> &'static str {
        match self {
            Self::Smooth => "Smooth",
            Self::Linear => "Linear",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "Smooth" => Some(Self::Smooth),
            "Linear" => Some(Self::Linear),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkCoord {
    pub x: u32,
    pub z: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EinsteinTerrainDetail {
    pub enabled: bool,
    pub seed: u32,
    pub density: f32,
    pub max_displacement_m: f32,
    pub error_threshold_px: f32,
    pub distance_m: f32,
    pub surface_class: u8,
    pub cliff: CliffProjection,
    pub debug_colors: bool,
    /// Must stay false. Enabling it is rejected.
    pub collision: bool,
}

impl Default for EinsteinTerrainDetail {
    fn default() -> Self {
        Self {
            enabled: false,
            seed: 1,
            density: 1.0,
            max_displacement_m: 0.05,
            error_threshold_px: DETAIL_ERROR_THRESHOLD_PX,
            distance_m: 8.0,
            surface_class: 0,
            cliff: CliffProjection::HorizontalXz,
            debug_colors: false,
            collision: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainRecord {
    pub width_m: f32,
    pub depth_m: f32,
    pub spacing_m: f32,
    pub chunk_m: f32,
    pub height_min: f32,
    pub height_max: f32,
    pub collision: bool,
    /// Stored. The chunk mesh is the only LOD. No second mesh is built.
    pub lod_enabled: bool,
    pub debug_visualization: bool,
    pub material_name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    /// Row-major, `(quads_x + 1) * (quads_z + 1)`, terrain-local Y.
    pub heights: Vec<f32>,
    pub layers: Vec<u8>,
    pub einstein: EinsteinTerrainDetail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StampResult {
    pub dirty: Vec<ChunkCoord>,
    pub height_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainHit {
    pub local_x: f32,
    pub local_z: f32,
    pub height: f32,
    pub slope: SlopeClass,
    pub layer: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainError {
    Grid,
    Range,
    EinsteinCollision,
}

impl TerrainRecord {
    pub fn flat(width_m: f32, depth_m: f32, spacing_m: f32, chunk_m: f32, height: f32) -> Result<Self, TerrainError> {
        let quads_x = whole_quads(width_m, spacing_m)?;
        let quads_z = whole_quads(depth_m, spacing_m)?;
        let chunk_quads = whole_quads(chunk_m, spacing_m)?;
        if quads_x % chunk_quads != 0 || quads_z % chunk_quads != 0 {
            return Err(TerrainError::Grid);
        }
        let count = (quads_x as usize + 1) * (quads_z as usize + 1);
        let record = Self {
            width_m,
            depth_m,
            spacing_m,
            chunk_m,
            height_min: TERRAIN_DEFAULT_HEIGHT_MIN,
            height_max: TERRAIN_DEFAULT_HEIGHT_MAX,
            collision: true,
            lod_enabled: false,
            debug_visualization: false,
            material_name: "standard_white".into(),
            base_color: [0.42, 0.48, 0.30, 1.0],
            metallic: 0.0,
            roughness: 0.92,
            heights: vec![height; count],
            layers: vec![0; count],
            einstein: EinsteinTerrainDetail::default(),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), TerrainError> {
        let quads_x = whole_quads(self.width_m, self.spacing_m)?;
        let quads_z = whole_quads(self.depth_m, self.spacing_m)?;
        let chunk_quads = whole_quads(self.chunk_m, self.spacing_m)?;
        if quads_x % chunk_quads != 0 || quads_z % chunk_quads != 0 {
            return Err(TerrainError::Grid);
        }
        if !self.height_min.is_finite() || !self.height_max.is_finite() || self.height_min >= self.height_max {
            return Err(TerrainError::Range);
        }
        let count = (quads_x as usize + 1) * (quads_z as usize + 1);
        if self.heights.len() != count || self.layers.len() != count || self.heights.iter().any(|height| !height.is_finite()) {
            return Err(TerrainError::Range);
        }
        if self.material_name.is_empty() || !self.metallic.is_finite() || !self.roughness.is_finite() {
            return Err(TerrainError::Range);
        }
        if self.base_color.iter().any(|channel| !channel.is_finite()) {
            return Err(TerrainError::Range);
        }
        let detail = &self.einstein;
        if detail.collision {
            return Err(TerrainError::EinsteinCollision);
        }
        if !detail.density.is_finite() || !detail.max_displacement_m.is_finite() || !detail.error_threshold_px.is_finite() || !detail.distance_m.is_finite() {
            return Err(TerrainError::Range);
        }
        Ok(())
    }

    pub fn quads_x(&self) -> u32 {
        whole_quads(self.width_m, self.spacing_m).unwrap_or(0)
    }

    pub fn quads_z(&self) -> u32 {
        whole_quads(self.depth_m, self.spacing_m).unwrap_or(0)
    }

    pub fn chunk_quads(&self) -> u32 {
        whole_quads(self.chunk_m, self.spacing_m).unwrap_or(0)
    }

    pub fn chunks_x(&self) -> u32 {
        let chunk = self.chunk_quads();
        if chunk == 0 { 0 } else { self.quads_x() / chunk }
    }

    pub fn chunks_z(&self) -> u32 {
        let chunk = self.chunk_quads();
        if chunk == 0 { 0 } else { self.quads_z() / chunk }
    }

    pub fn verts_x(&self) -> u32 {
        self.quads_x().saturating_add(1)
    }

    pub fn verts_z(&self) -> u32 {
        self.quads_z().saturating_add(1)
    }

    /// Bilinear terrain-local height. This is the collision and navigation sample.
    pub fn height_at(&self, local_x: f32, local_z: f32) -> Option<f32> {
        if !self.collision {
            return None;
        }
        self.sample_height(local_x, local_z)
    }

    /// Visual and authored height. Independent of the collision flag.
    pub fn sample_height(&self, local_x: f32, local_z: f32) -> Option<f32> {
        let (x0, z0, tx, tz) = self.cell(local_x, local_z)?;
        let h00 = self.sample(x0, z0)?;
        let h10 = self.sample(x0 + 1, z0)?;
        let h01 = self.sample(x0, z0 + 1)?;
        let h11 = self.sample(x0 + 1, z0 + 1)?;
        let x0 = h00 * (1.0 - tx) + h10 * tx;
        let x1 = h01 * (1.0 - tx) + h11 * tx;
        Some(x0 * (1.0 - tz) + x1 * tz)
    }

    pub fn slope_class_at(&self, local_x: f32, local_z: f32) -> SlopeClass {
        let Some((ix, iz, _, _)) = self.cell(local_x, local_z) else {
            return SlopeClass::Flat;
        };
        slope_of(self.gradient(ix, iz))
    }

    pub fn layer_at(&self, local_x: f32, local_z: f32) -> u8 {
        let Some((ix, iz)) = self.nearest(local_x, local_z) else {
            return 0;
        };
        self.layers.get(self.index(ix, iz)).copied().unwrap_or(0)
    }

    /// Deterministic lookup hook. Coordinate in, displacement out. No origin walk.
    /// Horizontal terrain uses XZ. Dominant-axis projection is reserved and not sampled.
    /// This foundation always returns `None`, so enabling detail cannot change the heightfield.
    pub fn detail_displacement(&self, local_x: f32, local_z: f32, level: u32) -> Option<f32> {
        let _ = (local_x, local_z, level, self.einstein.enabled, self.einstein.cliff);
        None
    }

    pub fn debug_line(&self, local_x: f32, local_z: f32) -> String {
        let height = self.sample_height(local_x, local_z).unwrap_or(0.0);
        format!(
            "terrain height={height:.3} slope={} layer={} einstein=not-generated",
            self.slope_class_at(local_x, local_z).label(),
            self.layer_at(local_x, local_z)
        )
    }

    pub fn stamp(
        &mut self,
        brush: TerrainBrush,
        local_x: f32,
        local_z: f32,
        radius_m: f32,
        delta_m: f32,
        layer: u8,
        falloff: TerrainFalloff,
        flatten_to: Option<f32>,
    ) -> Result<StampResult, TerrainError> {
        if !radius_m.is_finite() || radius_m <= 0.0 || !delta_m.is_finite() || !local_x.is_finite() || !local_z.is_finite() {
            return Err(TerrainError::Range);
        }
        if flatten_to.is_some_and(|height| !height.is_finite()) {
            return Err(TerrainError::Range);
        }
        let radius_sq = radius_m * radius_m;
        let target = flatten_to.unwrap_or_else(|| self.sample_height(local_x, local_z).unwrap_or(0.0));
        let original = if matches!(brush, TerrainBrush::Paint) { Vec::new() } else { self.heights.clone() };
        let mut dirty = Vec::new();
        let mut height_changed = false;
        let (ix0, ix1) = index_span(local_x, self.width_m, self.spacing_m, radius_m, self.quads_x());
        let (iz0, iz1) = index_span(local_z, self.depth_m, self.spacing_m, radius_m, self.quads_z());
        if ix0 <= ix1 && iz0 <= iz1 {
        for iz in iz0..=iz1 {
            for ix in ix0..=ix1 {
                let (x, z) = self.sample_position(ix, iz);
                let dx = x - local_x;
                let dz = z - local_z;
                let distance_sq = dx * dx + dz * dz;
                if distance_sq > radius_sq {
                    continue;
                }
                let weight = falloff_weight(falloff, distance_sq.sqrt(), radius_m);
                if weight <= 0.0 {
                    continue;
                }
                let index = self.index(ix, iz);
                match brush {
                    TerrainBrush::Sculpt => {
                        let next = (original[index] + delta_m * weight).clamp(self.height_min, self.height_max);
                        if next != self.heights[index] {
                            self.heights[index] = next;
                            height_changed = true;
                            self.push_chunks(ix, iz, &mut dirty);
                        }
                    }
                    TerrainBrush::Smooth => {
                        let average = self.neighbor_average(&original, ix, iz);
                        let next = (original[index] + (average - original[index]) * weight).clamp(self.height_min, self.height_max);
                        if (next - self.heights[index]).abs() > 1.0e-6 {
                            self.heights[index] = next;
                            height_changed = true;
                            self.push_chunks(ix, iz, &mut dirty);
                        }
                    }
                    TerrainBrush::Flatten => {
                        let next = (original[index] + (target - original[index]) * weight).clamp(self.height_min, self.height_max);
                        if (next - self.heights[index]).abs() > 1.0e-6 {
                            self.heights[index] = next;
                            height_changed = true;
                            self.push_chunks(ix, iz, &mut dirty);
                        }
                    }
                    TerrainBrush::Paint => {
                        self.layers[index] = layer;
                    }
                }
            }
        }
        }
        Ok(StampResult { dirty, height_changed })
    }

    /// One chunk in terrain-local space. Shared edges read the same samples, so their normals match.
    pub fn chunk_mesh(&self, cx: u32, cz: u32) -> Result<Mesh, MeshError> {
        let chunk_quads = self.chunk_quads();
        if cx >= self.chunks_x() || cz >= self.chunks_z() || chunk_quads == 0 {
            return Err(MeshError::Empty);
        }
        let x0 = cx * chunk_quads;
        let z0 = cz * chunk_quads;
        let row = chunk_quads + 1;
        let mut bytes = Vec::new();
        let color = [1.0_f32, 1.0, 1.0];
        for iz in 0..row {
            for ix in 0..row {
                let sample_x = x0 + ix;
                let sample_z = z0 + iz;
                let (x, z) = self.sample_position(sample_x, sample_z);
                let y = self.sample(sample_x, sample_z).unwrap_or(0.0);
                let normal = self.normal_at(sample_x, sample_z);
                let u = (x + self.width_m * 0.5) / self.width_m;
                let v = (z + self.depth_m * 0.5) / self.depth_m;
                push_vertex(&mut bytes, [x, y, z], color, [u, v], normal, [1.0, 0.0, 0.0, 1.0]);
            }
        }
        let vert_count = (row * row) as u32;
        let tri_count = chunk_quads * chunk_quads * 2;
        let mut indices = Vec::new();
        for iz in 0..chunk_quads {
            for ix in 0..chunk_quads {
                let i00 = iz * row + ix;
                let i10 = i00 + 1;
                let i01 = i00 + row;
                let i11 = i01 + 1;
                // Same winding as floor_mesh: (0, 2, 1) then (0, 3, 2) with +Y up.
                push_index(&mut indices, vert_count, i00);
                push_index(&mut indices, vert_count, i11);
                push_index(&mut indices, vert_count, i10);
                push_index(&mut indices, vert_count, i00);
                push_index(&mut indices, vert_count, i01);
                push_index(&mut indices, vert_count, i11);
            }
        }
        let _ = tri_count;
        create_mesh(MeshDesc {
            streams: vec![VertexStreamDesc {
                stride: 60,
                attributes: vec![
                    MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                    MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                    MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                    MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                    MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
                ],
                bytes,
            }],
            index_format: if vert_count > u16::MAX as u32 { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 },
            index_bytes: indices,
            submeshes: vec![SubmeshDesc {
                first_index: 0,
                index_count: chunk_quads * chunk_quads * 6,
                base_vertex: 0,
                topology: crate::MeshTopology::TriangleList,
                material_slot: 0,
            }],
        })
    }

    /// Terrain-local ray. A horizontal ray above a flat field misses. A downward ray hits.
    pub fn ray_heightfield(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<TerrainHit> {
        let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
        if length < 1.0e-8 {
            return None;
        }
        let dir = [direction[0] / length, direction[1] / length, direction[2] / length];
        if dir[0].abs() < 1.0e-6 && dir[2].abs() < 1.0e-6 {
            let height = self.sample_height(origin[0], origin[2])?;
            if dir[1].abs() < 1.0e-6 {
                return None;
            }
            let t = (height - origin[1]) / dir[1];
            if t < 0.0 {
                return None;
            }
            return Some(self.hit(origin[0], origin[2], height));
        }
        let half_x = self.width_m * 0.5;
        let half_z = self.depth_m * 0.5;
        let mut tmin = 0.0_f32;
        let mut tmax = 1.0e7_f32;
        if !slab(origin[0], dir[0], -half_x, half_x, &mut tmin, &mut tmax) || !slab(origin[2], dir[2], -half_z, half_z, &mut tmin, &mut tmax) {
            return None;
        }
        if tmax < tmin {
            return None;
        }
        let step = (self.spacing_m * 0.25).max(0.05);
        let mut t = tmin;
        let mut guard = 0u32;
        while t <= tmax && guard < 100_000 {
            guard += 1;
            let x = origin[0] + dir[0] * t;
            let y = origin[1] + dir[1] * t;
            let z = origin[2] + dir[2] * t;
            if let Some(height) = self.sample_height(x, z) {
                if y <= height + 1.0e-3 {
                    return Some(self.hit(x, z, height));
                }
            }
            if t >= tmax {
                break;
            }
            t = (t + step).min(tmax);
        }
        None
    }

    fn hit(&self, local_x: f32, local_z: f32, height: f32) -> TerrainHit {
        TerrainHit {
            local_x,
            local_z,
            height,
            slope: self.slope_class_at(local_x, local_z),
            layer: self.layer_at(local_x, local_z),
        }
    }

    fn cell(&self, local_x: f32, local_z: f32) -> Option<(u32, u32, f32, f32)> {
        if self.spacing_m <= 0.0 || self.quads_x() == 0 {
            return None;
        }
        let fx = (local_x + self.width_m * 0.5) / self.spacing_m;
        let fz = (local_z + self.depth_m * 0.5) / self.spacing_m;
        if !fx.is_finite() || !fz.is_finite() || fx < 0.0 || fz < 0.0 || fx > self.quads_x() as f32 || fz > self.quads_z() as f32 {
            return None;
        }
        let x0 = (fx.floor() as u32).min(self.quads_x().saturating_sub(1));
        let z0 = (fz.floor() as u32).min(self.quads_z().saturating_sub(1));
        let tx = (fx - x0 as f32).clamp(0.0, 1.0);
        let tz = (fz - z0 as f32).clamp(0.0, 1.0);
        Some((x0, z0, tx, tz))
    }

    fn nearest(&self, local_x: f32, local_z: f32) -> Option<(u32, u32)> {
        let (x0, z0, tx, tz) = self.cell(local_x, local_z)?;
        let ix = x0 + u32::from(tx >= 0.5);
        let iz = z0 + u32::from(tz >= 0.5);
        Some((ix.min(self.quads_x()), iz.min(self.quads_z())))
    }

    fn sample(&self, ix: u32, iz: u32) -> Option<f32> {
        if ix > self.quads_x() || iz > self.quads_z() {
            return None;
        }
        self.heights.get(self.index(ix, iz)).copied()
    }

    fn index(&self, ix: u32, iz: u32) -> usize {
        iz as usize * self.verts_x() as usize + ix as usize
    }

    fn sample_position(&self, ix: u32, iz: u32) -> (f32, f32) {
        (-self.width_m * 0.5 + ix as f32 * self.spacing_m, -self.depth_m * 0.5 + iz as f32 * self.spacing_m)
    }

    fn gradient(&self, ix: u32, iz: u32) -> f32 {
        let left = self.sample(ix.saturating_sub(1), iz).unwrap_or(0.0);
        let right = self.sample(ix + 1, iz).or_else(|| self.sample(ix, iz)).unwrap_or(0.0);
        let down = self.sample(ix, iz.saturating_sub(1)).unwrap_or(0.0);
        let up = self.sample(ix, iz + 1).or_else(|| self.sample(ix, iz)).unwrap_or(0.0);
        let span_x = if ix == 0 || ix >= self.quads_x() { self.spacing_m } else { self.spacing_m * 2.0 };
        let span_z = if iz == 0 || iz >= self.quads_z() { self.spacing_m } else { self.spacing_m * 2.0 };
        let dx = (right - left) / span_x.max(1.0e-6);
        let dz = (up - down) / span_z.max(1.0e-6);
        (dx * dx + dz * dz).sqrt()
    }

    fn normal_at(&self, ix: u32, iz: u32) -> [f32; 3] {
        let left = self.sample(ix.saturating_sub(1), iz).unwrap_or(0.0);
        let right = self.sample(ix + 1, iz).or_else(|| self.sample(ix, iz)).unwrap_or(0.0);
        let down = self.sample(ix, iz.saturating_sub(1)).unwrap_or(0.0);
        let up = self.sample(ix, iz + 1).or_else(|| self.sample(ix, iz)).unwrap_or(0.0);
        let span_x = if ix == 0 || ix >= self.quads_x() { self.spacing_m } else { self.spacing_m * 2.0 };
        let span_z = if iz == 0 || iz >= self.quads_z() { self.spacing_m } else { self.spacing_m * 2.0 };
        let dhdx = (right - left) / span_x.max(1.0e-6);
        let dhdz = (up - down) / span_z.max(1.0e-6);
        let normal = [-dhdx, 1.0, -dhdz];
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length < 1.0e-8 { [0.0, 1.0, 0.0] } else { [normal[0] / length, normal[1] / length, normal[2] / length] }
    }

    fn neighbor_average(&self, heights: &[f32], ix: u32, iz: u32) -> f32 {
        let mut sum = 0.0;
        let mut count = 0.0;
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let x = ix as i32 + dx;
                let z = iz as i32 + dz;
                if x < 0 || z < 0 || x > self.quads_x() as i32 || z > self.quads_z() as i32 {
                    continue;
                }
                let index = z as usize * self.verts_x() as usize + x as usize;
                if let Some(height) = heights.get(index) {
                    sum += *height;
                    count += 1.0;
                }
            }
        }
        if count == 0.0 { heights[self.index(ix, iz)] } else { sum / count }
    }

    fn push_chunks(&self, ix: u32, iz: u32, dirty: &mut Vec<ChunkCoord>) {
        let chunk = self.chunk_quads().max(1);
        for x in chunk_ids(ix, chunk, self.chunks_x()) {
            for z in chunk_ids(iz, chunk, self.chunks_z()) {
                if !dirty.iter().any(|coord| coord.x == x && coord.z == z) {
                    dirty.push(ChunkCoord { x, z });
                }
            }
        }
    }
}

pub fn apply_terrain_property(record: &TerrainRecord, field: FieldId, value: PropertyValue) -> Result<TerrainRecord, AuthoringError> {
    let mut next = record.clone();
    match (field, value) {
        (FIELD_TERRAIN_WIDTH | FIELD_TERRAIN_DEPTH | FIELD_TERRAIN_SPACING | FIELD_TERRAIN_CHUNK | FIELD_TERRAIN_HEIGHT | FIELD_TERRAIN_MATERIAL | FIELD_TERRAIN_EINSTEIN_COLLISION, _) => {
            return Err(AuthoringError::ReadOnly);
        }
        (FIELD_TERRAIN_HEIGHT_MIN, PropertyValue::F64(meters)) => next.height_min = meters as f32,
        (FIELD_TERRAIN_HEIGHT_MAX, PropertyValue::F64(meters)) => next.height_max = meters as f32,
        (FIELD_TERRAIN_COLLISION, PropertyValue::Bool(enabled)) => next.collision = enabled,
        (FIELD_TERRAIN_LOD, PropertyValue::Bool(enabled)) => next.lod_enabled = enabled,
        (FIELD_TERRAIN_DEBUG, PropertyValue::Bool(enabled)) => next.debug_visualization = enabled,
        (FIELD_TERRAIN_EINSTEIN, PropertyValue::Bool(enabled)) => next.einstein.enabled = enabled,
        (FIELD_TERRAIN_SEED, PropertyValue::F64(seed)) => {
            if !seed.is_finite() || seed < 0.0 || seed > u32::MAX as f64 {
                return Err(AuthoringError::InvalidValue);
            }
            next.einstein.seed = seed as u32;
        }
        (FIELD_TERRAIN_DENSITY, PropertyValue::F64(density)) => next.einstein.density = density as f32,
        (FIELD_TERRAIN_DISPLACEMENT, PropertyValue::F64(meters)) => next.einstein.max_displacement_m = meters as f32,
        (FIELD_TERRAIN_ERROR, PropertyValue::F64(pixels)) => next.einstein.error_threshold_px = pixels as f32,
        (FIELD_TERRAIN_DISTANCE, PropertyValue::F64(meters)) => next.einstein.distance_m = meters as f32,
        (FIELD_TERRAIN_CLASS, PropertyValue::F64(class)) => {
            if !class.is_finite() || class < 0.0 || class > 255.0 {
                return Err(AuthoringError::InvalidValue);
            }
            next.einstein.surface_class = class as u8;
        }
        (FIELD_TERRAIN_CLIFF, PropertyValue::String(label)) => {
            next.einstein.cliff = CliffProjection::parse(&label).ok_or(AuthoringError::InvalidValue)?;
        }
        (FIELD_TERRAIN_DEBUG_COLORS, PropertyValue::Bool(enabled)) => next.einstein.debug_colors = enabled,
        _ => return Err(AuthoringError::WrongType),
    }
    next.validate().map_err(|_| AuthoringError::InvalidValue)?;
    Ok(next)
}

pub fn encode_f32_base64(values: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    encode_base64(&bytes)
}

pub fn decode_f32_base64(text: &str) -> Result<Vec<f32>, TerrainError> {
    let bytes = decode_base64(text)?;
    if bytes.len() % 4 != 0 {
        return Err(TerrainError::Range);
    }
    Ok(bytes.chunks_exact(4).map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])).collect())
}

pub fn encode_u8_base64(values: &[u8]) -> String {
    encode_base64(values)
}

pub fn decode_u8_base64(text: &str) -> Result<Vec<u8>, TerrainError> {
    decode_base64(text)
}

fn whole_quads(extent: f32, spacing: f32) -> Result<u32, TerrainError> {
    if !extent.is_finite() || !spacing.is_finite() || extent <= 0.0 || spacing <= 0.0 {
        return Err(TerrainError::Grid);
    }
    let quads = extent / spacing;
    let rounded = quads.round();
    if (quads - rounded).abs() > 1.0e-3 || rounded < 1.0 || rounded > TERRAIN_MAX_QUADS as f32 {
        return Err(TerrainError::Grid);
    }
    Ok(rounded as u32)
}

fn slope_of(gradient: f32) -> SlopeClass {
    if gradient < SLOPE_FLAT_MAX {
        SlopeClass::Flat
    } else if gradient < SLOPE_SLOPE_MAX {
        SlopeClass::Slope
    } else {
        SlopeClass::Cliff
    }
}

fn falloff_weight(falloff: TerrainFalloff, distance: f32, radius: f32) -> f32 {
    if distance >= radius || radius <= 0.0 {
        0.0
    } else {
        match falloff {
            TerrainFalloff::Smooth => 0.5 * (1.0 + (distance / radius * std::f32::consts::PI).cos()),
            TerrainFalloff::Linear => 1.0 - distance / radius,
        }
    }
}

fn index_span(local: f32, extent: f32, spacing: f32, radius: f32, quads: u32) -> (u32, u32) {
    if !local.is_finite() || spacing <= 0.0 || quads == 0 {
        return (1, 0);
    }
    let half = extent * 0.5;
    let min = ((local - radius + half) / spacing).floor();
    let max = ((local + radius + half) / spacing).ceil();
    if !min.is_finite() || !max.is_finite() {
        return (1, 0);
    }
    let min = min.max(0.0) as u32;
    let max = (max.max(0.0) as u32).min(quads);
    if min > max { (1, 0) } else { (min, max) }
}

fn chunk_ids(sample: u32, chunk_quads: u32, chunks: u32) -> Vec<u32> {
    if chunks == 0 || chunk_quads == 0 {
        return Vec::new();
    }
    if sample >= chunk_quads * chunks {
        return vec![chunks - 1];
    }
    let id = sample / chunk_quads;
    let mut ids = vec![id.min(chunks - 1)];
    if sample > 0 && sample % chunk_quads == 0 && id > 0 {
        ids.push(id - 1);
    }
    ids
}

fn slab(origin: f32, direction: f32, min: f32, max: f32, tmin: &mut f32, tmax: &mut f32) -> bool {
    if direction.abs() < 1.0e-8 {
        return origin >= min && origin <= max;
    }
    let inv = 1.0 / direction;
    let mut t0 = (min - origin) * inv;
    let mut t1 = (max - origin) * inv;
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    *tmin = tmin.max(t0);
    *tmax = tmax.min(t1);
    *tmin <= *tmax
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], color: [f32; 3], uv: [f32; 2], normal: [f32; 3], tangent: [f32; 4]) {
    for value in position.iter().chain(color.iter()).chain(uv.iter()).chain(normal.iter()).chain(tangent.iter()) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_index(bytes: &mut Vec<u8>, vert_count: u32, index: u32) {
    if vert_count > u16::MAX as u32 {
        bytes.extend_from_slice(&index.to_le_bytes());
    } else {
        bytes.extend_from_slice(&(index as u16).to_le_bytes());
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let packed = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8) | bytes[index + 2] as u32;
        out.push(ALPHABET[((packed >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((packed >> 12) & 63) as usize] as char);
        out.push(ALPHABET[((packed >> 6) & 63) as usize] as char);
        out.push(ALPHABET[(packed & 63) as usize] as char);
        index += 3;
    }
    if index < bytes.len() {
        let remain = bytes.len() - index;
        let mut packed = (bytes[index] as u32) << 16;
        if remain == 2 {
            packed |= (bytes[index + 1] as u32) << 8;
        }
        out.push(ALPHABET[((packed >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((packed >> 12) & 63) as usize] as char);
        if remain == 2 {
            out.push(ALPHABET[((packed >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

fn decode_base64(text: &str) -> Result<Vec<u8>, TerrainError> {
    fn value(byte: u8) -> Result<u8, TerrainError> {
        match byte {
            b'A'..=b'Z' => Ok(byte - b'A'),
            b'a'..=b'z' => Ok(byte - b'a' + 26),
            b'0'..=b'9' => Ok(byte - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(TerrainError::Range),
        }
    }
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(TerrainError::Range);
    }
    let mut out = Vec::new();
    for chunk in bytes.chunks_exact(4) {
        let padded = chunk.iter().filter(|byte| **byte == b'=').count();
        if padded > 2 {
            return Err(TerrainError::Range);
        }
        let mut sextets = [0u8; 4];
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                sextets[index] = 0;
            } else {
                sextets[index] = value(*byte)?;
            }
        }
        let packed = ((sextets[0] as u32) << 18) | ((sextets[1] as u32) << 12) | ((sextets[2] as u32) << 6) | sextets[3] as u32;
        out.push((packed >> 16) as u8);
        if padded < 2 {
            out.push((packed >> 8) as u8);
        }
        if padded == 0 {
            out.push(packed as u8);
        }
    }
    Ok(out)
}

/// Editor overlay budget. The grid is not a mesh and is not stored in the level.
pub const TERRAIN_OVERLAY_BUDGET: usize = 2500;

/// Fractional part of `camera_axis / spacing`. The camera axis stays binary64, including the billion-meter root.
pub fn world_grid_phase(camera_axis: f64, spacing: f32) -> f32 {
    if !camera_axis.is_finite() || !spacing.is_finite() || spacing <= 0.0 {
        return 0.0;
    }
    let scaled = camera_axis / f64::from(spacing);
    let fraction = scaled - scaled.floor();
    if fraction.is_finite() { fraction as f32 } else { 0.0 }
}

/// World-grid coordinate in spacing units. `camera_relative` is the camera-relative float32 axis.
pub fn world_grid_coord(camera_relative: f32, camera_axis: f64, spacing: f32) -> f32 {
    if !camera_relative.is_finite() || !spacing.is_finite() || spacing <= 0.0 {
        return 0.0;
    }
    camera_relative / spacing + world_grid_phase(camera_axis, spacing)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainOverlayMask {
    pub world: bool,
    pub vertices: bool,
    pub chunks: bool,
    pub lod: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainOverlayLine {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct BuiltTerrainChunk {
    pub coord: ChunkCoord,
    pub mesh: Mesh,
}

const WORLD_MINOR: [f32; 4] = [0.72, 0.78, 0.70, 0.55];
const WORLD_MAJOR: [f32; 4] = [0.93, 0.95, 0.88, 0.92];
const VERTEX_COLOR: [f32; 4] = [0.45, 0.78, 0.95, 0.75];
const CHUNK_COLOR: [f32; 4] = [0.95, 0.62, 0.22, 0.95];

/// Surface lines for the editor. Sampling the heightfield does not write it.
pub fn terrain_overlay_lines(
    record: &TerrainRecord,
    mask: TerrainOverlayMask,
    minor_m: f32,
    major_m: f32,
    follow: bool,
    focus_x: f32,
    focus_z: f32,
) -> Vec<TerrainOverlayLine> {
    let mut lines = Vec::new();
    let minor = if minor_m.is_finite() && minor_m > 0.0 { minor_m } else { record.spacing_m.max(0.05) };
    let major = if major_m.is_finite() && major_m >= minor { major_m } else { (minor * 10.0).max(minor) };
    if mask.world {
        push_world_grid(&mut lines, record, minor, major, follow, focus_x, focus_z);
    }
    if mask.vertices {
        let half = 24.0_f32;
        push_grid_lines(
            &mut lines,
            record,
            (focus_x - half).max(-record.width_m * 0.5),
            (focus_x + half).min(record.width_m * 0.5),
            (focus_z - half).max(-record.depth_m * 0.5),
            (focus_z + half).min(record.depth_m * 0.5),
            record.spacing_m.max(0.05),
            follow,
            VERTEX_COLOR,
            4,
            None,
        );
    }
    if mask.chunks {
        push_chunk_bounds(&mut lines, record, follow);
    }
    if mask.lod {
        push_lod_marks(&mut lines, record, follow);
    }
    lines.truncate(TERRAIN_OVERLAY_BUDGET);
    lines
}

/// Circle on the heightfield. Points outside the patch clamp to the edge.
pub fn brush_ring(record: &TerrainRecord, local_x: f32, local_z: f32, radius_m: f32, segments: u32) -> Vec<[f32; 3]> {
    let segments = segments.clamp(8, 64);
    if !radius_m.is_finite() || radius_m <= 0.0 || !local_x.is_finite() || !local_z.is_finite() {
        return Vec::new();
    }
    let half_x = record.width_m * 0.5;
    let half_z = record.depth_m * 0.5;
    let mut points = Vec::with_capacity(segments as usize);
    for step in 0..segments {
        let angle = step as f32 / segments as f32 * std::f32::consts::TAU;
        let x = (local_x + angle.cos() * radius_m).clamp(-half_x, half_x);
        let z = (local_z + angle.sin() * radius_m).clamp(-half_z, half_z);
        let y = record.sample_height(x, z).unwrap_or(0.0);
        points.push([x, y, z]);
    }
    points
}

/// Snap a terrain-local point onto a spacing lattice measured from the terrain edge.
pub fn snap_terrain_xz(width_m: f32, depth_m: f32, x: f32, z: f32, spacing: f32) -> (f32, f32) {
    if !spacing.is_finite() || spacing <= 0.0 {
        return (x, z);
    }
    let snap = |value: f32, half: f32| {
        let snapped = ((value + half) / spacing).round() * spacing - half;
        if snapped.is_finite() { snapped.clamp(-half, half) } else { value }
    };
    (snap(x, width_m * 0.5), snap(z, depth_m * 0.5))
}

/// Chunk meshes for the dirty set only. Callers publish them on the frame thread.
pub fn build_dirty_chunks(record: &TerrainRecord, dirty: &[ChunkCoord]) -> Result<Vec<BuiltTerrainChunk>, MeshError> {
    let mut built = Vec::with_capacity(dirty.len());
    for coord in dirty {
        if coord.x >= record.chunks_x() || coord.z >= record.chunks_z() {
            continue;
        }
        built.push(BuiltTerrainChunk { coord: *coord, mesh: record.chunk_mesh(coord.x, coord.z)? });
    }
    Ok(built)
}

fn push_world_grid(lines: &mut Vec<TerrainOverlayLine>, record: &TerrainRecord, minor: f32, major: f32, follow: bool, focus_x: f32, focus_z: f32) {
    let half_x = record.width_m * 0.5;
    let half_z = record.depth_m * 0.5;
    push_grid_lines(lines, record, -half_x, half_x, -half_z, half_z, major, follow, WORLD_MAJOR, 8, None);
    let window = (major * 4.0).clamp(16.0, 48.0);
    push_grid_lines(
        lines,
        record,
        (focus_x - window).max(-half_x),
        (focus_x + window).min(half_x),
        (focus_z - window).max(-half_z),
        (focus_z + window).min(half_z),
        minor,
        follow,
        WORLD_MINOR,
        4,
        Some(major),
    );
}

fn push_grid_lines(
    lines: &mut Vec<TerrainOverlayLine>,
    record: &TerrainRecord,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    step: f32,
    follow: bool,
    color: [f32; 4],
    subdivisions: u32,
    skip_step: Option<f32>,
) {
    if step <= 0.0 || x1 < x0 || z1 < z0 || lines.len() >= TERRAIN_OVERLAY_BUDGET {
        return;
    }
    let half_x = record.width_m * 0.5;
    let half_z = record.depth_m * 0.5;
    let mut x = lattice_start(x0, half_x, step);
    while x <= x1 + step * 1.0e-3 {
        if skip_step.is_none_or(|major| !on_lattice(x, half_x, major)) {
            push_run(lines, record, x, z0, z1, true, follow, color, subdivisions);
        }
        x += step;
        if lines.len() >= TERRAIN_OVERLAY_BUDGET {
            return;
        }
    }
    let mut z = lattice_start(z0, half_z, step);
    while z <= z1 + step * 1.0e-3 {
        if skip_step.is_none_or(|major| !on_lattice(z, half_z, major)) {
            push_run(lines, record, z, x0, x1, false, follow, color, subdivisions);
        }
        z += step;
        if lines.len() >= TERRAIN_OVERLAY_BUDGET {
            return;
        }
    }
}

fn push_chunk_bounds(lines: &mut Vec<TerrainOverlayLine>, record: &TerrainRecord, follow: bool) {
    let half_x = record.width_m * 0.5;
    let half_z = record.depth_m * 0.5;
    for index in 0..=record.chunks_x() {
        let x = -half_x + index as f32 * record.chunk_m;
        push_run(lines, record, x, -half_z, half_z, true, follow, CHUNK_COLOR, 4);
    }
    for index in 0..=record.chunks_z() {
        let z = -half_z + index as f32 * record.chunk_m;
        push_run(lines, record, z, -half_x, half_x, false, follow, CHUNK_COLOR, 4);
    }
}

fn push_lod_marks(lines: &mut Vec<TerrainOverlayLine>, record: &TerrainRecord, follow: bool) {
    let alpha = if record.lod_enabled { 0.95 } else { 0.4 };
    let color = [0.62, 0.48, 0.95, alpha];
    let half_x = record.width_m * 0.5;
    let half_z = record.depth_m * 0.5;
    let arm = (record.chunk_m * 0.12).clamp(0.25, 4.0);
    for cz in 0..record.chunks_z() {
        for cx in 0..record.chunks_x() {
            if lines.len() + 2 > TERRAIN_OVERLAY_BUDGET {
                return;
            }
            let x = -half_x + (cx as f32 + 0.5) * record.chunk_m;
            let z = -half_z + (cz as f32 + 0.5) * record.chunk_m;
            push_run(lines, record, z, x - arm, x + arm, false, follow, color, 1);
            push_run(lines, record, x, z - arm, z + arm, true, follow, color, 1);
        }
    }
}

fn push_run(
    lines: &mut Vec<TerrainOverlayLine>,
    record: &TerrainRecord,
    fixed: f32,
    along0: f32,
    along1: f32,
    constant_x: bool,
    follow: bool,
    color: [f32; 4],
    subdivisions: u32,
) {
    let segments = if follow { subdivisions.max(1) } else { 1 };
    let at = |along: f32| {
        let (x, z) = if constant_x { (fixed, along) } else { (along, fixed) };
        let y = if follow { record.sample_height(x, z).unwrap_or(0.0) } else { 0.0 };
        [x, y, z]
    };
    let mut previous = at(along0);
    for step in 1..=segments {
        if lines.len() >= TERRAIN_OVERLAY_BUDGET {
            return;
        }
        let along = along0 + (along1 - along0) * (step as f32 / segments as f32);
        let next = at(along);
        lines.push(TerrainOverlayLine { from: previous, to: next, color });
        previous = next;
    }
}

fn lattice_start(min: f32, half: f32, step: f32) -> f32 {
    let origin = -half;
    let n = ((min - origin) / step).ceil();
    origin + n * step
}

fn on_lattice(value: f32, half: f32, step: f32) -> bool {
    if step <= 0.0 {
        return false;
    }
    let n = (value + half) / step;
    (n - n.round()).abs() < 1.0e-3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SHADOW_CASTER_TRIANGLE_LIMIT;

    #[test]
    fn flat_grid_rejects_a_size_that_does_not_divide() {
        assert!(TerrainRecord::flat(512.0, 512.0, 1.0, 64.0, 0.0).is_ok());
        assert!(TerrainRecord::flat(500.0, 512.0, 1.0, 64.0, 0.0).is_err());
        assert!(TerrainRecord::flat(4096.0, 512.0, 1.0, 64.0, 0.0).is_err());
    }

    #[test]
    fn a_default_chunk_is_8192_triangles_and_under_the_shadow_cap() {
        let terrain = TerrainRecord::flat(512.0, 512.0, 1.0, 64.0, 0.0).unwrap();
        assert_eq!(terrain.chunks_x(), 8);
        let mesh = terrain.chunk_mesh(0, 0).unwrap();
        let triangles = mesh.index_count() / 3;
        assert_eq!(triangles, 8192);
        assert!(triangles <= SHADOW_CASTER_TRIANGLE_LIMIT);
        let flat = mesh.position(0).unwrap();
        assert!((flat[1]).abs() < 1.0e-6);
    }

    #[test]
    fn height_bits_round_trip_through_base64() {
        let mut terrain = TerrainRecord::flat(4.0, 4.0, 1.0, 2.0, 0.0).unwrap();
        terrain.heights[3] = 1.25;
        terrain.heights[7] = -0.5;
        let text = encode_f32_base64(&terrain.heights);
        assert!(!text.contains('"'));
        let decoded = decode_f32_base64(&text).unwrap();
        assert_eq!(decoded.len(), terrain.heights.len());
        for (left, right) in terrain.heights.iter().zip(decoded.iter()) {
            assert_eq!(left.to_bits(), right.to_bits());
        }
    }

    #[test]
    fn world_grid_stays_on_integer_lines_beside_the_billion_meter_root() {
        let camera = 1.0e9 + 0.25;
        let on_line = world_grid_coord(-0.25, camera, 1.0);
        assert!(on_line.abs() < 1.0e-4, "{on_line}");
        let three = world_grid_coord(2.75, camera, 1.0);
        assert!((three - 3.0).abs() < 1.0e-4, "{three}");
        let negative = world_grid_phase(-1.25, 1.0);
        assert!((negative - 0.75).abs() < 1.0e-5, "{negative}");
    }

    #[test]
    fn sculpt_raises_the_center_and_dirties_the_shared_edge() {
        let mut terrain = TerrainRecord::flat(4.0, 4.0, 1.0, 2.0, 0.0).unwrap();
        let before = terrain.heights.clone();
        let stamped = terrain.stamp(TerrainBrush::Sculpt, 0.0, 0.0, 1.5, 0.35, 0, TerrainFalloff::Smooth, None).unwrap();
        assert!(stamped.height_changed);
        assert!(terrain.sample_height(0.0, 0.0).unwrap() > 0.0);
        assert!(stamped.dirty.iter().any(|coord| coord.x == 0));
        assert!(stamped.dirty.iter().any(|coord| coord.x == 1));
        terrain.einstein.enabled = true;
        assert!(terrain.detail_displacement(0.0, 0.0, 1).is_none());
        assert_eq!(terrain.heights[0].to_bits(), before[0].to_bits());
        terrain.einstein.collision = true;
        assert_eq!(terrain.validate(), Err(TerrainError::EinsteinCollision));
    }

    #[test]
    fn a_horizontal_ray_misses_a_flat_field_and_a_downward_ray_hits() {
        let terrain = TerrainRecord::flat(8.0, 8.0, 1.0, 4.0, 0.0).unwrap();
        assert!(terrain.ray_heightfield([0.0, 1.6, 8.0], [0.0, 0.0, -1.0]).is_none());
        let hit = terrain.ray_heightfield([0.0, 10.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
        assert!(hit.height.abs() < 1.0e-3);
        assert_eq!(hit.slope, SlopeClass::Flat);
    }

    #[test]
    fn enabling_einstein_does_not_change_height_bits() {
        let mut terrain = TerrainRecord::flat(4.0, 4.0, 1.0, 2.0, 0.0).unwrap();
        let bits: Vec<u32> = terrain.heights.iter().map(|height| height.to_bits()).collect();
        let changed = apply_terrain_property(&terrain, FIELD_TERRAIN_EINSTEIN, PropertyValue::Bool(true)).unwrap();
        assert!(changed.einstein.enabled);
        assert!(changed.heights.iter().zip(bits.iter()).all(|(height, bits)| height.to_bits() == *bits));
        assert!(apply_terrain_property(&terrain, FIELD_TERRAIN_EINSTEIN_COLLISION, PropertyValue::Bool(true)).is_err());
        assert!(apply_terrain_property(&terrain, FIELD_TERRAIN_WIDTH, PropertyValue::F64(8.0)).is_err());
        terrain.heights[0] = 3.0;
        assert_eq!(terrain.heights[0].to_bits(), 3.0f32.to_bits());
    }

    #[test]
    fn sculpt_lowers_smooths_flattens_and_clamps_without_touching_a_far_chunk() {
        let mut terrain = TerrainRecord::flat(8.0, 8.0, 1.0, 4.0, 0.0).unwrap();
        let lowered = terrain.stamp(TerrainBrush::Sculpt, -3.0, -3.0, 1.2, -0.5, 0, TerrainFalloff::Smooth, None).unwrap();
        let carved = terrain.sample_height(-3.0, -3.0).unwrap();
        assert!(carved < 0.0);
        assert!(lowered.dirty.iter().all(|coord| coord.x == 0 && coord.z == 0));
        let bits: Vec<u32> = terrain.heights.iter().map(|height| height.to_bits()).collect();
        let mask = TerrainOverlayMask { world: true, vertices: true, chunks: true, lod: true };
        let lines = terrain_overlay_lines(&terrain, mask, 1.0, 4.0, true, -3.0, -3.0);
        assert!(!lines.is_empty());
        assert!(lines.len() <= TERRAIN_OVERLAY_BUDGET);
        assert!(lines.iter().any(|line| line.from[1] < 0.0 || line.to[1] < 0.0));
        assert_eq!(terrain.heights.iter().map(|height| height.to_bits()).collect::<Vec<_>>(), bits);
        let flat = terrain_overlay_lines(&terrain, TerrainOverlayMask { world: true, vertices: false, chunks: false, lod: false }, 1.0, 4.0, false, 0.0, 0.0);
        assert!(flat.iter().all(|line| line.from[1].abs() < 1.0e-6 && line.to[1].abs() < 1.0e-6));
        let center = terrain.index(1, 1);
        terrain.heights[center] = 2.0;
        terrain.stamp(TerrainBrush::Smooth, -3.0, -3.0, 1.2, 0.35, 0, TerrainFalloff::Smooth, None).unwrap();
        let smoothed = terrain.heights[center];
        assert!(smoothed < 2.0);
        terrain.stamp(TerrainBrush::Flatten, -3.0, -3.0, 1.2, 0.35, 0, TerrainFalloff::Linear, Some(0.0)).unwrap();
        assert!(terrain.heights[center].abs() < smoothed.abs());
        terrain.stamp(TerrainBrush::Sculpt, -3.0, -3.0, 1.0, 10_000.0, 0, TerrainFalloff::Linear, None).unwrap();
        assert!(terrain.sample_height(-3.0, -3.0).unwrap() <= terrain.height_max);
        let smooth = falloff_weight(TerrainFalloff::Smooth, 0.375, 1.5);
        let linear = falloff_weight(TerrainFalloff::Linear, 0.375, 1.5);
        assert!((smooth - linear).abs() > 0.05);
        let (sx, sz) = snap_terrain_xz(8.0, 8.0, -2.6, 1.2, 1.0);
        assert!((sx + 3.0).abs() < 1.0e-4);
        assert!((sz - 1.0).abs() < 1.0e-4);
        let ring = brush_ring(&terrain, -3.0, -3.0, 1.2, 16);
        assert_eq!(ring.len(), 16);
        assert!(ring.iter().all(|point| (point[1] - terrain.sample_height(point[0], point[2]).unwrap_or(0.0)).abs() < 1.0e-4));
        let built = build_dirty_chunks(&terrain, &lowered.dirty).unwrap();
        assert_eq!(built.len(), 1);
        assert_eq!((built[0].coord.x, built[0].coord.z), (0, 0));
        assert!(build_dirty_chunks(&terrain, &[ChunkCoord { x: 7, z: 7 }]).unwrap().is_empty());
    }
}
