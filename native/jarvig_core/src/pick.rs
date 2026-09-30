//! Analytic viewport ray and CPU intersection. Not a GPU id buffer.
//!
//! The ray is built from the camera basis and the vertical field of view.
//! It does not read a depth buffer, so reversed-Z does not affect the hit.
//! Positions stay binary64. The semantic result is an [`EntityId`].
//! The dedicated server does not call this. There is no window and no overlay.

use crate::{EntityId, MeshLibrary, RenderSceneSnapshot, ResolvedPose, Vec3};

/// World-space ray. Origin is the camera position. Direction is a unit vector.
#[derive(Clone, Copy, Debug)]
pub struct PickRay {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// One visible renderable hit. Not an [`crate::ObjectId`] and not a mesh id.
#[derive(Clone, Copy, Debug)]
pub struct PickHit {
    pub entity: EntityId,
    pub position: Vec3,
    pub distance: f64,
}

/// Perspective ray through a client pixel.
///
/// Pixel (0, 0) is the top-left of the drawable, matching the child window.
/// The sample is the pixel center. Window Y grows downward, so it is flipped
/// into JARVIG's +Y up. +X is right and camera forward is local -Z.
///
/// `width` and `height` are the configured drawable pixels, which match the
/// child client and the depth target.
pub fn perspective_ray(camera: ResolvedPose, vertical_fov_radians: f64, width: u32, height: u32, pixel_x: f64, pixel_y: f64) -> Option<PickRay> {
    if width == 0 || height == 0 || !vertical_fov_radians.is_finite() || vertical_fov_radians <= 0.0 {
        return None;
    }
    if !pixel_x.is_finite() || !pixel_y.is_finite() {
        return None;
    }
    let aspect = width as f64 / height as f64;
    let ndc_x = ((pixel_x + 0.5) / width as f64) * 2.0 - 1.0;
    let ndc_y = 1.0 - ((pixel_y + 0.5) / height as f64) * 2.0;
    let half = (vertical_fov_radians * 0.5).tan();
    if !half.is_finite() {
        return None;
    }
    let forward = camera.rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
    let right = camera.rotation.rotate(Vec3::new(1.0, 0.0, 0.0));
    let up = camera.rotation.rotate(Vec3::new(0.0, 1.0, 0.0));
    let direction = normalize(forward + right.scale(ndc_x * half * aspect) + up.scale(ndc_y * half))?;
    Some(PickRay { origin: camera.translation, direction })
}

/// Timings for one click. Outliner selection leaves the pick fields at zero.
#[derive(Clone, Copy, Debug, Default)]
pub struct PickTimings {
    pub broadphase_us: u128,
    pub mesh_us: u128,
    pub candidates: u32,
    pub rejected: u32,
}

/// Closest visible actor. Bounds reject first. A mesh is queried through its BVH.
/// The returned id is the render instance's authoring entity.
pub fn pick_snapshot(ray: PickRay, snapshot: &RenderSceneSnapshot, meshes: &MeshLibrary) -> Option<PickHit> {
    pick_snapshot_timed(ray, snapshot, meshes).0
}

pub fn pick_snapshot_timed(ray: PickRay, snapshot: &RenderSceneSnapshot, meshes: &MeshLibrary) -> (Option<PickHit>, PickTimings) {
    let mut timings = PickTimings::default();
    let started = std::time::Instant::now();
    let mut candidates = Vec::new();
    for instance in snapshot.instances().iter().filter(|instance| instance.visible) {
        let Some(mesh) = meshes.get(instance.mesh) else { continue };
        let sphere = mesh.bounds().sphere;
        let center = instance.pose.translation
            + instance.pose.rotation.rotate(Vec3::new(
                sphere.center[0] as f64 * instance.scale.x,
                sphere.center[1] as f64 * instance.scale.y,
                sphere.center[2] as f64 * instance.scale.z,
            ));
        let max_scale = instance.scale.x.abs().max(instance.scale.y.abs()).max(instance.scale.z.abs());
        let radius = sphere.radius as f64 * max_scale;
        if ray_sphere(ray, center, radius).is_none() {
            timings.rejected = timings.rejected.saturating_add(1);
            continue;
        }
        candidates.push(instance);
    }
    timings.candidates = candidates.len() as u32;
    timings.broadphase_us = started.elapsed().as_micros();
    let mesh_started = std::time::Instant::now();
    let mut best: Option<PickHit> = None;
    for instance in candidates {
        let Some(mesh) = meshes.get(instance.mesh) else { continue };
        let Some(local) = local_ray(ray, instance.pose, instance.scale) else { continue };
        let Some(distance) = mesh.intersect_local_ray([local.origin.x, local.origin.y, local.origin.z], [local.direction.x, local.direction.y, local.direction.z]) else {
            continue;
        };
        if best.as_ref().is_some_and(|hit| hit.distance <= distance) {
            continue;
        }
        best = Some(PickHit {
            entity: instance.entity,
            position: ray.origin + ray.direction.scale(distance),
            distance,
        });
    }
    timings.mesh_us = mesh_started.elapsed().as_micros();
    (best, timings)
}

/// Same `t` as the world ray. The local direction is not renormalized.
fn local_ray(ray: PickRay, pose: ResolvedPose, scale: Vec3) -> Option<PickRay> {
    if scale.x.abs() < 1.0e-12 || scale.y.abs() < 1.0e-12 || scale.z.abs() < 1.0e-12 {
        return None;
    }
    let offset = Vec3::new(ray.origin.x - pose.translation.x, ray.origin.y - pose.translation.y, ray.origin.z - pose.translation.z);
    let rotated = pose.rotation.conjugate().rotate(offset);
    let turned = pose.rotation.conjugate().rotate(ray.direction);
    Some(PickRay {
        origin: Vec3::new(rotated.x / scale.x, rotated.y / scale.y, rotated.z / scale.z),
        direction: Vec3::new(turned.x / scale.x, turned.y / scale.y, turned.z / scale.z),
    })
}

fn ray_sphere(ray: PickRay, center: Vec3, radius: f64) -> Option<f64> {
    if radius < 0.0 {
        return None;
    }
    let offset = Vec3::new(center.x - ray.origin.x, center.y - ray.origin.y, center.z - ray.origin.z);
    let along = dot(offset, ray.direction);
    let closest = dot(offset, offset) - along * along;
    let radius_sq = radius * radius;
    if closest > radius_sq {
        return None;
    }
    let half = (radius_sq - closest).sqrt();
    let near = along - half;
    if near > 1.0e-4 {
        Some(near)
    } else if along + half > 1.0e-4 {
        Some(along + half)
    } else {
        None
    }
}

fn dot(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn normalize(value: Vec3) -> Option<Vec3> {
    let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
    if length < 1.0e-12 || !length.is_finite() {
        None
    } else {
        Some(value.scale(1.0 / length))
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::{near_triangle_mesh, Camera, Quat, SceneWorld, Vec3, BOOTSTRAP_ROOT_M};

    fn identity_camera() -> ResolvedPose {
        ResolvedPose { translation: Vec3::new(BOOTSTRAP_ROOT_M, 0.0, 0.0), rotation: Quat::IDENTITY }
    }

    #[test]
    fn center_pixel_looks_down_negative_z_and_corners_stay_finite() {
        let camera = identity_camera();
        let center = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, 99.5, 49.5).unwrap();
        assert!((center.direction.x).abs() < 1.0e-6);
        assert!((center.direction.y).abs() < 1.0e-6);
        assert!((center.direction.z + 1.0).abs() < 1.0e-6);
        assert!((center.origin.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
        let left = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, 0.0, 49.5).unwrap();
        assert!(left.direction.x < -0.2);
        let right = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, 199.0, 49.5).unwrap();
        assert!(right.direction.x > 0.2);
        let top = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, 99.5, 0.0).unwrap();
        assert!(top.direction.y > 0.2);
        let bottom = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, 99.5, 99.0).unwrap();
        assert!(bottom.direction.y < -0.2);
        let wide = perspective_ray(camera, 60.0_f64.to_radians(), 400, 100, 0.0, 49.5).unwrap();
        assert!(wide.direction.x.abs() > left.direction.x.abs());
        for (x, y) in [(0.0, 0.0), (199.0, 0.0), (0.0, 99.0), (199.0, 99.0)] {
            let ray = perspective_ray(camera, 60.0_f64.to_radians(), 200, 100, x, y).unwrap();
            assert!(ray.direction.x.is_finite() && ray.direction.y.is_finite() && ray.direction.z.is_finite());
        }
        let shifted = Vec3::new(camera.translation.x + 0.01, camera.translation.y, camera.translation.z);
        assert!((shifted.x - (camera.translation.x as f32 as f64)).abs() > 0.001 || (shifted.x - BOOTSTRAP_ROOT_M - 0.01).abs() < 1.0e-9);
        assert!((shifted.x - (BOOTSTRAP_ROOT_M + 0.01)).abs() < 1.0e-9);
    }

