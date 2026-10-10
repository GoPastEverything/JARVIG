//! Deterministic comparison of a recorded log with the stored body.
//!
//! Intent Shadow Phase 1 stays frozen. These tests do not promote the log and do not
//! start a per-camera realization. A rewritten upstream parameter is expected to diverge.

use crate::{
    empty_world_level, evaluate_intent_shadow, parse_level, AuthoringResult, BlockOp, BlockRecord, ConcreteElement, EntityId, IntentShadowStatus,
    LevelDocument, MaterialAssetRef, SceneWorld, SolidBody, TopologyEdit, Vec3, BLOCK_EXTRUDE_STEP_M, BLOCK_MIN_EXTENT_M, FIELD_BLOCK_MATERIAL, FIELD_NAME,
    PLANE_DEPTH_M, PLANE_WIDTH_M,
};

#[test]
fn a_plane_is_the_same_evaluated_solid() {
    let document = empty_world_level();
    let mut world = document.instantiate().unwrap();
    let block = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
    let plane = world.create_plane(Vec3::new(2.5, 1.0, -4.0)).unwrap();
    let second = world.create_plane(Vec3::new(5.0, 1.0, -4.0)).unwrap();
    assert_eq!(inspected(&world, block, FIELD_NAME), "Block");
    assert_eq!(inspected(&world, plane, FIELD_NAME), "Plane");
    assert_eq!(inspected(&world, second, FIELD_NAME), "Plane 2");
    let record = world.authored_block(plane).unwrap();
    assert!(record.is_plain());
    assert_eq!(record.size_m, [PLANE_WIDTH_M, BLOCK_MIN_EXTENT_M, PLANE_DEPTH_M]);
    assert_eq!(record.seed_size_m, Some(record.size_m));
    assert!(record.body.is_none());
    assert_eq!(record.material.name, "standard_white");
    assert_eq!(inspected(&world, plane, FIELD_BLOCK_MATERIAL), "standard_white");
    let report = evaluate_intent_shadow(&record);
    assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
    assert!(report.text.contains("analytic box, no stored topology"), "{}", report.text);
    let json = LevelDocument::capture(&world, document.level_uuid, "Plane").unwrap().to_json();
    assert!(json.contains("\"Plane\""));
    assert!(!json.contains("\"body\""));
    assert!(!json.contains("seed_size_m"));
    let loaded = parse_level(&json).unwrap().instantiate().unwrap();
    let again = loaded.authored_block(plane).unwrap();
    assert_eq!(again.size_m, record.size_m);
    assert_eq!(again.material, record.material);
    assert_eq!(evaluate_intent_shadow(&again).status, IntentShadowStatus::Pass);
}

#[test]
fn shadow_validation_downstream_edits_match_on_a_block_and_a_plane() {
    let document = empty_world_level();
    let mut world = document.instantiate().unwrap();
    let mut block_record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
    block_record.material = MaterialAssetRef::builtin("bootstrap_near", [0.25, 0.5, 1.0, 1.0], 0.0, 0.5, [0.0, 0.0, 0.0, 1.0]);
    let block = world.create_block(Vec3::new(0.0, 1.0, -4.0), block_record).unwrap();
    let plane = world.create_plane(Vec3::new(2.5, 1.0, -4.0)).unwrap();
    sculpt(&mut world, block, 5, "bootstrap_near");
    sculpt(&mut world, plane, 3, "standard_white");
    reload_twice(&document, &world, &[block, plane]);
}

