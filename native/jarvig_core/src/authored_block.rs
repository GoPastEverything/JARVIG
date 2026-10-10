//! Authored Block v1. Create Block is a seed. Round and Split stay on the tape.
//! A stored body, a size-only block, and Plane keep the path they already had.

use std::path::{Path, PathBuf};

use crate::{
    authored_extrude_record, authored_split_conflict, block_surface_mesh, class_c_cache_extent, commit_authored_round, create_project_at, create_project_directories, empty_world_level,
    curve_catalog, fillet_for_chain, intent_authority_candidate, intent_authority_diagnostic, logical_edge_chain, mesh_from_body, observation_camera, parse_level, persistent_edge_token,
    realize_round_observation, replay_round, replay_rounds, round_curve_id, round_entry, round_entry_set, save_level_atomic, save_project_atomic, semantic_edge_bindings, semantic_edge_names, semantic_face_names,
    semantic_vertex_names, visible_authored_face, AuthoringError, AuthoringResult, BlockRecord, CurveElement, CurveKind, Fillet,
    EntityId, IntentAuthorityCandidate, IntentPayload, LevelDocument, ProjectDocument, ProjectTemplate, SceneWorld, SolidBody, Vec3, CURVE_EDITED_RADIUS_M,
    CURVE_ERROR_PX, CURVE_FOV, CURVE_RADIUS_M, LEVEL_BLOCK_VERSION, PROJECT_FORMAT_VERSION,
};

const VISUAL_RADIUS_M: f64 = 0.15;
/// Vertical edge at +X, +Z. The window-shot camera looks toward −X and −Z, so this edge faces it.
const VISUAL_EDGE: &str = "E:seed-edge/7";
const VISUAL_SHOW_M: f64 = 0.35;

fn fresh() -> (LevelDocument, SceneWorld) {
    let document = empty_world_level();
    let world = document.instantiate().expect("empty world");
    (document, world)
}

fn place(world: &mut SceneWorld, record: BlockRecord) -> EntityId {
    world.create_block(Vec3::new(0.0, 1.0, -4.0), record).expect("create block")
}

fn seed_record() -> BlockRecord {
    BlockRecord::authored_seed([2.0, 2.0, 2.0]).expect("authored seed")
}

fn reload(document: &LevelDocument, world: &SceneWorld) -> (String, BlockRecord) {
    let saved = LevelDocument::capture(world, document.level_uuid, "Authored Block").expect("capture");
    assert_eq!(saved.format_version, LEVEL_BLOCK_VERSION);
    let text = saved.to_json();
    let loaded = parse_level(&text).expect("parse").instantiate().expect("instantiate");
    let again = loaded
        .entity_outline()
        .into_iter()
        .find_map(|row| loaded.authored_block(row.uuid).filter(|record| record.has_authored_seed()))
        .expect("authored block reloaded");
    (text, again)
}

fn rounds(record: &BlockRecord) -> Vec<(Vec<String>, f64)> {
    record
        .intent
        .iter()
        .filter_map(|entry| {
            let IntentPayload::Round { radius_m } = entry.payload else { return None };
            let tokens = entry.groups.clone().unwrap_or_default().into_iter().flatten().collect();
            Some((tokens, radius_m))
        })
        .collect()
}

fn splits(record: &BlockRecord) -> Vec<Vec<String>> {
    record
        .intent
        .iter()
        .filter_map(|entry| match entry.payload {
            IntentPayload::Split => Some(entry.groups.clone().unwrap_or_default().into_iter().flatten().collect()),
            _ => None,
        })
        .collect()
}

fn binding(record: &BlockRecord, token: &str) -> u32 {
    semantic_edge_bindings(record)
        .expect("bindings")
        .into_iter()
        .find(|(name, _)| name == token)
        .map(|(_, id)| id)
        .unwrap_or_else(|| panic!("{token} is not bound"))
}

fn assert_constructor_ids(record: &BlockRecord) {
    for axis in 0..6u32 {
        let names = crate::semantic_shadow::face_provenance_names(record, axis + 1);
        assert_eq!(names, vec![format!("F:seed/{axis}")]);
    }
    for slot in 0..12u32 {
        assert_eq!(binding(record, &format!("E:seed-edge/{slot}")), slot + 9);
    }
}

fn assert_no_body(text: &str, record: &BlockRecord) {
    assert!(record.body.is_none());
    assert!(!text.contains("\"body\""));
    assert!(text.contains("\"op\": \"seed\""));
    assert!(text.contains("\"version\": 4"));
    assert!(text.contains("\"seed_size_m\""));
    assert_eq!(record.seed_size_m, Some([2.0, 2.0, 2.0]));
    assert_eq!(intent_authority_diagnostic(record), "Intent Authority: ELIGIBLE");
    assert!(record.history.is_empty());
    assert!(record.steps.is_empty());
    assert_eq!(record.intent.first().map(|entry| entry.groups.is_none() && matches!(entry.payload, IntentPayload::Seed)), Some(true));
}

fn candidate_body(record: &BlockRecord) -> SolidBody {
    match intent_authority_candidate(record) {
        IntentAuthorityCandidate::Reconstructable(body) => body,
        IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
    }
}

fn report(label: &str, text: &str, record: &BlockRecord) {
    let edges: Vec<_> = (0..12u32).map(|slot| format!("E:seed-edge/{slot}={}", binding(record, &format!("E:seed-edge/{slot}")))).collect();
    println!("AUTHORED_BLOCK {label}");
    println!("diagnostic {}", intent_authority_diagnostic(record));
    println!("body {}", record.body.is_some());
    println!("seed_size_m {:?}", record.seed_size_m);
    println!("size_m {:?}", record.size_m);
    println!("rounds {:?}", rounds(record));
    println!("splits {:?}", splits(record));
    println!("semantic {}", edges.join(" "));
    println!("AUTHORED_BLOCK_RECORD_BEGIN");
    println!("{text}");
    println!("AUTHORED_BLOCK_RECORD_END");
}

#[test]
fn a_plain_authored_block_reloads_without_a_body() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let live = world.authored_block(id).unwrap();
    assert_constructor_ids(&live);
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
    let surface = block_surface_mesh([2.0, 2.0, 2.0], [0.0; 6], 0.0);
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.index_count(), 36);
    assert_eq!(mesh, &surface);
    assert!(live.body.is_none());
    let before = semantic_edge_bindings(&live).unwrap();
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_constructor_ids(&loaded);
    assert_eq!(semantic_edge_bindings(&loaded).unwrap(), before);
    assert_eq!(intent_authority_diagnostic(&loaded), intent_authority_diagnostic(&live));
    assert_eq!(loaded.seed_size_m, live.seed_size_m);
    assert_eq!(loaded.size_m, live.size_m);
    report("plain-seed", &text, &loaded);
}