    #[test]
    fn the_closer_visible_triangle_wins_and_a_hidden_one_does_not() {
        let mut world = SceneWorld::bootstrap();
        let snapshot = world.extract(crate::RenderFrameId(1)).unwrap();
        let camera = snapshot.camera(world.front_camera().frame).unwrap().pose;
        let ray = perspective_ray(camera, world.front_camera().vertical_fov_radians, 200, 100, 99.5, 49.5).unwrap();
        let hit = pick_snapshot(ray, &snapshot, world.meshes()).unwrap();
        let near = world.entity_outline()[0].uuid;
        let far = world.entity_outline()[1].uuid;
        assert_eq!(hit.entity, near);
        assert!(hit.distance > 1.0 && hit.distance < 3.0);
        let miss = PickRay { origin: camera.translation, direction: Vec3::new(0.0, 1.0, 0.0) };
        assert!(pick_snapshot(miss, &snapshot, world.meshes()).is_none());
        let near_id = world.objects().next().unwrap();
        world.set_visible(near_id, false).unwrap();
        let hidden = world.extract(crate::RenderFrameId(2)).unwrap();
        let past = pick_snapshot(ray, &hidden, world.meshes()).unwrap();
        assert_eq!(past.entity, far);
        assert!(past.distance > hit.distance);
        let mesh = near_triangle_mesh();
        assert!(mesh.intersect_local_ray([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).is_some());
        assert!(mesh.intersect_local_ray([10.0, 10.0, 5.0], [0.0, 0.0, -1.0]).is_none());
        let _ = Camera { frame: world.front_camera().frame, vertical_fov_radians: 1.0, near_m: 0.1 };
    }

    #[test]
    fn a_ray_beside_the_near_triangle_hits_the_far_one() {
        let world = SceneWorld::bootstrap();
        let snapshot = world.extract(crate::RenderFrameId(3)).unwrap();
        let camera = snapshot.camera(world.front_camera().frame).unwrap().pose;
        let near = world.entity_outline()[0].uuid;
        let far = world.entity_outline()[1].uuid;
        let target = Vec3::new(camera.translation.x + 1.8, camera.translation.y - 1.2, camera.translation.z - 5.0);
        let direction = normalize(Vec3::new(target.x - camera.translation.x, target.y - camera.translation.y, target.z - camera.translation.z)).unwrap();
        let ray = PickRay { origin: camera.translation, direction };
        let hit = pick_snapshot(ray, &snapshot, world.meshes()).unwrap();
        assert_eq!(hit.entity, far);
        assert_ne!(hit.entity, near);
        let _: crate::EntityId = hit.entity;
        assert!((ray.origin.x - BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
    }
}
