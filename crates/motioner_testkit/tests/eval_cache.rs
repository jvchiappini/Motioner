//! Proves `Evaluator::evaluate_cached` against every available fixture (prompt
//! 18 P7).
//!
//! Each document is compiled, its images and videos are resolved, and then the
//! cached evaluator is driven over a frame set that covers the duration plus a
//! backwards jump. Every cached plan must be item-for-item equal to a fresh
//! uncached evaluation at the same frame. A revision change and an override
//! compile are covered separately. The generated perf fixtures under `out/`
//! are optional (that folder is gitignored): the small ones join the main test
//! and the large ones stay behind `#[ignore]` because compiling them in debug
//! is slow.
//!
//! The test is deterministic: no wall clock, no randomness, and factories live
//! in fixed order.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use motioner_core::{
    Document, Frame, OverrideAction, OverrideEntry, OverrideSet, OverrideValue, Property,
    PropertyValue, Vec2,
};
use motioner_dsl::Limits;
use motioner_eval::{Evaluator, RenderPlan};
use motioner_io::{Project, ProjectAssets, ProjectImages};
use motioner_render::{CpuTextMetrics, FontSet};

/// Frames evaluated per document in the exhaustive comparison.
const MAX_FRAMES: usize = 200;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("motioner_testkit lives under crates/")
        .to_path_buf()
}

fn metrics() -> CpuTextMetrics {
    CpuTextMetrics::new(FontSet::bundled())
}

/// Recursively collects `.motion` files under `dir`, sorted for determinism.
fn motion_files(dir: &Path) -> Vec<PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("motion") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out
}

/// A loose `.motion` file: compile it with `use()` assets and resolve media.
fn compile_loose(root: &Path, path: &Path, metrics: &CpuTextMetrics) -> Document {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let name = path.to_string_lossy();
    let mut document = motioner_dsl::run_with_assets(
        &source,
        &Limits::default(),
        metrics,
        &ProjectAssets::new(root),
    )
    .unwrap_or_else(|error| panic!("{}", error.render(&source, &name)));
    let resolver = ProjectImages::loose(root);
    let _ = document.resolve_images(&resolver);
    let _ = document.resolve_videos(&resolver);
    document
}

/// A `.motioner` project: compile its main composition and resolve its media.
fn compile_project(dir: &Path, metrics: &CpuTextMetrics) -> Document {
    let project =
        Project::open(dir).unwrap_or_else(|error| panic!("cannot open {}: {error}", dir.display()));
    let path = project.composition_path("main");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let name = path.to_string_lossy();
    let mut document = motioner_dsl::run_with_assets(
        &source,
        &Limits::default(),
        metrics,
        &ProjectAssets::new(project.root()),
    )
    .unwrap_or_else(|error| panic!("{}", error.render(&source, &name)));
    let resolver = ProjectImages::new(&project);
    let _ = document.resolve_images(&resolver);
    let _ = document.resolve_videos(&resolver);
    document
}

/// The frames to evaluate: the whole duration when short, a stride when long,
/// the last frame, and a final jump back to `0` to exercise the reverse window.
fn frames_for(document: &Document) -> Vec<Frame> {
    let duration = document.composition().duration().max(1);
    let total = usize::try_from(duration).unwrap_or(MAX_FRAMES);
    let stride = total.div_ceil(MAX_FRAMES).max(1);
    let step = i64::try_from(stride).unwrap_or(1);
    let mut frames = Vec::new();
    let mut seen = BTreeSet::new();
    let mut frame: Frame = 0;
    while frame < duration {
        if seen.insert(frame) {
            frames.push(frame);
        }
        frame = frame.saturating_add(step);
    }
    let last = duration - 1;
    if seen.insert(last) {
        frames.push(last);
    }
    if frames.last().copied() != Some(0) {
        frames.push(0);
    }
    frames
}

fn evaluate_plain(document: &Document, frame: Frame) -> RenderPlan {
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(document, frame, &mut plan);
    plan
}

/// Compares cached and uncached evaluation over `frames`, returning their
/// count. Both evaluators keep their plan (as the session does), so the
/// comparison also covers arena reuse.
fn assert_cache_matches(document: &Document, label: &str, revision: u64) -> usize {
    let frames = frames_for(document);
    let mut plain = Evaluator::new();
    let mut cached = Evaluator::new();
    let mut expected = RenderPlan::new();
    let mut actual = RenderPlan::new();
    for &frame in &frames {
        plain.evaluate(document, frame, &mut expected);
        cached.evaluate_cached(document, revision, frame, &mut actual);
        assert!(
            expected == actual,
            "{label}: cached plan differs from uncached at frame {frame}"
        );
    }
    frames.len()
}

fn small_loose_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join("examples")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("motion")
            {
                out.push(path);
            }
        }
    }
    out.extend(motion_files(&root.join("examples/design_defects")));
    let demo = root.join("examples/rig/mascot_demo.motion");
    if demo.is_file() {
        out.push(demo);
    }
    out.sort();
    out
}