#[test]
fn ordinary_round_reloads_and_a_second_apply_replaces_the_radius() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let open = world.authored_block(id).unwrap();
    let token = persistent_edge_token(&candidate_body(&open), &semantic_edge_bindings(&open).unwrap(), 9).unwrap();
    assert_eq!(token, "E:seed-edge/0");
    let once = commit_authored_round(&open, round_entry(&token, CURVE_RADIUS_M)).expect("first round");
    assert_eq!(rounds(&once).len(), 1);
    let replaced = commit_authored_round(&once, round_entry(&token, CURVE_EDITED_RADIUS_M)).expect("replace radius");
    assert_eq!(rounds(&replaced), vec![(vec![token.clone()], CURVE_EDITED_RADIUS_M)]);
    let added = commit_authored_round(&replaced, round_entry("E:seed-edge/1", CURVE_RADIUS_M)).expect("different edge appends");
    assert_eq!(rounds(&added).len(), 2);
    world.replace_block_record(id, replaced).expect("install round");
    let other = world
        .create_block(Vec3::new(2.5, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap())
        .expect("second object");
    assert!(!world.authored_block(other).unwrap().has_authored_seed());
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_eq!(rounds(&loaded), vec![(vec![token.clone()], CURVE_EDITED_RADIUS_M)]);
    assert_eq!(binding(&loaded, &token), 9);
    assert_constructor_ids(&loaded);
    let replayed = replay_round(&loaded).unwrap();
    assert_eq!(replayed.token, token);
    assert_eq!(replayed.edge_id, 9);
    assert_eq!(replayed.radius_m, CURVE_EDITED_RADIUS_M);
    assert_eq!(loaded.size_m, class_c_cache_extent(&loaded).unwrap());
    assert!(loaded.size_m.iter().all(|axis| *axis + 1.0e-6 >= 2.0));
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
    let surface = block_surface_mesh([2.0, 2.0, 2.0], [0.0; 6], 0.0);
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.index_count(), 36);
    assert_eq!(mesh, &surface);
    report("round-replaced", &text, &loaded);
}

#[test]
fn split_then_round_keeps_the_unrelated_edge() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    assert_eq!(world.split_block_edge(id, 9).unwrap(), AuthoringResult::Applied);
    let split = world.authored_block(id).unwrap();
    assert!(split.body.is_none());
    assert_eq!(binding(&split, "E:seed-edge/1"), 10);
    assert_eq!(splits(&split).len(), 1);
    let token = persistent_edge_token(&candidate_body(&split), &semantic_edge_bindings(&split).unwrap(), 10).unwrap();
    assert_eq!(token, "E:seed-edge/1");
    let rounded = commit_authored_round(&split, round_entry(&token, CURVE_RADIUS_M)).expect("round after split");
    world.replace_block_record(id, rounded).expect("install");
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_eq!(binding(&loaded, "E:seed-edge/1"), 10);
    assert_eq!(rounds(&loaded), vec![(vec![token], CURVE_RADIUS_M)]);
    assert_eq!(splits(&loaded), splits(&world.authored_block(id).unwrap()));
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
    assert_eq!(mesh.index_count() / 3, 14);
    assert_eq!(mesh, &mesh_from_body(&candidate_body(&world.authored_block(id).unwrap())));
    report("split-then-round", &text, &loaded);
}

#[test]
fn round_then_unrelated_split_survives_and_the_rounded_edge_conflicts() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let open = world.authored_block(id).unwrap();
    let rounded = commit_authored_round(&open, round_entry("E:seed-edge/0", CURVE_RADIUS_M)).expect("round");
    world.replace_block_record(id, rounded).expect("install round");
    assert_eq!(world.split_block_edge(id, 10).unwrap(), AuthoringResult::Applied);
    let edited = world.authored_block(id).unwrap();
    assert!(edited.body.is_none());
    assert_eq!(rounds(&edited), vec![(vec!["E:seed-edge/0".to_string()], CURVE_RADIUS_M)]);
    assert_eq!(binding(&edited, "E:seed-edge/0"), 9);
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_eq!(rounds(&loaded), rounds(&edited));
    assert_eq!(binding(&loaded, "E:seed-edge/0"), 9);
    let replayed = replay_round(&loaded).unwrap();
    assert_eq!(replayed.token, "E:seed-edge/0");
    assert_eq!(replayed.edge_id, 9);
    assert_eq!(
        authored_split_conflict(&loaded, replayed.edge_id).as_deref(),
        Some("Split conflicts with the round on E:seed-edge/0")
    );
    let (copy_document, mut copy) = fresh();
    let copy_id = place(&mut copy, loaded.clone());
    let before = copy.authored_block(copy_id).unwrap();
    assert!(matches!(copy.split_block_edge(copy_id, replayed.edge_id), Err(AuthoringError::InvalidOperation)));
    assert_eq!(copy.authored_block(copy_id).unwrap(), before);
    let (copy_text, copy_loaded) = reload(&copy_document, &copy);
    assert_eq!(rounds(&copy_loaded), rounds(&before));
    assert_eq!(splits(&copy_loaded), splits(&before));
    assert!(copy_loaded.body.is_none());
    assert!(!copy_text.contains("\"body\""));
    report("round-then-split", &text, &loaded);
}

#[test]
fn legacy_stored_body_and_size_box_stay_on_the_old_path() {
    let plain = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
    let refused = commit_authored_round(&plain, round_entry("E:seed-edge/0", CURVE_RADIUS_M)).unwrap_err();
    assert!(refused.contains("legacy concrete-only history has no semantic program"), "{refused}");
    assert!(plain.intent.is_empty());
    assert!(plain.body.is_none());
    assert!(!plain.has_authored_seed());
    assert_eq!(intent_authority_diagnostic(&plain), "Intent Authority: INELIGIBLE — legacy concrete-only history has no semantic program");
    let plane = BlockRecord::plane(2.0, 2.0).unwrap();
    assert!(!plane.has_authored_seed());
    assert!(intent_authority_diagnostic(&plane).starts_with("Intent Authority: INELIGIBLE"));

    let (document, mut world) = fresh();
    let legacy = place(&mut world, plain);
    assert_eq!(world.split_block_edge(legacy, 9).unwrap(), AuthoringResult::Applied);
    let stored = world.authored_block(legacy).unwrap();
    assert!(stored.body.is_some());
    assert!(!stored.has_authored_seed());
    let saved = LevelDocument::capture(&world, document.level_uuid, "Legacy").unwrap().to_json();
    assert!(saved.contains("\"body\""));
    assert!(!saved.contains("\"op\": \"seed\""));
    assert!(!saved.contains("\"version\": 4"));

    let (document, mut world) = fresh();
    let id = place(&mut world, BlockRecord::standard([2.0, 2.0, 2.0]).unwrap());
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap().clone();
    let surface = block_surface_mesh([2.0, 2.0, 2.0], [0.0; 6], 0.0);
    assert_eq!(mesh.index_count(), surface.index_count());
    assert_eq!(mesh.vertex_count(), surface.vertex_count());
    assert_eq!(mesh, surface);
    let _ = document;
}

