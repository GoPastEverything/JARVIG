//! Reference frames and the camera-relative render packet.
//!
//! This is the native form of the contract in `@jarvig/math` and `FrameGraph`.
//! It is not a second world. Positions are meters. Frame translation and
//! rotation are binary64. A [`Mat4`] exists only as the float32 GPU upload
//! described in ADR-0020. It is not a world transform.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn scale(self, scalar: f64) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Quat {
    pub const IDENTITY: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn from_axis_angle(axis: Vec3, radians: f64) -> Result<Self, SpaceError> {
        let length = (axis.x * axis.x + axis.y * axis.y + axis.z * axis.z).sqrt();
        if length == 0.0 {
            return Err(SpaceError::BadRotation);
        }
        let half = radians * 0.5;
        let scale = half.sin() / length;
        Ok(Self {
            x: axis.x * scale,
            y: axis.y * scale,
            z: axis.z * scale,
            w: half.cos(),
        })
    }

    /// Hamilton product. `self * other` applies `other` first.
    pub fn mul(self, other: Self) -> Self {
        Self {
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        }
    }

    pub fn conjugate(self) -> Self {
        Self { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }

    /// Degrees for the inspector. `from_euler_xyz_degrees` builds the same quaternion.
    ///
    /// The order is roll about Z, then pitch about X, then yaw about Y. At ±90° pitch,
    /// yaw and roll are one twist; roll is reported as 0 and yaw keeps that twist.
    pub fn euler_xyz_degrees(self) -> Vec3 {
        let (x, y, z, w) = (self.x, self.y, self.z, self.w);
        let r12 = 2.0 * (y * z - w * x);
        let pitch = (-r12).clamp(-1.0, 1.0).asin();
        let (yaw, roll) = if pitch.abs() > 1.553 {
            let r00 = 1.0 - 2.0 * (y * y + z * z);
            let r01 = 2.0 * (x * y - w * z);
            if pitch > 0.0 { (r01.atan2(r00), 0.0) } else { ((-r01).atan2(r00), 0.0) }
        } else {
            let r02 = 2.0 * (x * z + w * y);
            let r22 = 1.0 - 2.0 * (x * x + y * y);
            let r10 = 2.0 * (x * y + w * z);
            let r11 = 1.0 - 2.0 * (x * x + z * z);
            (r02.atan2(r22), r10.atan2(r11))
        };
        Vec3::new(pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees())
    }

    pub fn from_euler_xyz_degrees(degrees: Vec3) -> Self {
        let x = Self::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), degrees.x.to_radians()).unwrap_or(Self::IDENTITY);
        let y = Self::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), degrees.y.to_radians()).unwrap_or(Self::IDENTITY);
        let z = Self::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), degrees.z.to_radians()).unwrap_or(Self::IDENTITY);
        y.mul(x).mul(z)
    }

    pub fn rotate(self, vector: Vec3) -> Vec3 {
        let axis = Vec3::new(self.x, self.y, self.z);
        let t = axis.cross(vector).scale(2.0);
        vector + t.scale(self.w) + axis.cross(t)
    }
}

/// Parent-relative rigid pose. Binary64 meters and a unit quaternion. No scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighPrecisionPose {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl HighPrecisionPose {
    pub const IDENTITY: Self = Self { translation: Vec3::ZERO, rotation: Quat::IDENTITY };

    pub const fn at(x: f64, y: f64, z: f64) -> Self {
        Self { translation: Vec3::new(x, y, z), rotation: Quat::IDENTITY }
    }
}

/// Frame origin and orientation in root space. Still binary64. Not a GPU matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPose {
    pub translation: Vec3,
    pub rotation: Quat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceError {
    MissingFrame,
    MissingParent,
    Cycle,
    BadRotation,
    BadProjection,
    FrameTooDeep,
    BadLight,
}

