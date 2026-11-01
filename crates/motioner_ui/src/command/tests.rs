use super::*;

#[test]
fn every_kind_has_a_unique_name_and_its_example_round_trips() {
    let mut names = std::collections::BTreeSet::new();
    for kind in CommandKind::ALL {
        assert!(names.insert(kind.name()), "duplicate name {}", kind.name());
        assert_eq!(CommandKind::from_name(kind.name()), Some(kind));
        assert!(kind.signature().starts_with(kind.name()));
        let example = kind.example();
        assert_eq!(example.kind(), kind, "example of {kind:?}");
        let text = example.to_text();
        let parsed = parse_commands(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(parsed, vec![example], "{text}");
    }
}

#[test]
fn several_commands_parse_from_one_text() {
    let commands = parse_commands("seek(10)\nselect(\"title\")\nloop(True)\n").unwrap();
    assert_eq!(
        commands,
        vec![
            Command::Seek(10),
            Command::Select("title".to_owned()),
            Command::Loop(true)
        ]
    );
}

#[test]
fn keyword_arguments_and_defaults() {
    assert_eq!(
        parse_commands("key(\"a\", \"opacity\", time=1, value=0.5)").unwrap(),
        vec![Command::Key {
            id: "a".to_owned(),
            property: "opacity".to_owned(),
            time: Time::Seconds(1.0),
            value: OverrideValue::Number(0.5),
            ease: None,
        }]
    );
    assert_eq!(
        parse_commands("expand(\"a\")").unwrap(),
        vec![Command::Expand {
            id: "a".to_owned(),
            open: true
        }]
    );
    assert_eq!(
        parse_commands("bake()").unwrap(),
        vec![Command::Bake(Vec::new())]
    );
}

#[test]
fn errors_are_typed() {
    let kind = |text: &str| parse_commands(text).unwrap_err().kind;
    assert_eq!(kind("seek("), ErrorKind::Parse);
    assert_eq!(kind("x = 3"), ErrorKind::Parse);
    assert_eq!(kind("fly()"), ErrorKind::UnknownCommand);
    assert_eq!(kind("seek()"), ErrorKind::BadArgument);
    assert_eq!(kind("seek(1.5)"), ErrorKind::BadArgument);
    assert_eq!(kind("seek(1, 2)"), ErrorKind::BadArgument);
    assert_eq!(kind("select(3)"), ErrorKind::BadArgument);
    assert_eq!(kind("set_ease(\"wobble\")"), ErrorKind::BadArgument);
    assert_eq!(kind("loop(1)"), ErrorKind::BadArgument);
    assert_eq!(kind("panel_size(\"canvas\", 3)"), ErrorKind::BadArgument);
    assert_eq!(kind("seek(frames=3)"), ErrorKind::BadArgument);
    let message = parse_commands("unkey(\"a\")").unwrap_err().message;
    assert!(
        message.contains("usage: unkey(id, property, time)"),
        "{message}"
    );
}

#[test]
fn times_convert_with_the_rate() {
    let rate = FrameRate::new(60, 1).unwrap();
    assert_eq!(Time::Seconds(0.8).frame(rate).unwrap(), 48);
    assert_eq!(Time::Frame(7).frame(rate).unwrap(), 7);
    assert!(Time::Seconds(1e300).frame(rate).is_err());
}

#[test]
fn only_editing_commands_touch_the_history() {
    assert!(CommandKind::Key.edits());
    assert!(CommandKind::Visible.edits());
    assert!(!CommandKind::Seek.edits());
    assert!(!CommandKind::Undo.edits());
    assert!(!CommandKind::Bake.edits());
}