#[test]
fn authored_mutators_do_not_store_a_body() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let before = world.authored_block(id).unwrap();
    assert!(matches!(world.set_block_extent(id, 0, 3.0), Err(AuthoringError::InvalidOperation)));
    assert!(matches!(world.subdivide_block_face(id, 1, 2, 2), Err(AuthoringError::InvalidOperation)));
    assert!(matches!(world.reset_block_shape(id), Err(AuthoringError::InvalidOperation)));
    assert!(matches!(world.mirror_block(id, 0), Err(AuthoringError::InvalidOperation)));
    assert_eq!(world.authored_block(id).unwrap(), before);
    assert!(world.authored_block(id).unwrap().body.is_none());
    let _ = document;
}

#[test]
fn observation_geometry_does_not_become_the_record() {
    let record = commit_authored_round(&seed_record(), round_entry("E:seed-edge/0", VISUAL_RADIUS_M)).expect("visual radius");
    assert!(record.body.is_none());
    let replayed = replay_round(&record).unwrap();
    let camera = observation_camera(&replayed.fillet, 2.0, 960.0, 540.0, CURVE_FOV, CURVE_ERROR_PX).unwrap();
    let product = realize_round_observation(&record, &camera, "authored-block", 1).expect("observation");
    let solid = crate::realize_round_solid(&record, &camera, "authored-block", 1).expect("shaded round");
    assert!(solid.triangles_constructed > 12, "shaded triangles {}", solid.triangles_constructed);
    assert!(product.triangles_constructed > 0);
    assert_eq!(product.triangles_discarded_after_construction, 0);
    assert!(!product.record_body_consulted);
    assert!(!product.object_mesh_consulted);
    assert!(!product.replay_consulted_size_m);
    assert!(record.body.is_none());
    assert_eq!(rounds(&record), vec![(vec!["E:seed-edge/0".to_string()], VISUAL_RADIUS_M)]);
    let (document, mut world) = fresh();
    let id = place(&mut world, record.clone());
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
    assert_eq!(mesh.index_count(), 36);
    assert_ne!(product.mesh.index_count(), mesh.index_count());
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_eq!(loaded, record);
}