impl fmt::Display for SpaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingFrame => "frame does not exist",
            Self::MissingParent => "parent frame does not exist",
            Self::Cycle => "frame parent cycle",
            Self::BadRotation => "rotation axis must be non-zero",
            Self::BadProjection => "projection parameters are invalid",
            Self::FrameTooDeep => "frame tree is deeper than the supported chain",
            Self::BadLight => "light parameters are invalid",
        })
    }
}

#[derive(Clone)]
struct FrameSlot {
    parent: Option<FrameId>,
    pose: HighPrecisionPose,
}

#[derive(Clone)]
pub struct FrameGraph {
    next: u64,
    frames: Vec<Option<FrameSlot>>,
}

impl FrameGraph {
    pub fn new() -> Self {
        Self { next: 0, frames: vec![None] }
    }

    pub fn add(&mut self, parent: Option<FrameId>, pose: HighPrecisionPose) -> Result<FrameId, SpaceError> {
        if let Some(parent) = parent {
            self.slot(parent).map_err(|_| SpaceError::MissingParent)?;
        }
        self.next += 1;
        let id = FrameId(self.next);
        if self.frames.len() <= id.0 as usize {
            self.frames.resize(id.0 as usize + 1, None);
        }
        self.frames[id.0 as usize] = Some(FrameSlot { parent, pose });
        Ok(id)
    }

    pub fn set_parent(&mut self, frame: FrameId, parent: Option<FrameId>) -> Result<(), SpaceError> {
        self.slot(frame)?;
        if let Some(parent) = parent {
            self.slot(parent).map_err(|_| SpaceError::MissingParent)?;
            let mut cursor = Some(parent);
            let mut guard = 0;
            while let Some(current) = cursor {
                if current == frame {
                    return Err(SpaceError::Cycle);
                }
                guard += 1;
                if guard > MAX_DEPTH {
                    return Err(SpaceError::FrameTooDeep);
                }
                cursor = self.slot(current)?.parent;
            }
        }
        self.slot_mut(frame)?.parent = parent;
        Ok(())
    }

    pub fn set_local_translation(&mut self, frame: FrameId, translation: Vec3) -> Result<(), SpaceError> {
        self.slot_mut(frame)?.pose.translation = translation;
        Ok(())
    }

    pub fn set_local_rotation(&mut self, frame: FrameId, rotation: Quat) -> Result<(), SpaceError> {
        self.slot_mut(frame)?.pose.rotation = rotation;
        Ok(())
    }

    pub fn parent(&self, frame: FrameId) -> Result<Option<FrameId>, SpaceError> {
        Ok(self.slot(frame)?.parent)
    }

    pub fn local_pose(&self, frame: FrameId) -> Result<HighPrecisionPose, SpaceError> {
        Ok(self.slot(frame)?.pose)
    }

    /// Frame origin in root space. One walk per call, not per vertex.
    pub fn resolve(&self, frame: FrameId) -> Result<ResolvedPose, SpaceError> {
        let mut chain = [FrameId(0); MAX_DEPTH];
        let mut length = 0;
        let mut cursor = Some(frame);
        while let Some(current) = cursor {
            if length == MAX_DEPTH {
                return Err(SpaceError::FrameTooDeep);
            }
            if chain[..length].contains(&current) {
                return Err(SpaceError::Cycle);
            }
            chain[length] = current;
            length += 1;
            cursor = self.slot(current)?.parent;
        }
        let mut translation = Vec3::ZERO;
        let mut rotation = Quat::IDENTITY;
        for id in chain[..length].iter().rev() {
            let slot = self.slot(*id)?;
            translation = translation + rotation.rotate(slot.pose.translation);
            rotation = rotation.mul(slot.pose.rotation);
        }
        Ok(ResolvedPose { translation, rotation })
    }

