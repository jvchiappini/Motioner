//! `use()` imports, SVG symbols and rig components (prompt 16).

use std::collections::HashMap;
use std::sync::Arc;

use motioner_core::Axis;
use motioner_dsl::{run_with_assets, AssetResolver, LoadedAsset, NoAssets};
use motioner_eval::sample_at;

const MASCOT: &str = include_str!("../../../examples/rig/assets/mascot.svg");

/// An in-memory resolver: key -> (source, directory).
#[derive(Default)]
struct Files {
    entries: HashMap<String, (String, String)>,
}

impl Files {
    fn with(mut self, key: &str, source: &str, dir: &str) -> Self {
        self.entries
            .insert(key.to_owned(), (source.to_owned(), dir.to_owned()));
        self
    }
}

impl AssetResolver for Files {
    fn load(&self, path: &str, base: &str) -> Result<LoadedAsset, String> {
        let joined = if base.is_empty() {
            path.to_owned()
        } else {
            format!("{base}/{path}")
        };
        // Normalise `..` for the few test paths that use it.
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        let key = parts.join("/");
        let Some((source, dir)) = self.entries.get(&key) else {
            return Err(format!("missing asset {key}"));
        };
        if key.to_ascii_lowercase().ends_with(".svg") {
            let symbol = motioner_io_stub::parse(source, &key)?;
            return Ok(LoadedAsset::Symbol {
                key,
                symbol: Arc::new(symbol),
            });
        }
        Ok(LoadedAsset::Module {
            key,
            source: source.clone(),
            dir: dir.clone(),
        })
    }
}

/// Parses with `motioner_core`'s symbol shape, mirroring `motioner_io` just
/// enough for the tests (the real parser is tested in `motioner_io`).
mod motioner_io_stub {
    use motioner_core::{
        Color, PaintSpec, PathGeometry, PathPoint, Symbol, SymbolPart, SymbolPath, SymbolShape,
        Vec2,
    };

    pub fn parse(source: &str, key: &str) -> Result<Symbol, String> {
        if !source.contains("id=\"body\"") {
            return Err(format!("{key}: no body"));
        }
        let geometry = PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(10.0, 0.0)),
                PathPoint::corner(Vec2::new(10.0, 10.0)),
            ],
            true,
            motioner_core::LineCap::Butt,
            motioner_core::LineJoin::Miter,
        )
        .map_err(|error| error.to_string())?;
        let part = SymbolPart {
            id: Some("body".to_owned()),
            anchor: Vec2::new(5.0, 5.0),
            opacity: 1.0,
            shape: SymbolShape::Path(SymbolPath {
                geometry,
                paint: PaintSpec::new(Some(Color::WHITE), None, 0.0),
                dash: None,
            }),
            clip: Vec::new(),
        };
        Ok(Symbol {
            name: "mascot".to_owned(),
            width: 200.0,
            height: 200.0,
            parts: vec![part],
            warnings: Vec::new(),
        })
    }
}

fn assets() -> Files {
    Files::default().with("mascot.svg", MASCOT, "")
}