#[test]
fn shadow_validation_upstream_rewrite_diverges() {
    let document = empty_world_level();
    let mut world = document.instantiate().unwrap();
    let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
    sculpt_topology(&mut world, id, 5);
    let record = world.authored_block(id).unwrap();
    assert_eq!(evaluate_intent_shadow(&record).status, IntentShadowStatus::Pass);

    let mut reseeded = record.clone();
    let mut seed = reseeded.seed_size_m.expect("creation size");
    seed[0] += 0.25;
    reseeded.seed_size_m = Some(seed);
    let drifted = evaluate_intent_shadow(&reseeded);
    assert_eq!(drifted.status, IntentShadowStatus::Mismatch, "{}", drifted.text);
    assert!(drifted.text.contains("INTENT_SHADOW_MISMATCH"), "{}", drifted.text);
    assert_eq!(reseeded.body, record.body);
    assert_eq!(world.authored_block(id).unwrap(), record);

    let mut coarser = record.clone();
    let index = coarser.history.iter().position(|op| matches!(op, BlockOp::SubdivideFace { .. })).expect("subdivide");
    if let BlockOp::SubdivideFace { u, v, .. } = &mut coarser.history[index] {
        *u = 4;
        *v = 4;
    }
    let broken = evaluate_intent_shadow(&coarser);
    assert_eq!(broken.status, IntentShadowStatus::Mismatch, "{}", broken.text);
    assert!(broken.text.contains("that element is not on the solid") || broken.text.contains("subdivide refused"), "{}", broken.text);
    assert_eq!(coarser.body, record.body);

    let mut wider = record.clone();
    let extrude = wider.history.iter().position(|op| matches!(op, BlockOp::ExtrudeFaces { .. })).expect("extrude");
    if let BlockOp::ExtrudeFaces { delta_m, .. } = &mut wider.history[extrude] {
        delta_m[2] += 0.15;
    }
    let shifted = evaluate_intent_shadow(&wider);
    assert_eq!(shifted.status, IntentShadowStatus::Mismatch, "{}", shifted.text);
    assert_eq!(wider.body, record.body);
    assert_eq!(world.authored_block(id).unwrap().body, record.body);
}

fn sculpt(world: &mut SceneWorld, id: EntityId, face: u32, material: &str) {
    let created = sculpt_topology(world, id, face);
    let matched = world.authored_block(id).unwrap();
    assert_eq!(matched.material.name, material);
    assert_eq!(inspected(world, id, FIELD_BLOCK_MATERIAL), material);
    let before_size = evaluate_intent_shadow(&matched);
    assert_eq!(before_size.status, IntentShadowStatus::Pass, "{}", before_size.text);
    assert!(before_size.text.contains("body hash: match"), "{}", before_size.text);
    assert_eq!(unmatched(&before_size), 0, "{}", before_size.text);
    assert!(created.face > 6 && created.edge > 20 && created.vertex > 8);
    assert!(matched.steps.iter().any(|step| step.concrete.contains(&ConcreteElement::Face(created.face))));
    assert!(matched.steps.iter().any(|step| step.concrete.contains(&ConcreteElement::Edge(created.edge))));
    assert!(matched.steps.iter().any(|step| step.concrete.contains(&ConcreteElement::Vertex(created.vertex))));
    assert_closed(&matched);
    assert_collision(world, id);

    let grown = matched.size_m[0] + 0.5;
    assert_eq!(world.set_block_extent(id, 0, grown).unwrap(), AuthoringResult::Applied);
    let scaled = world.authored_block(id).unwrap();
    assert_eq!(scaled.seed_size_m, matched.seed_size_m);
    assert!(scaled.size_m[0] > matched.size_m[0]);
    assert_eq!(scaled.material, matched.material);
    let after_size = evaluate_intent_shadow(&scaled);
    assert_eq!(after_size.status, IntentShadowStatus::Pass, "{}", after_size.text);
    assert_eq!(unmatched(&after_size), 0, "{}", after_size.text);
    assert_closed(&scaled);
    assert_collision(world, id);
    let untouched = scaled.clone();
    let _ = evaluate_intent_shadow(&untouched);
    assert_eq!(world.authored_block(id).unwrap(), untouched);
}

struct Created {
    face: u32,
    edge: u32,
    vertex: u32,
}