    /// A point expressed inside `frame`, composed out to the root. Same rule as `FrameGraph.rootPosition`.
    pub fn root_point(&self, frame: FrameId, local: Vec3) -> Result<Vec3, SpaceError> {
        let pose = self.resolve(frame)?;
        Ok(pose.translation + pose.rotation.rotate(local))
    }

    fn slot(&self, frame: FrameId) -> Result<&FrameSlot, SpaceError> {
        self.frames.get(frame.0 as usize).and_then(Option::as_ref).ok_or(SpaceError::MissingFrame)
    }

    fn slot_mut(&mut self, frame: FrameId) -> Result<&mut FrameSlot, SpaceError> {
        self.frames.get_mut(frame.0 as usize).and_then(Option::as_mut).ok_or(SpaceError::MissingFrame)
    }
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_DEPTH: usize = 32;

/// Column-major float32 matrix for the GPU upload only. Not a world pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn from_rotation(rotation: Quat) -> Self {
        let (x, y, z, w) = (rotation.x as f32, rotation.y as f32, rotation.z as f32, rotation.w as f32);
        let x2 = x + x;
        let y2 = y + y;
        let z2 = z + z;
        let xx = x * x2;
        let xy = x * y2;
        let xz = x * z2;
        let yy = y * y2;
        let yz = y * z2;
        let zz = z * z2;
        let wx = w * x2;
        let wy = w * y2;
        let wz = w * z2;
        Self {
            cols: [
                [1.0 - (yy + zz), xy + wz, xz - wy, 0.0],
                [xy - wz, 1.0 - (xx + zz), yz + wx, 0.0],
                [xz + wy, yz - wx, 1.0 - (xx + yy), 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn from_rotation_translation(rotation: Quat, translation: [f32; 3]) -> Self {
        let mut matrix = Self::from_rotation(rotation);
        matrix.cols[3] = [translation[0], translation[1], translation[2], 1.0];
        matrix
    }

    /// Scale is object-local. It is not a scale on the world frame.
    pub fn from_rotation_translation_scale(rotation: Quat, translation: [f32; 3], scale: [f32; 3]) -> Self {
        let mut matrix = Self::from_rotation_translation(rotation, translation);
        for row in 0..3 {
            matrix.cols[0][row] *= scale[0];
            matrix.cols[1][row] *= scale[1];
            matrix.cols[2][row] *= scale[2];
        }
        matrix
    }

    /// Right-handed infinite reversed-Z. Vertical fov. Near maps to 1. Infinity maps toward 0.
    ///
    /// ADR-0021. There is no far clip in this matrix. `ndc_z = near / distance`.
    pub fn perspective_infinite_reverse_z(fov_y_radians: f32, aspect: f32, near: f32) -> Result<Self, SpaceError> {
        if !(fov_y_radians > 0.0) || !(aspect > 0.0) || !(near > 0.0) {
            return Err(SpaceError::BadProjection);
        }
        let height = 1.0 / (fov_y_radians * 0.5).tan();
        Ok(Self {
            cols: [
                [height / aspect, 0.0, 0.0, 0.0],
                [0.0, height, 0.0, 0.0],
                [0.0, 0.0, 0.0, -1.0],
                [0.0, 0.0, near, 0.0],
            ],
        })
    }

    pub fn mul(self, other: Self) -> Self {
        let mut cols = [[0.0; 4]; 4];
        for column in 0..4 {
            for row in 0..4 {
                cols[column][row] = self.cols[0][row] * other.cols[column][0]
                    + self.cols[1][row] * other.cols[column][1]
                    + self.cols[2][row] * other.cols[column][2]
                    + self.cols[3][row] * other.cols[column][3];
            }
        }
        Self { cols }
    }

    pub fn transform_point(self, point: [f32; 3]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for row in 0..4 {
            out[row] = self.cols[0][row] * point[0]
                + self.cols[1][row] * point[1]
                + self.cols[2][row] * point[2]
                + self.cols[3][row];
        }
        out
    }

    pub fn write_column_major(self, target: &mut [u8]) {
        debug_assert!(target.len() >= 64);
        let mut offset = 0;
        for column in self.cols {
            for value in column {
                target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                offset += 4;
            }
        }
    }
}

/// Subtract in binary64, then quantize. Never subtract two float32 world positions.
pub fn camera_relative_f32(object: Vec3, camera: Vec3) -> [f32; 3] {
    [(object.x - camera.x) as f32, (object.y - camera.y) as f32, (object.z - camera.z) as f32]
}

/// Reversed-Z clear. Farther than every finite fragment. ADR-0021.
pub const DEPTH_CLEAR: f32 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub frame: FrameId,
    pub vertical_fov_radians: f64,
    pub near_m: f32,
}

/// Float32 packet for one draw. Projection, orientation-only view, camera-relative model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuTransforms {
    pub projection: Mat4,
    pub view: Mat4,
    pub model: Mat4,
}

impl GpuTransforms {
    pub const BYTES: usize = 192;

    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0; Self::BYTES];
        self.projection.write_column_major(&mut bytes[0..64]);
        self.view.write_column_major(&mut bytes[64..128]);
        self.model.write_column_major(&mut bytes[128..192]);
        bytes
    }
}

