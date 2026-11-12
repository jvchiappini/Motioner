//! The headless editor session compiles `use()` through the same resolver as
//! a direct run, and exposes the compiler's rig manifest (D-0056).

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use motioner_core::NoTextMetrics;
use motioner_io::ProjectAssets;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_editor_session_matches_a_direct_run_and_its_manifest() {
    let root = root();
    let path = root.join("examples/rig/mascot_demo.motion");
    let source = std::fs::read_to_string(&path).expect("mascot_demo.motion is readable");
    let assets = ProjectAssets::new(&root);

    let direct = motioner_dsl::run_with_assets(
        &source,
        &motioner_dsl::Limits::default(),
        &NoTextMetrics,
        &assets,
    )
    .expect("the demo runs directly");

    let editor = motioner_ui::Editor::open_with_assets(
        &path,
        Arc::new(NoTextMetrics),
        Rc::new(ProjectAssets::new(&root)),
    )
    .expect("the editor session opens the demo");
    let session = editor
        .session()
        .document()
        .expect("the editor session has a document");

    assert_eq!(session, &direct, "the editor and the direct run disagree");
    assert_eq!(
        session.rig_manifest(),
        direct.rig_manifest(),
        "the manifest must come from the same compile"
    );

    let manifest = session.rig_manifest();
    assert_eq!(manifest.version, motioner_core::RIG_MANIFEST_VERSION);
    assert_eq!(manifest.rigs.len(), 1);
    let rig = &manifest.rigs[0];
    assert_eq!(rig.name, "mascot");
    assert_eq!(rig.parts.len(), direct_manifest_part_count(&direct));
    assert!(rig.properties.len() >= 19);
    assert_eq!(rig.poses.len(), 17);
    assert_eq!(rig.clips.len(), 2);
    let machine = rig.machine.as_ref().expect("the demo has a machine");
    assert_eq!(machine.states.len(), 3);
    assert_eq!(machine.transitions.len(), 6);
    assert_eq!(machine.inputs.len(), 1);
    assert_eq!(machine.inputs[0].name, "mood");
    assert_eq!(machine.events.len(), machine.transitions.len());
    assert!(rig.layers.iter().any(|layer| layer.kind == "blink"));
    assert!(rig.aims.iter().any(|aim| aim.part == "pupil-left"));
    let lid = rig
        .properties
        .iter()
        .find(|property| property.name == "lt_L")
        .expect("lt_L");
    assert_eq!(lid.group.as_deref(), Some("lid"));
    assert_eq!(
        lid.mirror.as_ref().map(|mirror| mirror.side),
        Some(motioner_core::RigMirrorSide::Left)
    );
}

/// The direct document's manifest is the authority for the part count.
fn direct_manifest_part_count(document: &motioner_core::Document) -> usize {
    document.rig_manifest().rigs[0].parts.len()
}

#[test]
fn query_prints_the_manifest_of_one_instance() {
    let root = root();
    let path = root.join("examples/rig/mascot_demo.motion");
    let editor = motioner_ui::Editor::open_with_assets(
        &path,
        Arc::new(NoTextMetrics),
        Rc::new(ProjectAssets::new(&root)),
    )
    .expect("the editor session opens the demo");

    let json = editor
        .rig_manifest_json(Some("mascot"))
        .expect("the rig is in the manifest");
    assert!(json.starts_with("{\"version\":1,\"rigs\":["));
    assert!(json.contains("\"name\":\"mascot\""));
    assert!(json.contains("\"kind\":\"blink\""));

    let all = editor
        .rig_manifest_json(None)
        .expect("the whole manifest prints");
    assert!(all.contains("\"properties\""));

    let error = editor
        .rig_manifest_json(Some("missing"))
        .expect_err("an unknown rig is a typed error");
    assert_eq!(error.kind, motioner_ui::ErrorKind::UnknownNode);
}