fn sculpt_topology(world: &mut SceneWorld, id: EntityId, face: u32) -> Created {
    assert_eq!(world.subdivide_block_face(id, face, 2, 2).unwrap(), AuthoringResult::Applied);
    let divided = body(world, id);
    let parent = divided.unit_normal(face).expect("subdivide keeps the face");
    let cell = divided
        .faces
        .iter()
        .find(|candidate| candidate.id != face && divided.unit_normal(candidate.id).is_some_and(|normal| dot(normal, parent) > 0.9))
        .map(|candidate| candidate.id)
        .expect("subdivide created a cell");
    let before_faces: Vec<u32> = divided.faces.iter().map(|candidate| candidate.id).collect();
    let cell_normal = divided.unit_normal(cell).unwrap();
    assert_eq!(
        world.extrude_block_faces(id, &[cell], scale(cell_normal, BLOCK_EXTRUDE_STEP_M)).unwrap(),
        AuthoringResult::Applied
    );
    let raised = body(world, id);
    let side = raised
        .faces
        .iter()
        .find(|candidate| !before_faces.contains(&candidate.id) && raised.unit_normal(candidate.id).is_some_and(|normal| dot(normal, parent).abs() < 0.5))
        .map(|candidate| candidate.id)
        .expect("extrude created a side face");
    let side_normal = raised.unit_normal(side).unwrap();
    assert_eq!(world.extrude_block_faces(id, &[side], scale(side_normal, 0.2)).unwrap(), AuthoringResult::Applied);

    let mut edge = None;
    let mut vertex = None;
    let candidates: Vec<u32> = body(world, id).edges.iter().map(|candidate| candidate.id).filter(|candidate| *candidate > 20).collect();
    for candidate in candidates {
        let before: Vec<u32> = body(world, id).vertices.iter().map(|vertex| vertex.id).collect();
        if world.split_block_edge(id, candidate).is_err() {
            continue;
        }
        vertex = body(world, id).vertices.iter().map(|vertex| vertex.id).find(|id| !before.contains(id));
        edge = Some(candidate);
        break;
    }
    let edge = edge.expect("no edge created by an earlier edit accepted a split");
    let vertex = vertex.expect("split did not create a vertex");
    move_created_vertex(world, id, vertex);
    move_created_edge(world, id);
    bevel_created_edge(world, id);
    Created { face: cell, edge, vertex }
}

fn move_created_vertex(world: &mut SceneWorld, id: EntityId, vertex: u32) {
    let deltas = [[0.02, 0.0, 0.0], [0.0, 0.02, 0.0], [0.0, 0.0, 0.02]];
    for delta in deltas {
        let current = body(world, id);
        if let Ok(edit) = current.move_vertex(vertex, delta) {
            commit_preview(world, id, edit, BlockOp::MoveVertex { vertex, delta_m: delta });
            return;
        }
    }
    panic!("the vertex created by split accepted no move");
}

fn move_created_edge(world: &mut SceneWorld, id: EntityId) {
    let edges: Vec<u32> = body(world, id).edges.iter().map(|edge| edge.id).filter(|edge| *edge > 20).collect();
    let deltas = [[0.02, 0.0, 0.0], [0.0, 0.02, 0.0], [0.0, 0.0, 0.02]];
    for edge in edges {
        for delta in deltas {
            let current = body(world, id);
            if let Ok(edit) = current.move_edge(edge, delta) {
                commit_preview(world, id, edit, BlockOp::MoveEdge { edge, delta_m: delta });
                return;
            }
        }
    }
    panic!("no edge created by an earlier edit accepted a move");
}

fn bevel_created_edge(world: &mut SceneWorld, id: EntityId) {
    let current = body(world, id);
    let mut ordered: Vec<u32> = current.edges.iter().map(|edge| edge.id).filter(|edge| *edge > 20).collect();
    ordered.extend(current.edges.iter().map(|edge| edge.id).filter(|edge| *edge <= 20));
    for width in [0.02, 0.01, 0.05, 0.1] {
        for edge in &ordered {
            if let Ok(cut) = current.bevel_edges(&[*edge], width) {
                let edges = cut.edges.clone();
                let distance_m = cut.width_m;
                commit_preview(world, id, cut.edit, BlockOp::BevelEdges { edges, distance_m });
                return;
            }
        }
    }
    panic!("the sculpted solid has no bevelable edge");
}