pub fn render_transforms(
    frames: &FrameGraph,
    camera: &Camera,
    object: FrameId,
    aspect: f32,
) -> Result<GpuTransforms, SpaceError> {
    let object_pose = frames.resolve(object)?;
    let camera_pose = frames.resolve(camera.frame)?;
    let relative = camera_relative_f32(object_pose.translation, camera_pose.translation);
    Ok(GpuTransforms {
        model: Mat4::from_rotation_translation(object_pose.rotation, relative),
        view: Mat4::from_rotation(camera_pose.rotation.conjugate()),
        projection: Mat4::perspective_infinite_reverse_z(camera.vertical_fov_radians as f32, aspect, camera.near_m)?,
    })
}

/// The first presented scene. A large root translation, a camera, and one object a few meters in front of it.
pub struct BootstrapScene {
    pub frames: FrameGraph,
    pub camera: Camera,
    /// Submitted first. Closer to the camera. Depth must keep it visible.
    pub near_object: FrameId,
    /// Submitted second, behind the near object, and larger on screen.
    pub far_object: FrameId,
}

pub const BOOTSTRAP_ROOT_M: f64 = 1_000_000_000.0;

/// One world, two cameras. Views do not own a second copy of the frames or the meshes.
pub struct SharedBootstrap {
    pub frames: FrameGraph,
    pub near_object: FrameId,
    pub far_object: FrameId,
    pub front: Camera,
    pub side: Camera,
}

pub fn bootstrap_shared_world() -> SharedBootstrap {
    let mut frames = FrameGraph::new();
    let root = frames.add(None, HighPrecisionPose::at(BOOTSTRAP_ROOT_M, 0.0, 0.0)).expect("root");
    let scene = frames.add(Some(root), HighPrecisionPose::IDENTITY).expect("scene");
    let front = frames.add(Some(scene), HighPrecisionPose::IDENTITY).expect("front camera");
    let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), -0.45).expect("yaw");
    let side = frames
        .add(
            Some(scene),
            HighPrecisionPose { translation: Vec3::new(1.6, 0.45, 0.35), rotation: yaw },
        )
        .expect("side camera");
    let near_object = frames.add(Some(scene), HighPrecisionPose::at(0.0, 0.0, -2.0)).expect("near");
    let far_object = frames.add(Some(scene), HighPrecisionPose::at(0.0, 0.0, -5.0)).expect("far");
    let camera = |frame| Camera { frame, vertical_fov_radians: 60.0_f64.to_radians(), near_m: 0.1 };
    SharedBootstrap {
        frames,
        near_object,
        far_object,
        front: camera(front),
        side: camera(side),
    }
}