#[test]
fn a_seed_face_pick_names_the_same_face_after_reload_and_tessellation() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let live = world.authored_block(id).unwrap();
    let body = candidate_body(&live);
    let rays = [
        ([3.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 1, "F:seed/0"),
        ([-3.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2, "F:seed/1"),
        ([0.0, 3.0, 0.0], [0.0, -1.0, 0.0], 3, "F:seed/2"),
        ([0.0, -3.0, 0.0], [0.0, 1.0, 0.0], 4, "F:seed/3"),
        ([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 5, "F:seed/4"),
        ([0.0, 0.0, -3.0], [0.0, 0.0, 1.0], 6, "F:seed/5"),
    ];
    for (origin, direction, face, token) in rays {
        let pick = body.pick_face(origin, direction).unwrap_or_else(|| panic!("{token} missed"));
        assert_eq!(pick.id, face, "{token}");
        assert_eq!(semantic_face_names(&live, pick.id), vec![token.to_string()]);
    }
    let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
    assert_eq!(mesh.index_count() / 3, 12);
    assert_ne!(1, mesh.index_count() / 3);
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    let again = candidate_body(&loaded);
    let pick = again.pick_face([3.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).expect("+X");
    assert_eq!(pick.id, 1);
    assert_eq!(semantic_face_names(&loaded, pick.id), vec!["F:seed/0".to_string()]);

    let rounded = commit_authored_round(&loaded, round_entry("E:seed-edge/0", VISUAL_RADIUS_M)).expect("round");
    assert!(rounded.body.is_none());
    let replayed = replay_round(&rounded).expect("replay");
    let near = observation_camera(&replayed.fillet, 2.0, 1280.0, 720.0, CURVE_FOV, CURVE_ERROR_PX).expect("near");
    let far = observation_camera(&replayed.fillet, 40.0, 1280.0, 720.0, CURVE_FOV, CURVE_ERROR_PX).expect("far");
    let near_solid = crate::realize_round_solid(&rounded, &near, "face-near", 1).expect("near solid");
    let far_solid = crate::realize_round_solid(&rounded, &far, "face-far", 1).expect("far solid");
    assert_ne!(near_solid.triangles_constructed, far_solid.triangles_constructed);
    assert!(rounded.body.is_none());
    let planar = candidate_body(&rounded);
    let pick = planar.pick_face([3.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).expect("+X after round");
    assert_eq!(pick.id, 1);
    assert_eq!(semantic_face_names(&rounded, pick.id), vec!["F:seed/0".to_string()]);
    assert_ne!(pick.id, near_solid.triangles_constructed);
    assert_ne!(pick.id, far_solid.triangles_constructed);
}

#[test]
fn a_seed_face_extrude_appends_the_face_and_keeps_no_body() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let before = world.authored_block(id).unwrap();
    assert!(matches!(world.extrude_authored_faces(id, &[1], [0.0, 0.0, 0.0]), Err(AuthoringError::InvalidValue)));
    assert!(matches!(world.extrude_authored_faces(id, &[1], [f64::NAN, 0.0, 0.0]), Err(AuthoringError::InvalidValue)));
    assert!(matches!(world.extrude_authored_faces(id, &[], [1.0, 0.0, 0.0]), Err(AuthoringError::InvalidValue)));
    assert!(matches!(world.extrude_block_faces(id, &[1], [1.0, 0.0, 0.0]), Err(AuthoringError::InvalidOperation)));
    assert_eq!(world.authored_block(id).unwrap(), before);

    world.extrude_authored_faces(id, &[1], [1.0, 0.0, 0.0]).expect("extrude");
    let record = world.authored_block(id).unwrap();
    assert!(record.body.is_none());
    assert_eq!(intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
    assert_eq!(record.history.len(), 0);
    let extrude = record.intent.last().expect("extrude entry");
    assert!(matches!(extrude.payload, IntentPayload::Extrude { delta_m } if delta_m == [1.0, 0.0, 0.0]));
    assert_eq!(extrude.groups.clone(), Some(vec![vec!["F:seed/0".to_string()]]));
    for axis in 0..3 {
        let expected = if axis == 0 { 3.0 } else { 2.0 };
        assert!((record.size_m[axis] - expected).abs() < 1.0e-9, "{:?}", record.size_m);
    }
    let pose = world.entity_local_pose(id).expect("pose");
    assert!((pose.translation.x - 0.5).abs() < 1.0e-9, "{}", pose.translation.x);
    assert!((pose.translation.y - 1.0).abs() < 1.0e-9);
    assert!((pose.translation.z + 4.0).abs() < 1.0e-9);
    let body = candidate_body(&record);
    assert!(body.faces.len() > 6);
    assert!(body.faces.iter().any(|face| face.id == 1));
    let cap_names = semantic_face_names(&record, 1);
    assert!(cap_names.iter().any(|name| name == "F:seed/0"), "{cap_names:?}");
    assert!(cap_names.iter().any(|name| name == "F:cap(F:seed/0)"), "{cap_names:?}");
    let triangles = {
        let mesh = world.meshes().get(world.object_mesh(id).unwrap()).unwrap();
        assert_eq!(mesh.index_count(), mesh_from_body(&body).index_count());
        let triangles = mesh.index_count() / 3;
        assert_ne!(triangles, 12);
        assert_ne!(triangles, 1);
        triangles
    };
    let side_name = body
        .faces
        .iter()
        .map(|face| face.id)
        .find_map(|face| semantic_face_names(&record, face).into_iter().find(|name| name.starts_with("F:side(F:seed/0,")))
        .expect("side face name");
    let tape = record.intent.clone();
    assert!(matches!(world.extrude_authored_faces(id, &[999], [0.0, 1.0, 0.0]), Err(AuthoringError::InvalidOperation)));
    assert_eq!(world.authored_block(id).unwrap().intent, tape);
    assert!(world.authored_block(id).unwrap().body.is_none());

    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert!(text.contains("\"op\": \"extrude\""));
    assert!(text.contains("F:seed/0"));
    assert_eq!(loaded.intent.last().and_then(|entry| entry.groups.clone()), Some(vec![vec!["F:seed/0".to_string()]]));
    let again = candidate_body(&loaded);
    let pick = again.pick_face([4.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).expect("+X after extrude");
    assert_eq!(pick.id, 1);
    let loaded_cap = semantic_face_names(&loaded, pick.id);
    assert!(loaded_cap.iter().any(|name| name == "F:seed/0"), "{loaded_cap:?}");
    assert!(again.faces.iter().any(|face| semantic_face_names(&loaded, face.id).iter().any(|name| name == &side_name)));
    assert_ne!(pick.id, triangles);

    let (stored_document, mut stored_world) = fresh();
    let mut stored = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
    stored.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
    let stored_id = place(&mut stored_world, stored);
    stored_world.extrude_block_faces(stored_id, &[1], [1.0, 0.0, 0.0]).expect("stored extrude");
    assert!(stored_world.authored_block(stored_id).unwrap().body.is_some());
    assert!(matches!(stored_world.extrude_authored_faces(stored_id, &[1], [1.0, 0.0, 0.0]), Err(AuthoringError::InvalidOperation)));
    assert!(stored_world.authored_block(stored_id).unwrap().body.is_some());
    let _ = stored_document;
}

#[test]
fn a_generated_face_extrudes_again_and_a_generated_edge_rounds_and_splits() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    world.extrude_authored_faces(id, &[1], [1.0, 0.0, 0.0]).expect("first extrude");
    world.extrude_authored_faces(id, &[1], [1.0, 0.0, 0.0]).expect("cap extrude");
    let after_cap = world.authored_block(id).unwrap();
    assert!(after_cap.body.is_none());
    assert!(semantic_face_names(&after_cap, 1).iter().any(|name| name == "F:seed/0"));
    let cap_body = candidate_body(&after_cap);
    let side_id = cap_body
        .faces
        .iter()
        .map(|face| face.id)
        .find(|face| semantic_face_names(&after_cap, *face).iter().any(|name| name.starts_with("F:side(F:seed/0,")))
        .expect("side");
    let side_name = semantic_face_names(&after_cap, side_id).into_iter().find(|name| name.starts_with("F:side(")).expect("side token");
    let normal = cap_body.unit_normal(side_id).expect("side normal");
    let delta = [normal[0] * 0.5, normal[1] * 0.5, normal[2] * 0.5];
    world.extrude_authored_faces(id, &[side_id], delta).expect("side extrude");
    let sided = world.authored_block(id).unwrap();
    assert!(sided.body.is_none());
    assert_eq!(sided.history.len(), 0);
    assert_eq!(intent_authority_diagnostic(&sided), "Intent Authority: ELIGIBLE");
    assert_eq!(sided.intent.last().and_then(|entry| entry.groups.clone()), Some(vec![vec![side_name.clone()]]));
    assert!(semantic_face_names(&sided, side_id).iter().any(|name| name == &side_name));
    let sided_body = candidate_body(&sided);
    assert!(sided_body.faces.iter().any(|face| face.id != side_id && semantic_face_names(&sided, face.id).iter().any(|name| name.contains(&side_name))));

    let bindings = semantic_edge_bindings(&sided).expect("edge names");
    let mut round_edge = None;
    let mut split_edge = None;
    for edge in &sided_body.edges {
        let Ok(token) = persistent_edge_token(&sided_body, &bindings, edge.id) else { continue };
        if token.starts_with("E:seed-edge/") {
            continue;
        }
        let Ok(chain) = logical_edge_chain(&sided_body, edge.id) else { continue };
        if fillet_for_chain(&sided_body, &chain, 0.05).is_err() {
            continue;
        }
        if round_edge.is_none() {
            round_edge = Some((edge.id, token, chain));
            continue;
        }
        let (_, _, round_chain) = round_edge.as_ref().expect("round edge");
        if chain.iter().any(|id| round_chain.contains(id)) {
            continue;
        }
        let round_vertices: Vec<u32> = round_chain.iter().filter_map(|id| sided_body.edges.iter().find(|edge| edge.id == *id)).flat_map(|edge| [edge.a, edge.b]).collect();
        let shares_vertex = sided_body.edges.iter().find(|candidate| candidate.id == edge.id).is_some_and(|candidate| round_vertices.contains(&candidate.a) || round_vertices.contains(&candidate.b));
        if shares_vertex {
            continue;
        }
        split_edge = Some((edge.id, token));
        break;
    }
    let (round_id, round_token, _) = round_edge.expect("generated edge that can round");
    let (split_id, split_token) = split_edge.expect("generated edge that can split");
    let rounded = commit_authored_round(&sided, round_entry(&round_token, 0.05)).expect("round generated edge");
    assert!(rounded.body.is_none());
    assert!(rounded.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Round { .. }) && entry.groups.as_ref().is_some_and(|groups| groups.iter().any(|group| group == &vec![round_token.clone()]))));
    world.replace_block_record(id, rounded).expect("install round");
    world.split_block_edge(id, split_id).expect("split generated edge");
    let split = world.authored_block(id).unwrap();
    assert!(split.body.is_none());
    assert_eq!(intent_authority_diagnostic(&split), "Intent Authority: ELIGIBLE");
    assert!(split.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Split) && entry.groups.as_ref().is_some_and(|groups| groups.iter().any(|group| group == &vec![split_token.clone()]))));
    let split_body = candidate_body(&split);
    let vertex_token = split_body
        .vertices
        .iter()
        .find_map(|vertex| semantic_vertex_names(&split, vertex.id).into_iter().find(|name| name.starts_with("V:split-at(")))
        .expect("split vertex");

    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert!(!text.contains("\"body\""));
    let loaded_body = candidate_body(&loaded);
    assert!(loaded_body.faces.iter().any(|face| semantic_face_names(&loaded, face.id).iter().any(|name| name == &side_name)));
    assert!(semantic_face_names(&loaded, 1).iter().any(|name| name == "F:seed/0"));
    let loaded_edges = semantic_edge_bindings(&loaded).expect("reloaded edges");
    assert!(loaded_edges.iter().any(|(token, _)| token == &round_token));
    assert!(loaded_edges.iter().any(|(token, _)| token == &split_token));
    assert!(loaded_body.vertices.iter().any(|vertex| semantic_vertex_names(&loaded, vertex.id).iter().any(|name| name == &vertex_token)));
    assert_eq!(persistent_edge_token(&loaded_body, &loaded_edges, round_id).as_deref(), Ok(round_token.as_str()));
    println!("CHAIN cap=F:seed/0 side={side_name} round={round_token} split={split_token} vertex={vertex_token}");
}

