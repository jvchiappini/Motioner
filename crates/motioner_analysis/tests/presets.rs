//! Every preset, reveal, emphasis, count-up and component passes
//! `check_design` with no errors on its own, on three representative themes
//! (prompt 13 phase 5 + phase 7 deliverable; the full ten-theme matrix is
//! rendered by the release matrix example).

mod common;

use common::{build, Metrics, NoSampler};
use motioner_analysis::{check_design, CheckOptions, Severity};

const THEMES: &[&str] = &[
    "dark",
    "light",
    "editorial",
    "playful",
    "corporate",
    "neon",
    "pastel",
    "mono",
    "warm",
    "tech",
];

fn check(source: &str) -> motioner_analysis::CheckReport {
    let document = build(source);
    check_design(
        &document,
        &CheckOptions::default(),
        &Metrics,
        Some(&NoSampler),
    )
}

fn assert_clean(label: &str, source: &str) {
    let report = check(source);
    assert!(
        report
            .issues
            .iter()
            .all(|issue| issue.severity != Severity::Error),
        "{label} has design errors: {:?}",
        report
            .issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .collect::<Vec<_>>()
    );
}

#[test]
fn every_enter_preset_is_design_clean() {
    for theme in THEMES {
        for kind in [
            "fade", "rise", "drop", "slide", "pop", "zoom", "blur", "wipe", "spin",
        ] {
            let source = format!(
                r#"
comp(name="P", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
r = rect(220, 120, fill="accent")
r.id("r")
r.pos = (500, 300)
r.enter("{kind}", at=0.0, duration=2.0)
"#
            );
            assert_clean(&format!("enter {kind} on {theme}"), &source);
        }
    }
}

#[test]
fn the_draw_preset_is_design_clean_on_a_path() {
    for theme in THEMES {
        let source = format!(
            r#"
comp(name="D", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
p = polygon(5, 90, fill=None, stroke="accent", stroke_width=8, join="round")
p.id("p")
p.pos = (500, 300)
p.enter("draw", at=0.0, duration=2.0)
"#
        );
        assert_clean(&format!("draw on {theme}"), &source);
    }
}

#[test]
fn every_exit_preset_is_design_clean() {
    for theme in THEMES {
        for kind in [
            "fade", "rise", "drop", "slide", "pop", "zoom", "blur", "spin",
        ] {
            let source = format!(
                r#"
comp(name="E", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
r = rect(220, 120, fill="accent")
r.id("r")
r.pos = (500, 300)
r.exit("{kind}", at=1.0, duration=2.0)
"#
            );
            assert_clean(&format!("exit {kind} on {theme}"), &source);
        }
    }
}

#[test]
fn every_emphasis_is_design_clean() {
    for theme in THEMES {
        for kind in ["pulse", "bounce", "shake", "wiggle", "glow"] {
            let source = format!(
                r#"
comp(name="M", width=1000, height=600, fps=60, duration=3.0, theme="{theme}")
use_theme(theme("{theme}"))
r = rect(220, 120, fill="accent")
r.id("r")
r.pos = (500, 300)
r.emphasis("{kind}", at=0.5, duration=2.0)
"#
            );
            assert_clean(&format!("emphasis {kind} on {theme}"), &source);
        }
    }
}

#[test]
fn every_text_reveal_is_design_clean() {
    for theme in THEMES {
        for kind in ["fade", "rise", "mask_up"] {
            let source = format!(
                r#"
comp(name="R", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
t = text("Kinetic title", token("font:body"), token("h1"), token("text"), "center", 700)
t.id("t")
t.pos = (500, 300)
t.reveal("{kind}", at=0.0, duration=3.0)
"#
            );
            assert_clean(&format!("reveal {kind} on {theme}"), &source);
        }
    }
}

#[test]
fn count_up_is_design_clean() {
    for theme in THEMES {
        let source = format!(
            r#"
comp(name="C", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
c = count_up(250, at=0.0, duration=3.0, steps=12, prefix="$", suffix="k")
c.id("count")
c.pos = (500, 300)
"#
        );
        assert_clean(&format!("count_up on {theme}"), &source);
    }
}

#[test]
fn every_component_is_design_clean() {
    let components = [
        (
            "card",
            "g = card(\"Quarterly\", \"Revenue grew 24 percent\")\n",
        ),
        (
            "title_card",
            "g = title_card(\"Launch\", \"A new chapter\")\n",
        ),
        (
            "lower_third",
            "g = lower_third(\"Ana Silva\", \"Design lead\")\n",
        ),
        ("logo_lockup", "g = logo_lockup(\"Motioner\")\n"),
        ("badge", "g = badge(\"New\")\n"),
        ("button", "g = button(\"Get started\")\n"),
        (
            "bar_chart",
            "g = bar_chart([20, 45, 70, 95], [\"Q1\", \"Q2\", \"Q3\", \"Q4\"])\n",
        ),
        ("progress", "g = progress(0.68, \"68 percent\")\n"),
        (
            "background gradient",
            "g = background(\"gradient_drift\")\n",
        ),
        ("background orbs", "g = background(\"glow_orbs\")\n"),
        ("background grain", "g = background(\"grain\")\n"),
    ];
    for theme in THEMES {
        for (label, body) in components {
            let source = format!(
                r#"
comp(name="K", width=1000, height=600, fps=60, duration=4.0, theme="{theme}")
use_theme(theme("{theme}"))
{body}g.id("component")
g.pos = (500, 300)
g.enter("rise", at=0.0, duration=2.0)
"#
            );
            assert_clean(&format!("component {label} on {theme}"), &source);
        }
    }
}