pub fn bootstrap_triangle_scene() -> BootstrapScene {
    let mut frames = FrameGraph::new();
    let root = frames.add(None, HighPrecisionPose::at(BOOTSTRAP_ROOT_M, 0.0, 0.0)).expect("root");
    let scene = frames.add(Some(root), HighPrecisionPose::IDENTITY).expect("scene");
    let camera = frames.add(Some(scene), HighPrecisionPose::IDENTITY).expect("camera");
    let near_object = frames.add(Some(scene), HighPrecisionPose::at(0.0, 0.0, -2.0)).expect("near");
    let far_object = frames.add(Some(scene), HighPrecisionPose::at(0.0, 0.0, -5.0)).expect("far");
    BootstrapScene {
        frames,
        camera: Camera {
            frame: camera,
            vertical_fov_radians: 60.0_f64.to_radians(),
            near_m: 0.1,
        },
        near_object,
        far_object,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaw90() -> Quat {
        Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap()
    }

    #[test]
    fn euler_degrees_round_trip_the_authored_quaternion() {
        let identity = Quat::IDENTITY.euler_xyz_degrees();
        assert!(identity.x.abs() < 1.0e-8 && identity.y.abs() < 1.0e-8 && identity.z.abs() < 1.0e-8);
        assert!(quaternion_close(Quat::from_euler_xyz_degrees(identity), Quat::IDENTITY));
        for degrees in [Vec3::new(90.0, 0.0, 0.0), Vec3::new(0.0, 90.0, 0.0), Vec3::new(0.0, 0.0, 90.0), Vec3::new(20.0, -35.0, 10.0)] {
            let rotation = Quat::from_euler_xyz_degrees(degrees);
            let back = rotation.euler_xyz_degrees();
            let again = Quat::from_euler_xyz_degrees(back);
            assert!(quaternion_close(rotation, again), "{degrees:?} -> {back:?}");
        }
    }

    fn quaternion_close(left: Quat, right: Quat) -> bool {
        let same = (left.x - right.x).abs() + (left.y - right.y).abs() + (left.z - right.z).abs() + (left.w - right.w).abs();
        let opposite = (left.x + right.x).abs() + (left.y + right.y).abs() + (left.z + right.z).abs() + (left.w + right.w).abs();
        same < 1.0e-6 || opposite < 1.0e-6
    }

    #[test]
    fn identity_local_point_is_unchanged() {
        let mut frames = FrameGraph::new();
        let root = frames.add(None, HighPrecisionPose::IDENTITY).unwrap();
        let point = frames.root_point(root, Vec3::new(1.0, 2.0, 3.0)).unwrap();
        assert_eq!(point, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn parent_yaw_matches_the_existing_right_handed_rule() {
        let mut frames = FrameGraph::new();
        let root = frames
            .add(None, HighPrecisionPose { translation: Vec3::ZERO, rotation: yaw90() })
            .unwrap();
        let point = frames.root_point(root, Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert!(point.x.abs() < 1.0e-9);
        assert!(point.y.abs() < 1.0e-9);
        assert!((point.z + 1.0).abs() < 1.0e-9);
        let matrix = Mat4::from_rotation(yaw90());
        let transformed = matrix.transform_point([1.0, 0.0, 0.0]);
        assert!(transformed[0].abs() < 1.0e-5);
        assert!(transformed[1].abs() < 1.0e-5);
        assert!((transformed[2] + 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn child_offset_composes_through_the_parent() {
        let mut frames = FrameGraph::new();
        let root = frames
            .add(None, HighPrecisionPose { translation: Vec3::ZERO, rotation: yaw90() })
            .unwrap();
        let child = frames.add(Some(root), HighPrecisionPose::at(1.0, 0.0, 0.0)).unwrap();
        let pose = frames.resolve(child).unwrap();
        assert!(pose.translation.x.abs() < 1.0e-9);
        assert!((pose.translation.z + 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn grandchild_walks_to_the_root() {
        let mut frames = FrameGraph::new();
        let root = frames.add(None, HighPrecisionPose::at(10.0, 0.0, 0.0)).unwrap();
        let child = frames.add(Some(root), HighPrecisionPose::at(1.0, 0.0, 0.0)).unwrap();
        let grandchild = frames.add(Some(child), HighPrecisionPose::at(0.0, 2.0, 0.0)).unwrap();
        let pose = frames.resolve(grandchild).unwrap();
        assert_eq!(pose.translation, Vec3::new(11.0, 2.0, 0.0));
    }

    #[test]
    fn reparent_into_a_descendant_is_rejected() {
        let mut frames = FrameGraph::new();
        let parent = frames.add(None, HighPrecisionPose::IDENTITY).unwrap();
        let child = frames.add(Some(parent), HighPrecisionPose::IDENTITY).unwrap();
        assert_eq!(frames.set_parent(parent, Some(child)), Err(SpaceError::Cycle));
        assert_eq!(frames.add(Some(FrameId(99)), HighPrecisionPose::IDENTITY), Err(SpaceError::MissingParent));
    }

    #[test]
    fn identity_camera_view_has_no_translation() {
        let mut frames = FrameGraph::new();
        let camera = frames.add(None, HighPrecisionPose::at(4.0, 5.0, 6.0)).unwrap();
        let packet = render_transforms(
            &frames,
            &Camera { frame: camera, vertical_fov_radians: 1.0, near_m: 0.1 },
            camera,
            1.0,
        )
        .unwrap();
        assert_eq!(packet.view, Mat4::IDENTITY);
        assert_eq!(packet.model.cols[3], [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn two_cameras_stay_camera_relative_at_a_billion_meters() {
        let world = bootstrap_shared_world();
        let front = render_transforms(&world.frames, &world.front, world.near_object, 1.0).unwrap();
        let side = render_transforms(&world.frames, &world.side, world.near_object, 0.5).unwrap();
        assert_eq!(front.model.cols[3][0], 0.0);
        assert!((front.model.cols[3][2] + 2.0).abs() < 1.0e-4);
        assert!(side.model.cols[3][0].abs() < 20.0);
        assert!(side.model.cols[3][1].abs() < 20.0);
        assert!(side.model.cols[3][2].abs() < 20.0);
        assert_ne!(front.model.cols[3], side.model.cols[3]);
        assert_ne!(front.view, side.view);
        let root = world.frames.resolve(world.near_object).unwrap();
        assert!((root.translation.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
    }

    fn ndc_z(projection: Mat4, distance: f32) -> f32 {
        let clip = projection.transform_point([0.0, 0.0, -distance]);
        clip[2] / clip[3]
    }

    #[test]
    fn reversed_z_puts_near_at_one_and_falls_toward_zero() {
        let projection = Mat4::perspective_infinite_reverse_z(std::f32::consts::FRAC_PI_2, 1.0, 0.1).unwrap();
        let near = ndc_z(projection, 0.1);
        assert!((near - 1.0).abs() < 1.0e-4, "{near}");
        let mid = ndc_z(projection, 10.0);
        let far = ndc_z(projection, 1_000.0);
        let huge = ndc_z(projection, 1.0e7);
        assert!(near > mid && mid > far && far > huge, "{near} {mid} {far} {huge}");
        assert!(huge > 0.0 && huge.is_finite(), "{huge}");
        let up = projection.transform_point([0.0, 1.0, -2.0]);
        assert!(up[1] / up[3] > 0.0);
        let right = projection.transform_point([1.0, 0.0, -2.0]);
        assert!(right[0] / right[3] > 0.0);
        assert!(Mat4::perspective_infinite_reverse_z(0.0, 1.0, 0.1).is_err());
        assert_eq!(DEPTH_CLEAR, 0.0);
        assert!(near > DEPTH_CLEAR);
    }

    #[test]
    fn centimeter_survives_at_eight_million_meters() {
        let mut frames = FrameGraph::new();
        let planet = frames.add(None, HighPrecisionPose::at(8_000_000.0, 0.0, 0.0)).unwrap();
        let world = frames.root_point(planet, Vec3::new(0.25, 0.0, 0.0)).unwrap();
        assert!((world.x - 8_000_000.25).abs() < 1.0e-6);
        let collapsed = (world.x as f32) - (8_000_000.0_f32);
        assert_eq!(collapsed, 0.0);
        let relative = camera_relative_f32(world, Vec3::new(8_000_000.0, 0.0, 0.0));
        assert!((relative[0] - 0.25).abs() < 1.0e-6);
    }

    #[test]
    fn billion_meter_origin_uploads_a_meter_scale_delta() {
        let mut frames = FrameGraph::new();
        let root = frames.add(None, HighPrecisionPose::at(1_000_000_000.0, 0.0, 0.0)).unwrap();
        let camera = frames.add(Some(root), HighPrecisionPose::IDENTITY).unwrap();
        let object = frames.add(Some(root), HighPrecisionPose::at(3.0, 0.25, -2.0)).unwrap();
        let object_pose = frames.resolve(object).unwrap();
        assert!((object_pose.translation.x - 1_000_000_003.0).abs() < 1.0e-3);
        let collapsed = (object_pose.translation.x as f32) - (1_000_000_000.0_f32);
        assert_eq!(collapsed, 0.0);
        let packet = render_transforms(
            &frames,
            &Camera { frame: camera, vertical_fov_radians: 60.0_f64.to_radians(), near_m: 0.1 },
            object,
            16.0 / 9.0,
        )
        .unwrap();
        let translation = packet.model.cols[3];
        assert!((translation[0] - 3.0).abs() < 1.0e-4);
        assert!((translation[1] - 0.25).abs() < 1.0e-4);
        assert!((translation[2] + 2.0).abs() < 1.0e-4);
        assert_eq!(packet.view, Mat4::IDENTITY);
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), 192);
        assert!(bytes.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn bootstrap_triangle_is_in_front_of_a_far_camera() {
        let scene = bootstrap_triangle_scene();
        let camera = scene.frames.resolve(scene.camera.frame).unwrap();
        let near = scene.frames.resolve(scene.near_object).unwrap();
        let far = scene.frames.resolve(scene.far_object).unwrap();
        assert!((far.translation.z - (BOOTSTRAP_ROOT_M * 0.0 - 5.0)).abs() < 1.0e-6 || (far.translation.z + 5.0).abs() < 1.0e-3);
        assert!((camera.translation.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-3);
        assert!((near.translation.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-3);
        let near_packet = render_transforms(&scene.frames, &scene.camera, scene.near_object, 1600.0 / 900.0).unwrap();
        let far_packet = render_transforms(&scene.frames, &scene.camera, scene.far_object, 1600.0 / 900.0).unwrap();
        assert!((near_packet.model.cols[3][2] + 2.0).abs() < 1.0e-4);
        assert!((far_packet.model.cols[3][2] + 5.0).abs() < 1.0e-4);
        assert_eq!(near_packet.model.cols[3][0], 0.0);
        let near_clip = near_packet.projection.mul(near_packet.view).mul(near_packet.model).transform_point([0.0, 0.6, 0.0]);
        let far_clip = far_packet.projection.mul(far_packet.view).mul(far_packet.model).transform_point([0.0, 0.0, 0.0]);
        let near_z = near_clip[2] / near_clip[3];
        let far_z = far_clip[2] / far_clip[3];
        assert!(near_z > far_z, "closer depth {near_z} must be greater than farther depth {far_z}");
        assert!(near_z > DEPTH_CLEAR && far_z > DEPTH_CLEAR);
    }
}