#[test]
fn a_face_loop_rounds_together_and_a_whole_box_names_the_corner() {
    let record = seed_record();
    let body = candidate_body(&record);
    let bindings = semantic_edge_bindings(&record).expect("bindings");
    let pair = [11u32, 16];
    let pair_tokens: Vec<String> = pair.iter().map(|edge| persistent_edge_token(&body, &bindings, *edge).unwrap()).collect();
    let paired = commit_authored_round(&record, round_entry_set(&pair_tokens, 0.05)).expect("two edges");
    assert!(paired.body.is_none());
    let round = paired.intent.iter().rev().find(|entry| matches!(entry.payload, IntentPayload::Round { .. })).expect("round");
    assert_eq!(round.groups.as_ref().map(|groups| groups.len()), Some(2));

    let loop_edges = [11u32, 12, 15, 16];
    let loop_tokens: Vec<String> = loop_edges.iter().map(|edge| persistent_edge_token(&body, &bindings, *edge).unwrap()).collect();
    let looped = commit_authored_round(&record, round_entry_set(&loop_tokens, 0.05)).expect("face loop");
    assert!(looped.body.is_none());
    let face_round = looped.intent.iter().rev().find(|entry| matches!(entry.payload, IntentPayload::Round { .. })).expect("loop round");
    assert_eq!(face_round.groups.as_ref().map(|groups| groups.len()), Some(4));

    let mut all = Vec::new();
    for edge in &body.edges {
        let token = persistent_edge_token(&body, &bindings, edge.id).unwrap();
        if !all.contains(&token) {
            all.push(token);
        }
    }
    assert_eq!(all.len(), 12);
    let rounded = commit_authored_round(&record, round_entry_set(&all, 0.05)).expect("whole box");
    assert!(rounded.body.is_none());
    assert_eq!(rounded.intent.len(), 2);
    assert_eq!(intent_authority_diagnostic(&rounded), "Intent Authority: ELIGIBLE");
}

#[test]
fn a_round_set_mints_curves_and_a_box_corner_closes() {
    let record = seed_record();
    let body = candidate_body(&record);
    let bindings = semantic_edge_bindings(&record).expect("bindings");
    let pair = [11u32, 16];
    let pair_tokens: Vec<String> = pair.iter().map(|edge| persistent_edge_token(&body, &bindings, *edge).unwrap()).collect();
    let mut sorted = pair_tokens.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["E:seed-edge/2".to_string(), "E:seed-edge/7".to_string()]);
    let paired = commit_authored_round(&record, round_entry_set(&pair_tokens, 0.05)).expect("two edges");
    assert!(paired.body.is_none());
    assert_eq!(paired.intent.len(), 2);
    let catalog = catalog_of(&paired);
    let expected = [
        "F:fillet(E:seed-edge/2)",
        "F:fillet(E:seed-edge/7)",
        "E:fillet-a(E:seed-edge/2)",
        "E:fillet-b(E:seed-edge/2)",
        "E:fillet-a(E:seed-edge/7)",
        "E:fillet-b(E:seed-edge/7)",
        "V:fillet-start(E:seed-edge/2)",
        "V:fillet-end(E:seed-edge/2)",
        "V:fillet-start(E:seed-edge/7)",
        "V:fillet-end(E:seed-edge/7)",
        "F:corner(E:seed-edge/2,E:seed-edge/7)",
    ];
    for token in expected {
        let element = catalog.iter().find(|item| item.token == token).unwrap_or_else(|| panic!("missing {token}"));
        assert_eq!(element.id, round_curve_id(token, curve_slot(token)), "{token}");
        assert!(!token.contains("division") && !token.contains("segment"), "{token}");
        let named = match element.kind {
            CurveKind::Face => semantic_face_names(&paired, element.id),
            CurveKind::Edge => semantic_edge_names(&paired, element.id),
            CurveKind::Vertex => semantic_vertex_names(&paired, element.id),
        };
        assert!(named.iter().any(|name| name == token), "{token} named {named:?}");
    }
    let mut ids: Vec<u32> = catalog.iter().map(|item| item.id).collect();
    ids.sort_unstable();
    let mut unique = ids.clone();
    unique.dedup();
    assert_eq!(ids.len(), unique.len());

    let (document, mut world) = fresh();
    let id = place(&mut world, paired);
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    let loaded_catalog = catalog_of(&loaded);
    for element in &catalog {
        let again = loaded_catalog.iter().find(|item| item.token == element.token).unwrap_or_else(|| panic!("reload lost {}", element.token));
        assert_eq!(again.id, element.id, "{}", element.token);
    }

    let edited = commit_authored_round(&loaded, round_entry_set(&pair_tokens, 0.04)).expect("radius");
    assert_eq!(edited.intent.len(), 2);
    assert!(matches!(edited.intent.last().map(|entry| &entry.payload), Some(IntentPayload::Round { radius_m }) if (*radius_m - 0.04).abs() < 1.0e-9));
    let edited_catalog = catalog_of(&edited);
    for element in &catalog {
        let again = edited_catalog.iter().find(|item| item.token == element.token).unwrap_or_else(|| panic!("radius lost {}", element.token));
        assert_eq!(again.id, element.id, "{}", element.token);
    }

    let mut all = Vec::new();
    for edge in &body.edges {
        let token = persistent_edge_token(&body, &bindings, edge.id).unwrap();
        if !all.contains(&token) {
            all.push(token);
        }
    }
    assert_eq!(all.len(), 12);
    let whole = commit_authored_round(&record, round_entry_set(&all, 0.05)).expect("twelve edges");
    assert!(whole.body.is_none());
    assert_eq!(whole.intent.len(), 2);
    let corners: Vec<_> = catalog_of(&whole).into_iter().filter(|item| item.sources.len() == 3 && item.token.starts_with("F:corner(")).collect();
    assert_eq!(corners.len(), 8, "{:?}", corners.iter().map(|item| item.token.clone()).collect::<Vec<_>>());
    let max = corners.iter().find(|item| item.outline.iter().all(|point| point_distance(*point, [1.0, 1.0, 1.0]) < 0.2)).expect("max corner");
    assert!(max.samples.iter().all(|point| point_distance(*point, [1.0, 1.0, 1.0]) > 0.02));
    let scale = 0.05 / 3.0_f64.sqrt();
    let crown = [0.95 + scale, 0.95 + scale, 0.95 + scale];
    assert!(max.samples.iter().any(|point| point_distance(*point, crown) < 0.01), "crown missing near {crown:?}");
    world.replace_block_record(id, whole).expect("install whole");
    let (text, loaded_whole) = reload(&document, &world);
    assert_no_body(&text, &loaded_whole);
    assert_eq!(catalog_of(&loaded_whole).iter().filter(|item| item.sources.len() == 3).count(), 8);
}