#[test]
fn a_symbol_is_a_definition_with_metadata_and_parts() {
    let source = "\
comp(name=\"Rig\", width=200, height=200, fps=60, duration=2.0)
art = use(\"mascot.svg\")
w = art.width
h = art.height
ids = art.parts
s = art(pos=(20, 30), scale=2)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let group = document.node(document.roots()[0]).expect("the symbol root");
    assert!((group.transform().position.value().x - 25.0).abs() < 1e-9);
    assert!((group.transform().scale.value().x - 2.0).abs() < 1e-9);
}

#[test]
fn a_geometry_driver_bakes_path_keys() {
    let source = "\
comp(name=\"Geometry\", width=200, height=200, fps=60, duration=2.0)
art = use(\"mascot.svg\")
m = rig(art)
m.prop(\"lift\", 0.0, range=(0.0, 10.0))
m.lift.key(0.0, 0.0)
m.lift.key(1.0, 10.0)
def geometry(lift):
    return [[\"body\", \"geometry\", path_builder(move_to(0, 0), line_to(10, lift), line_to(10, 10), close())]]
m.driver(geometry)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("body");
    let motioner_core::NodeContent::Path { geometry, .. } = body.content() else {
        panic!("body is a path");
    };
    assert!(!geometry.keys().is_empty());
    let middle = geometry
        .sample(30.0, motioner_eval::ease)
        .expect("sampled geometry");
    assert!((middle.points()[1].position.y - 5.0).abs() < 1e-9);
    let end = geometry
        .sample(60.0, motioner_eval::ease)
        .expect("sampled geometry");
    assert!((end.points()[1].position.y - 10.0).abs() < 1e-9);
}

#[test]
fn a_geometry_driver_accepts_arc_to() {
    let source = "\
comp(name=\"Arc\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
m = rig(art)
def geometry():
    return [[\"body\", \"geometry\", path_builder(move_to(0, 0), arc_to(5, 5, 0, 0, 1, 10, 0, segments=2), close())]]
m.driver(geometry)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("body");
    let motioner_core::NodeContent::Path { geometry, .. } = body.content() else {
        panic!("body is a path");
    };
    assert!(!geometry.keys().is_empty());
}

#[test]
fn a_path_geometry_key_uses_the_fixed_topology() {
    let source = "\
comp(name=\"Geometry\", width=200, height=200, fps=60, duration=2.0)
p = path([[0, 0], [10, 0], [10, 10]])
p.key(\"geometry\", 1.0, [[0, 0], [10, 5], [10, 10]])
";
    let document = motioner_dsl::run(source, &motioner_dsl::Limits::default()).unwrap();
    let node = document
        .nodes()
        .find(|node| matches!(node.content(), motioner_core::NodeContent::Path { .. }))
        .unwrap();
    let motioner_core::NodeContent::Path { geometry, .. } = node.content() else {
        panic!("path");
    };
    let key = geometry.keys().first().expect("geometry key");
    assert_eq!(key.frame, 60);
    assert!((key.points[1].position.y - 5.0).abs() < 1e-9);
}

#[test]
fn rig_deformers_and_constraints_author_and_bake() {
    let source = "\
comp(name=\"Deform\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
m = rig(art)
m.prop(\"tl\", (0.0, 0.0))
m.prop(\"tr\", (0.0, 0.0))
m.prop(\"br\", (0.0, 0.0))
m.prop(\"bl\", (0.0, 0.0))
m.prop(\"weight\", 0.0)
m.prop(\"progress\", 0.0)
m.prop(\"target\", (10.0, 10.0))
m.lattice(\"body\", corners=(\"tl\", \"tr\", \"br\", \"bl\"))
m.bone(\"root\", at=(0, 0), length=10.0)
m.bone(\"tip\", parent=\"root\", at=(10, 0), length=10.0)
m.skin(\"body\")
m.blend(\"body\", points=[[0, 0], [10, 5], [10, 10]], weight=\"weight\", fit=True)
m.ik(\"bone:root\", \"bone:tip\", target=\"target\", tip=10.0)
m.copy(\"bone:tip\", \"bone:root\", position=False)
m.limit(\"bone:tip\", rotation=(-30, 30))
m.aim(\"bone:root\", \"target\", radius=2.0)
m.follow(\"body\", \"body\", progress=\"progress\")
m.copy(\"body\", \"body\")
m.limit(\"body\", min=(-5, -5), max=(5, 5), rotation=(-30, 30))
m.distance(\"body\", \"body\", distance=10.0)
m.spring(\"body\", target=\"target\")
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("body");
    let motioner_core::NodeContent::Path { geometry, .. } = body.content() else {
        panic!("body is a path");
    };
    assert_eq!(geometry.points().len(), 3);
}

#[test]
fn skin_declared_before_a_late_bone_still_bakes() {
    let source = "\
comp(name=\"Late\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
m = rig(art)
m.bone(\"root\", at=(0, 0), length=10.0)
m.skin(\"body\")
m.bone(\"late\", parent=\"root\", at=(10, 0), length=10.0)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("a bone declared after skin still bakes");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("body");
    let motioner_core::NodeContent::Path { geometry, .. } = body.content() else {
        panic!("body is a path");
    };
    assert_eq!(geometry.points().len(), 3);
}

#[test]
fn a_rig_property_and_a_driver_bake_into_keys() {
    let source = "\
comp(name=\"Rig\", width=200, height=200, fps=60, duration=2.0)
art = use(\"mascot.svg\")
m = rig(art)
m.prop(\"sy\", 1.0, range=(0.5, 2.0), step=0.01)
m.pose(\"tall\", sy=2.0)
def squash(sy):
    return [[\"body\", \"scale.y\", sy]]
m.driver(squash)
m.pose(\"tall\", at=1.0, dur=0.5, ease=\"linear\")
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("the body part");
    let scale = body.transform().scale.axis(Axis::Y);
    assert!(
        (sample_at(scale, 59.0) - 1.0).abs() < 1e-9,
        "still at rest: {:?} keys {:?}",
        sample_at(scale, 59.0),
        scale.keys()
    );
    assert!(
        (sample_at(scale, 60.0) - 1.0).abs() < 1e-9,
        "transition starts"
    );
    assert!(
        (sample_at(scale, 75.0) - 1.5).abs() < 1e-6,
        "halfway through the pose"
    );
    assert!((sample_at(scale, 90.0) - 2.0).abs() < 1e-9, "pose reached");
}

#[test]
fn a_module_exports_definitions_that_build_into_the_caller() {
    let files = assets().with(
        "lib/helpers.motion",
        "\
def make(art):
    m = rig(art)
    m.prop(\"twist\", 0.0, range=(-45, 45))
    return m
",
        "lib",
    );
    let source = "\
comp(name=\"Rig\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
helpers = use(\"lib/helpers.motion\")
m = helpers.make(art)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &files,
    )
    .expect("the module builds a rig");
    assert!(document.nodes().any(|node| node.name() == Some("body")));
}

#[test]
fn module_cycles_and_sandbox_errors_are_typed_diagnostics() {
    let files = Files::default().with("a.motion", "b = use(\"a.motion\")\n", "");
    let source = "\
comp(name=\"Cycle\", duration=1.0)
x = use(\"a.motion\")
";
    let error = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &files,
    )
    .unwrap_err();
    assert!(error.message.contains("cycle"), "{}", error.message);
    assert_eq!(error.span.line, 2);
}

#[test]
fn missing_assets_need_a_resolver_and_fail_clearly() {
    let source = "comp(name=\"X\", duration=1.0)\nx = use(\"mascot.svg\")\n";
    let error = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &NoAssets,
    )
    .unwrap_err();
    assert!(error.message.contains("resolver"), "{}", error.message);
}

#[test]
fn media_definitions_work_without_a_resolver() {
    let source = "\
comp(name=\"Media\", width=100, height=100, duration=1.0)
logo = use(\"logo.png\")
n = logo(width=50, height=50)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &NoAssets,
    )
    .expect("the image definition instantiates");
    assert_eq!(document.len(), 1);
}

#[test]
fn a_driver_outside_the_expression_subset_is_rejected_with_its_name() {
    let source = "\
comp(name=\"Rig\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
m = rig(art)
m.prop(\"sy\", 1.0, range=(0.5, 2.0))
def squash(sy):
    x = sy * 2.0
    return [[\"body\", \"scale.y\", x]]
m.driver(squash)
";
    let error = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .unwrap_err();
    assert!(
        error.message.contains("driver 'squash'"),
        "{}",
        error.message
    );
}

#[test]
fn keys_on_a_rig_property_are_part_of_the_bake() {
    let source = "\
comp(name=\"Rig\", width=200, height=200, fps=60, duration=1.0)
art = use(\"mascot.svg\")
m = rig(art)
m.prop(\"sy\", 1.0, range=(0.5, 2.0))
def squash(sy):
    return [[\"body\", \"scale.y\", sy]]
m.driver(squash)
m.sy.key(0.0, 1.0)
m.sy.key(1.0, 2.0)
";
    let document = run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets(),
    )
    .expect("the program runs");
    let body = document
        .nodes()
        .find(|node| node.name() == Some("body"))
        .expect("body");
    let scale = body.transform().scale.axis(Axis::Y);
    let at = |frame: f64| sample_at(scale, frame);
    assert!((at(0.0) - 1.0).abs() < 1e-9);
    assert!((at(30.0) - 1.5).abs() < 1e-6);
    assert!((at(60.0) - 2.0).abs() < 1e-9);
}