fn commit_preview(world: &mut SceneWorld, id: EntityId, edit: TopologyEdit, op: BlockOp) {
    let local = world.entity_local_pose(id).unwrap();
    let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
    assert_eq!(world.preview_block_body(id, edit.body, translation).unwrap(), AuthoringResult::Applied);
    assert_eq!(world.commit_block_topology(id, op).unwrap(), AuthoringResult::Applied);
}

fn reload_twice(document: &LevelDocument, world: &SceneWorld, ids: &[EntityId]) {
    let mut json = LevelDocument::capture(world, document.level_uuid, "Shadow").unwrap().to_json();
    for _ in 0..2 {
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the level");
        }
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        for id in ids {
            let record = loaded.authored_block(*id).unwrap();
            record.validate().unwrap();
            assert_closed(&record);
            assert_collision(&loaded, *id);
            let report = evaluate_intent_shadow(&record);
            assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
            assert_eq!(unmatched(&report), 0, "{}", report.text);
        }
        json = LevelDocument::capture(&loaded, document.level_uuid, "Shadow").unwrap().to_json();
    }
}

fn assert_closed(record: &BlockRecord) {
    let body = record.body.as_ref().expect("topology stored a body");
    body.validate().expect("authoritative body is not closed");
    assert!(signed_volume(body) > 1.0e-8, "volume {}", signed_volume(body));
    let size = body.aabb_size();
    for axis in 0..3 {
        assert!((size[axis] - record.size_m[axis]).abs() <= 1.0e-3, "bounds axis {axis}");
    }
}

fn assert_collision(world: &SceneWorld, id: EntityId) {
    let record = world.authored_block(id).unwrap();
    let pose = world.entity_local_pose(id).unwrap();
    let solid = world
        .block_solids()
        .into_iter()
        .find(|solid| distance(solid.translation, pose.translation) < 1.0e-6)
        .expect("analytic box");
    assert_eq!(solid.size_m, record.size_m);
    let pushed = crate::keep_outside_box(pose.translation, solid, crate::FLY_COLLISION_RADIUS_M);
    assert!(distance(pose.translation, pushed) > 0.05, "the center was not kept outside the box");
    let outside = Vec3::new(
        pose.translation.x + record.size_m[0] * 0.5 + crate::FLY_COLLISION_RADIUS_M + 0.5,
        pose.translation.y,
        pose.translation.z,
    );
    let stayed = crate::keep_outside_box(outside, solid, crate::FLY_COLLISION_RADIUS_M);
    assert!(distance(outside, stayed) < 1.0e-9, "a point outside this solid was moved");
}

fn inspected(world: &SceneWorld, id: EntityId, field: crate::FieldId) -> String {
    world
        .inspect_entity(id)
        .unwrap()
        .sections
        .iter()
        .flat_map(|section| section.fields.iter())
        .find(|inspected| inspected.info.id == field)
        .unwrap()
        .display
        .clone()
}

fn body(world: &SceneWorld, id: EntityId) -> SolidBody {
    world.authored_block(id).unwrap().body.expect("stored body")
}

fn unmatched(report: &crate::IntentShadowReport) -> usize {
    report
        .text
        .lines()
        .find_map(|line| line.strip_prefix("unmatched semantic elements: "))
        .and_then(|text| text.parse().ok())
        .unwrap_or(usize::MAX)
}

fn signed_volume(body: &SolidBody) -> f64 {
    let mut sum = 0.0;
    for face in &body.faces {
        let positions = body.face_positions(face.id).unwrap();
        for index in 1..positions.len() - 1 {
            sum += triple(positions[0], positions[index], positions[index + 1]);
        }
    }
    sum / 6.0
}

fn triple(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let cross = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    cross[0] * c[0] + cross[1] * c[1] + cross[2] * c[2]
}

fn scale(normal: [f64; 3], distance: f64) -> [f64; 3] {
    [normal[0] * distance, normal[1] * distance, normal[2] * distance]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn distance(left: Vec3, right: Vec3) -> f64 {
    let gap = [left.x - right.x, left.y - right.y, left.z - right.z];
    (gap[0] * gap[0] + gap[1] * gap[1] + gap[2] * gap[2]).sqrt()
}