/// A point on each adjacent face, mid-edge and inside the fillet strip.
fn fillet_strip_rays(fillet: &Fillet) -> [([f64; 3], [f64; 3]); 2] {
    let mid = pick_add(fillet.origin, pick_scale(fillet.direction, fillet.length_m * 0.5));
    let ray = |normal: [f64; 3], inward: [f64; 3]| {
        let point = pick_add(mid, pick_scale(inward, fillet.tangent_m * 0.45));
        let origin = pick_add(point, pick_scale(normal, 1.5));
        (origin, pick_scale(normal, -1.0))
    };
    [ray(fillet.normal0, fillet.inward0), ray(fillet.normal1, fillet.inward1)]
}

fn pick_add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn pick_scale(value: [f64; 3], scale: f64) -> [f64; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn pick_dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn pick_cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn pick_unit(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = pick_dot(value, value).sqrt();
    (length > 1.0e-12).then(|| pick_scale(value, 1.0 / length))
}

fn face_centroid(body: &SolidBody, face: u32) -> Option<[f64; 3]> {
    let positions = body.face_positions(face)?;
    let count = positions.len() as f64;
    let sum = positions.into_iter().fold([0.0; 3], pick_add);
    (count > 0.0).then(|| pick_scale(sum, 1.0 / count))
}

fn aabb_center(body: &SolidBody) -> [f64; 3] {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    for vertex in &body.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex.position[axis]);
            max[axis] = max[axis].max(vertex.position[axis]);
        }
    }
    pick_scale(pick_add(min, max), 0.5)
}

/// Rays from outside through every curved-face sample, plus the flat interior of every planar face.
/// A strip midpoint selects that fillet even when the pick mesh is empty.
fn check_curved_face_picks(label: &str, record: &BlockRecord, failures: &mut Vec<String>) {
    let rounds = replay_rounds(record).unwrap_or_else(|error| panic!("{label} replay: {error}"));
    let body = candidate_body(record);
    let tokens: Vec<String> = rounds.iter().map(|round| round.token.clone()).collect();
    let fillets: Vec<Fillet> = rounds.iter().map(|round| round.fillet.clone()).collect();
    let curves = catalog_of(record);
    let curve_faces: Vec<&CurveElement> = curves.iter().filter(|element| element.kind == CurveKind::Face).collect();
    if curve_faces.is_empty() {
        failures.push(format!("{label}: catalog minted no curved faces"));
        return;
    }
    let curve_ids: Vec<u32> = curve_faces.iter().map(|element| element.id).collect();
    let center = aabb_center(&body);
    let mut curved_rays = 0u32;
    for element in &curve_faces {
        let mut index = 0;
        while index + 2 < element.samples.len() {
            let triangle = [element.samples[index], element.samples[index + 1], element.samples[index + 2]];
            index += 3;
            let edge_u = [triangle[1][0] - triangle[0][0], triangle[1][1] - triangle[0][1], triangle[1][2] - triangle[0][2]];
            let edge_v = [triangle[2][0] - triangle[0][0], triangle[2][1] - triangle[0][1], triangle[2][2] - triangle[0][2]];
            let mut normal = pick_cross(edge_u, edge_v);
            let centroid = pick_scale(pick_add(pick_add(triangle[0], triangle[1]), triangle[2]), 1.0 / 3.0);
            if pick_dot(normal, [centroid[0] - center[0], centroid[1] - center[1], centroid[2] - center[2]]) < 0.0 {
                normal = pick_scale(normal, -1.0);
            }
            let Some(outward) = pick_unit(normal) else { continue };
            curved_rays += 1;
            let origin = pick_add(centroid, pick_scale(outward, 0.45));
            let hit = visible_authored_face(&body, &tokens, &fillets, &curves, origin, pick_scale(outward, -1.0));
            match hit {
                Some(pick) if curve_ids.contains(&pick.id) => {}
                Some(pick) => failures.push(format!(
                    "{label}: {} sample {index} selected planar face {} instead of a curved face",
                    element.token, pick.id
                )),
                None => failures.push(format!("{label}: {} sample {index} selected nothing", element.token)),
            }
        }
        let centroid = element
            .samples
            .iter()
            .copied()
            .fold([0.0; 3], pick_add);
        let count = element.samples.len() as f64;
        if count < 3.0 {
            continue;
        }
        let centroid = pick_scale(centroid, 1.0 / count);
        let eye = [0.55, 3.4, centroid[2] + 1.6];
        let toward = [centroid[0] - eye[0], centroid[1] - eye[1], centroid[2] - eye[2]];
        curved_rays += 1;
        let hit = visible_authored_face(&body, &tokens, &fillets, &curves, eye, toward);
        match hit {
            Some(pick) if curve_ids.contains(&pick.id) => {}
            Some(pick) => failures.push(format!(
                "{label}: camera through {} selected planar face {} instead of a curved face",
                element.token, pick.id
            )),
            None => failures.push(format!("{label}: camera through {} selected nothing", element.token)),
        }
    }
    if curved_rays == 0 {
        failures.push(format!("{label}: curved faces had no samples"));
    }
    for face in &body.faces {
        let Some(normal) = body.unit_normal(face.id) else { continue };
        let Some(centroid) = face_centroid(&body, face.id) else { continue };
        let origin = pick_add(centroid, pick_scale(normal, 2.0));
        let hit = visible_authored_face(&body, &tokens, &fillets, &curves, origin, pick_scale(normal, -1.0));
        match hit {
            Some(pick) if pick.id == face.id => {}
            Some(pick) if curve_ids.contains(&pick.id) => failures.push(format!(
                "{label}: flat interior of face {} selected curved face {}",
                face.id, pick.id
            )),
            Some(pick) => failures.push(format!(
                "{label}: flat interior of face {} selected face {}",
                face.id, pick.id
            )),
            None => failures.push(format!("{label}: flat interior of face {} selected nothing", face.id)),
        }
    }
    check_fillet_strips(label, &body, &tokens, &fillets, &curves, failures);
    check_fillet_strips(&format!("{label} without curve samples"), &body, &tokens, &fillets, &[], failures);
}