fn small_projects(root: &Path) -> Vec<PathBuf> {
    vec![root.join("examples/demo.motioner")]
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

fn large_loose_fixtures(root: &Path) -> Vec<PathBuf> {
    let fixtures = root.join("out/reports/phase18/fixtures");
    [
        fixtures.join("heavy_ui.motion"),
        fixtures.join("rows_1000.motion"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect()
}

fn large_projects(root: &Path) -> Vec<PathBuf> {
    let fixtures = root.join("out/reports/phase18/fixtures");
    [
        fixtures.join("heavy_ui_list.motioner"),
        fixtures.join("heavy_ui_list_1000.motioner"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn small_perf_fixtures(root: &Path) -> Vec<PathBuf> {
    let fixtures = root.join("out/reports/phase18/fixtures");
    [
        fixtures.join("rows_10.motion"),
        fixtures.join("rows_100.motion"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect()
}

#[test]
fn every_small_fixture_matches_uncached_frame_by_frame() {
    let root = repo_root();
    let metrics = metrics();
    let mut documents = 0_usize;
    let mut frames = 0_usize;

    for path in small_loose_fixtures(&root)
        .into_iter()
        .chain(small_perf_fixtures(&root))
    {
        let label = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let document = compile_loose(&root, &path, &metrics);
        frames += assert_cache_matches(&document, &label, 7);
        documents += 1;
    }

    for dir in small_projects(&root) {
        let label = dir
            .strip_prefix(&root)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let document = compile_project(&dir, &metrics);
        frames += assert_cache_matches(&document, &label, 7);
        documents += 1;
    }

    eprintln!("eval cache: {documents} documents, {frames} frames per evaluator");
    assert!(
        documents >= 44,
        "expected every shipped example, got {documents}"
    );
    assert!(frames > 0, "no frames were evaluated");
}

#[test]
#[ignore = "3000/1000-node perf fixtures are slow to compile in debug (Lane C)"]
fn large_perf_fixtures_match_uncached_frame_by_frame() {
    let root = repo_root();
    let metrics = metrics();
    let mut documents = 0_usize;
    let mut frames = 0_usize;

    for path in large_loose_fixtures(&root) {
        let label = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let document = compile_loose(&root, &path, &metrics);
        frames += assert_cache_matches(&document, &label, 7);
        documents += 1;
    }

    for dir in large_projects(&root) {
        let label = dir
            .strip_prefix(&root)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let document = compile_project(&dir, &metrics);
        frames += assert_cache_matches(&document, &label, 7);
        documents += 1;
    }

    if documents == 0 {
        eprintln!("skipping: the generated fixtures under out/ are not present");
        return;
    }
    eprintln!("eval cache (large): {documents} documents, {frames} frames per evaluator");
}

#[test]
fn a_revision_change_matches_uncached() {
    let root = repo_root();
    let metrics = metrics();
    let path = root.join("examples/animatable.motion");
    let document = compile_loose(&root, &path, &metrics);

    let frames = [0, 30, 60, 90, 119, 0];
    let mut cached = Evaluator::new();
    let mut actual = RenderPlan::new();
    cached.evaluate_cached(&document, 11, frames[0], &mut actual);
    let before = actual.clone();

    let mut mutated = document.clone();
    let target = mutated.roots()[0];
    mutated
        .set_property(
            target,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(42.0, 24.0)),
        )
        .expect("the first root accepts a static position");

    let mut plain = Evaluator::new();
    let mut expected = RenderPlan::new();
    for &frame in &frames {
        plain.evaluate(&mutated, frame, &mut expected);
        cached.evaluate_cached(&mutated, 12, frame, &mut actual);
        assert!(
            expected == actual,
            "revision change: cached plan differs from uncached at frame {frame}"
        );
    }
    assert_ne!(before, actual, "the mutation must change the plan");
}

#[test]
fn an_overridden_document_matches_uncached_after_a_revision_bump() {
    let root = repo_root();
    let metrics = metrics();
    let path = root.join("examples/hello.motion");
    let source = fs::read_to_string(&path).expect("examples/hello.motion is readable");

    let mut base = motioner_dsl::run_with_assets(
        &source,
        &Limits::default(),
        &metrics,
        &ProjectAssets::new(&root),
    )
    .expect("examples/hello.motion compiles");
    let resolver = ProjectImages::loose(&root);
    let _ = base.resolve_images(&resolver);
    let _ = base.resolve_videos(&resolver);

    // The first root is the title, whose text opacity is 0 at frame 0 and
    // whose rotation never changes a plan; use the first animated dot, whose
    // opacity (1.0) is what a plan item actually carries, so the override is
    // visible in the evaluated plan.
    let target = base.roots()[1];
    let identity = base
        .node(target)
        .expect("the root exists")
        .identity()
        .as_str()
        .to_owned();
    let mut set = OverrideSet::new();
    set.insert(OverrideEntry::new(
        identity,
        "opacity",
        OverrideAction::Set(OverrideValue::Number(0.5)),
    ));
    let overrides = set.to_mdf(base.composition().frame_rate());
    let compiled = motioner_dsl::run_with_overrides_and_metrics(
        &source,
        Some(&overrides),
        &Limits::default(),
        &metrics,
    )
    .expect("the overridden program compiles");
    let mut overridden = compiled.document;
    let _ = overridden.resolve_images(&resolver);
    let _ = overridden.resolve_videos(&resolver);

    let base_plan = evaluate_plain(&base, 0);

    let frames = [0, 30, 60, 120, 299, 0];
    let mut cached = Evaluator::new();
    let mut actual = RenderPlan::new();
    cached.evaluate_cached(&base, 3, frames[0], &mut actual);

    let mut plain = Evaluator::new();
    let mut expected = RenderPlan::new();
    for &frame in &frames {
        plain.evaluate(&overridden, frame, &mut expected);
        cached.evaluate_cached(&overridden, 4, frame, &mut actual);
        assert!(
            expected == actual,
            "override: cached plan differs from uncached at frame {frame}"
        );
    }
    assert_ne!(
        base_plan,
        evaluate_plain(&overridden, 0),
        "the override must change the plan"
    );
}