/// Each fillet's mid-strip, on both adjacent faces, names that fillet. A corner id is still a curved face.
fn check_fillet_strips(label: &str, body: &SolidBody, tokens: &[String], fillets: &[Fillet], curves: &[CurveElement], failures: &mut Vec<String>) {
    let planar: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
    for (token, fillet) in tokens.iter().zip(fillets.iter()) {
        let wanted = round_curve_id(&format!("F:fillet({token})"), 0);
        for (index, (origin, direction)) in fillet_strip_rays(fillet).into_iter().enumerate() {
            let hit = visible_authored_face(body, tokens, fillets, curves, origin, direction);
            match hit {
                Some(pick) if pick.id == wanted => {}
                Some(pick) if !planar.contains(&pick.id) => failures.push(format!(
                    "{label}: {token} side {index} selected curved face {} instead of the fillet",
                    pick.id
                )),
                Some(pick) => failures.push(format!(
                    "{label}: {token} side {index} selected planar face {} instead of the fillet",
                    pick.id
                )),
                None => failures.push(format!("{label}: {token} side {index} selected nothing")),
            }
        }
    }
}

#[test]
fn a_curved_face_stays_selectable_after_a_planar_extrude() {
    let record = seed_record();
    let body = candidate_body(&record);
    let bindings = semantic_edge_bindings(&record).expect("bindings");
    let top = [10u32, 12, 19, 20];
    let tokens: Vec<String> = top.iter().map(|edge| persistent_edge_token(&body, &bindings, *edge).unwrap()).collect();
    let rounded = commit_authored_round(&record, round_entry_set(&tokens, 0.25)).expect("top loop");
    assert!(rounded.body.is_none());
    let mut failures = Vec::new();
    check_curved_face_picks("top loop", &rounded, &mut failures);

    let cap = 5u32;
    let normal = body.unit_normal(cap).expect("+Z");
    let extruded = authored_extrude_record(&rounded, &[cap], pick_scale(normal, 2.35)).expect("extrude +Z");
    assert!(extruded.body.is_none());
    assert_eq!(tape_words(&extruded), vec!["seed", "round", "extrude"]);
    check_curved_face_picks("top loop then +Z extrude", &extruded, &mut failures);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_round_then_extrude_keeps_the_round_and_refuses_a_fillet() {
    let record = seed_record();
    let body = candidate_body(&record);
    let bindings = semantic_edge_bindings(&record).expect("bindings");
    let pair = [11u32, 16];
    let pair_tokens: Vec<String> = pair.iter().map(|edge| persistent_edge_token(&body, &bindings, *edge).unwrap()).collect();
    let rounded = commit_authored_round(&record, round_entry_set(&pair_tokens, 0.05)).expect("round");
    assert!(rounded.body.is_none());
    assert_eq!(tape_words(&rounded), vec!["seed", "round"]);

    let clear = body
        .faces
        .iter()
        .map(|face| face.id)
        .find(|face| !face_uses_edge(&body, *face, 11) && !face_uses_edge(&body, *face, 16))
        .expect("unaffected face");
    let clear_name = semantic_face_names(&rounded, clear).into_iter().find(|name| name.starts_with("F:seed/")).expect("seed face");
    let clear_normal = body.unit_normal(clear).expect("normal");
    let clear_delta = [clear_normal[0] * 0.4, clear_normal[1] * 0.4, clear_normal[2] * 0.4];
    let kept = authored_extrude_record(&rounded, &[clear], clear_delta).expect("unaffected extrude");
    assert!(kept.body.is_none());
    assert_eq!(tape_words(&kept), vec!["seed", "round", "extrude"]);
    assert_eq!(kept.intent.last().and_then(|entry| entry.groups.clone()), Some(vec![vec![clear_name]]));
    assert_eq!(binding(&kept, "E:seed-edge/2"), 11);
    assert_eq!(binding(&kept, "E:seed-edge/7"), 16);
    assert_round_observed(&kept);
    let (document, mut world) = fresh();
    let id = place(&mut world, kept);
    let (text, loaded) = reload(&document, &world);
    assert_no_body(&text, &loaded);
    assert_eq!(tape_words(&loaded), vec!["seed", "round", "extrude"]);
    assert!(rounds(&loaded).iter().any(|(tokens, radius)| tokens == &pair_tokens && (*radius - 0.05).abs() < 1.0e-9));
    assert_round_observed(&loaded);

    let cap = body.faces.iter().map(|face| face.id).find(|face| face_uses_edge(&body, *face, 11) && face_uses_edge(&body, *face, 16)).expect("rounded face");
    let cap_normal = body.unit_normal(cap).expect("cap normal");
    let cap_delta = [cap_normal[0] * 0.4, cap_normal[1] * 0.4, cap_normal[2] * 0.4];
    let moved = authored_extrude_record(&rounded, &[cap], cap_delta).expect("extrude of the rounded face");
    assert!(moved.body.is_none());
    assert_eq!(tape_words(&moved), vec!["seed", "round", "extrude"]);
    let moved_body = candidate_body(&moved);
    let followed = binding(&moved, "E:seed-edge/2");
    let followed_pair = binding(&moved, "E:seed-edge/7");
    assert_ne!(followed, 11);
    assert_ne!(followed_pair, 16);
    assert!(moved_body.edges.iter().any(|edge| edge.id == followed));
    assert!(moved_body.edges.iter().any(|edge| edge.id == followed_pair));
    assert_round_observed(&moved);
    assert!(catalog_of(&moved).iter().any(|item| item.token == "F:fillet(E:seed-edge/2)"));
    world.replace_block_record(id, moved.clone()).expect("install moved");
    let (moved_text, moved_loaded) = reload(&document, &world);
    assert_no_body(&moved_text, &moved_loaded);
    assert_eq!(binding(&moved_loaded, "E:seed-edge/2"), followed);
    assert_round_observed(&moved_loaded);

    let fillet = catalog_of(&rounded).into_iter().find(|item| item.token.starts_with("F:fillet(")).expect("fillet face");
    let before = rounded.intent.clone();
    let refused = authored_extrude_record(&rounded, &[fillet.id], [0.0, 0.4, 0.0]).expect_err("curved extrude");
    assert_eq!(refused, "UnsupportedOperation: Extrude does not edit a curved face.");
    world.replace_block_record(id, rounded.clone()).expect("install rounded");
    assert!(world.extrude_authored_faces(id, &[fillet.id], [0.0, 0.4, 0.0]).is_err());
    assert_eq!(world.authored_block(id).unwrap().intent, before);
    assert!(world.authored_block(id).unwrap().body.is_none());

    let mut stored = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
    stored.body = Some(SolidBody::from_box([2.0, 2.0, 2.0]).unwrap());
    let stored_err = authored_extrude_record(&stored, &[1], [0.4, 0.0, 0.0]).expect_err("stored body");
    assert_eq!(stored_err, "Extrude edits an authored block. This object stores its own shape.");
}

fn tape_words(record: &BlockRecord) -> Vec<&'static str> {
    record
        .intent
        .iter()
        .map(|entry| match entry.payload {
            IntentPayload::Seed => "seed",
            IntentPayload::Round { .. } => "round",
            IntentPayload::Extrude { .. } => "extrude",
            _ => "other",
        })
        .collect()
}

fn face_uses_edge(body: &SolidBody, face: u32, edge: u32) -> bool {
    let Some(edge) = body.edges.iter().find(|item| item.id == edge) else { return false };
    let Some(loop_) = body.face_loop(face) else { return false };
    let count = loop_.len();
    (0..count).any(|index| {
        let a = loop_[index];
        let b = loop_[(index + 1) % count];
        (a == edge.a && b == edge.b) || (a == edge.b && b == edge.a)
    })
}

fn assert_round_observed(record: &BlockRecord) {
    let replayed = replay_round(record).expect("replay");
    let camera = observation_camera(&replayed.fillet, 2.0, 1280.0, 720.0, CURVE_FOV, CURVE_ERROR_PX).expect("camera");
    let product = crate::realize_round_solid(record, &camera, "chain", 1).expect("round stays");
    let planar = mesh_from_body(&candidate_body(record)).index_count() / 3;
    assert!(product.triangles_constructed > planar, "round {} planar {planar}", product.triangles_constructed);
    assert!(record.body.is_none());
}

fn catalog_of(record: &BlockRecord) -> Vec<CurveElement> {
    let rounds = replay_rounds(record).expect("rounds");
    let body = candidate_body(record);
    let pairs: Vec<_> = rounds.into_iter().map(|round| (round.token, round.fillet)).collect();
    curve_catalog(&body, &pairs).expect("catalog")
}

fn curve_slot(token: &str) -> u8 {
    if token.starts_with("E:fillet-a(") {
        1
    } else if token.starts_with("E:fillet-b(") {
        2
    } else if token.starts_with("V:fillet-start(") {
        3
    } else if token.starts_with("V:fillet-end(") {
        4
    } else {
        0
    }
}

fn point_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let delta = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

#[test]
fn the_face_click_project_starts_blank() {
    let directory = face_dir();
    let project_file = directory.join("AuthoredFace.jarvigproject");
    if project_file.exists() {
        return;
    }
    create_project_at(&project_file, "Authored Face", ProjectTemplate::Blank).expect("blank project");
    let level = std::fs::read_to_string(directory.join("Content").join("Levels").join("Main.jarviglevel")).expect("level");
    assert!(!level.contains("\"body\""));
    assert!(!level.contains("\"op\": \"seed\""));
}

#[test]
fn the_visible_round_builds_a_shaded_solid() {
    let record = commit_authored_round(&seed_record(), round_entry(VISUAL_EDGE, VISUAL_SHOW_M)).expect("round");
    assert_eq!(intent_authority_diagnostic(&record), "Intent Authority: ELIGIBLE");
    assert!(record.body.is_none());
    let replayed = replay_round(&record).expect("replay");
    assert_eq!(replayed.edge_id, 16);
    let camera = observation_camera(&replayed.fillet, 3.0, 1280.0, 720.0, CURVE_FOV, CURVE_ERROR_PX).expect("camera");
    let solid = crate::realize_round_solid(&record, &camera, "viewport", 1).expect("shaded round");
    assert!(solid.triangles_constructed > 12, "shaded triangles {}", solid.triangles_constructed);
    println!(
        "SHADED_ROUND tris={} div={} err={} edge={}",
        solid.triangles_constructed, solid.chosen_arc_divisions, solid.measured_error_px, solid.semantic_edge_id
    );
}

#[test]
fn the_visual_project_saves_one_visible_round() {
    let (document, mut world) = fresh();
    let id = place(&mut world, seed_record());
    let rounded = commit_authored_round(&world.authored_block(id).unwrap(), round_entry(VISUAL_EDGE, VISUAL_SHOW_M)).expect("visible round");
    world.replace_block_record(id, rounded).expect("install");
    let saved = LevelDocument::capture(&world, document.level_uuid, "Authored Block").expect("capture");
    let text = saved.to_json();
    let loaded = parse_level(&text).unwrap().instantiate().unwrap();
    let record = loaded.entity_outline().into_iter().find_map(|row| loaded.authored_block(row.uuid)).unwrap();
    assert_no_body(&text, &record);
    assert_eq!(rounds(&record), vec![(vec![VISUAL_EDGE.to_string()], VISUAL_SHOW_M)]);
    assert_eq!(binding(&record, VISUAL_EDGE), 16);
    let directory = visual_dir();
    let _ = std::fs::remove_dir_all(&directory);
    let project_file = write_visual_project(&directory, &saved).expect("visual project");
    let on_disk = std::fs::read_to_string(project_file.parent().unwrap().join("Content").join("Levels").join("Main.jarviglevel")).unwrap();
    assert_eq!(on_disk, text);
    println!("AUTHORED_BLOCK_PROJECT {}", project_file.display());
    report("visual-0.35", &text, &record);
}

fn face_dir() -> PathBuf {
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Temp")
        .join("jarvig-authored-face");
    let text = directory.to_string_lossy();
    for forbidden in ["jarvig-authored-curve", "jarvig-intent-proof", "jarvig-authored-intent", "jarvig-round-tool", "IntentProof", "jarvig-authored-block"] {
        assert!(!text.contains(forbidden), "{text}");
    }
    assert!(text.contains("jarvig-authored-face"), "{text}");
    directory
}

fn visual_dir() -> PathBuf {
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Temp")
        .join("jarvig-authored-block");
    let text = directory.to_string_lossy();
    for forbidden in ["jarvig-authored-curve", "jarvig-intent-proof", "jarvig-authored-intent", "jarvig-round-tool", "IntentProof"] {
        assert!(!text.contains(forbidden), "{text}");
    }
    assert!(text.contains("jarvig-authored-block"), "{text}");
    directory
}

fn write_visual_project(directory: &Path, level: &LevelDocument) -> Result<PathBuf, String> {
    let project_file = directory.join("AuthoredBlock.jarvigproject");
    let project = ProjectDocument {
        format_version: PROJECT_FORMAT_VERSION,
        project_uuid: EntityId::parse("22222222-2222-4222-8222-222222222221").ok_or("project uuid")?,
        display_name: "Authored Block".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        startup_level: "Content/Levels/Main.jarviglevel".into(),
        content_directory: "Content".into(),
        saved_directory: "Saved".into(),
        config_directory: "Config".into(),
        intermediate_directory: "Intermediate".into(),
        settings: "Config/Project.jarvigsettings".into(),
    };
    create_project_directories(&project_file, &project).map_err(|error| error.to_string())?;
    let backup = directory.join("Saved").join("Backup");
    save_project_atomic(&project_file, &backup, &project).map_err(|error| error.to_string())?;
    let level_path = project.startup_level_path(&project_file).map_err(|error| error.to_string())?;
    save_level_atomic(&level_path, &backup, level).map_err(|error| error.to_string())?;
    let settings = directory.join(&project.settings);
    if let Some(parent) = settings.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n").map_err(|error| error.to_string())?;
    Ok(project_file)
}
