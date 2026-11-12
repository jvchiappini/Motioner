//! JSON output for the rig manifest (D-0056).
//!
//! The writer is the same [`Json`] used by `query` and `state`, so field order
//! is fixed and the output is deterministic. The schema lives in
//! `motioner_tools::catalogue::result_schema("rig_manifest")`.

use motioner_core::{
    RigClipManifest, RigInstanceManifest, RigManifest, RigPropertyManifest, RigPropertyValue,
    RigValue,
};

use crate::command::{CommandError, ErrorKind};
use crate::json::Json;
use crate::Editor;

impl Editor {
    /// The rig manifest as JSON: every instance, or only `instance` when
    /// given (matched by root identity or display name).
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NoDocument`] when nothing loaded,
    /// [`ErrorKind::UnknownNode`] when `instance` matches no rig.
    pub fn rig_manifest_json(&self, instance: Option<&str>) -> Result<String, CommandError> {
        let document = self
            .session()
            .document()
            .ok_or_else(|| CommandError::new(ErrorKind::NoDocument, "no document"))?;
        let manifest = document.rig_manifest();
        let rigs: Vec<&RigInstanceManifest> = match instance {
            Some(name) => {
                let rig = manifest
                    .rigs
                    .iter()
                    .find(|rig| rig.id == name || rig.name == name)
                    .ok_or_else(|| {
                        CommandError::new(
                            ErrorKind::UnknownNode,
                            format!("no rig named {name:?}; known rigs: {}", known(manifest)),
                        )
                    })?;
                vec![rig]
            }
            None => manifest.rigs.iter().collect(),
        };
        let mut json = Json::new();
        json.begin_object();
        json.key("version").int(i64::from(manifest.version));
        json.key("rigs").begin_array();
        for rig in rigs {
            rig_json(&mut json, rig);
        }
        json.end_array();
        json.end_object();
        Ok(json.finish())
    }
}

/// The names and ids of the known rigs, for the error message.
fn known(manifest: &RigManifest) -> String {
    manifest
        .rigs
        .iter()
        .map(|rig| format!("{} ({})", rig.name, rig.id))
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(clippy::too_many_lines)] // one straight writer per manifest section
fn rig_json(json: &mut Json, rig: &RigInstanceManifest) {
    json.begin_object();
    json.key("id").string(&rig.id);
    json.key("name").string(&rig.name);
    json.key("parts").begin_array();
    for part in &rig.parts {
        json.begin_object();
        json.key("id").string(&part.id);
        json.key("node").string(&part.node);
        json.end_object();
    }
    json.end_array();
    json.key("properties").begin_array();
    for property in &rig.properties {
        property_json(json, property);
    }
    json.end_array();
    json.key("poses").begin_array();
    for pose in &rig.poses {
        json.begin_object();
        json.key("name").string(&pose.name);
        json.key("reset").boolean(pose.reset);
        json.key("values").begin_array();
        for value in &pose.values {
            property_value_json(json, value);
        }
        json.end_array();
        json.end_object();
    }
    json.end_array();
    json.key("pose_tracks").begin_array();
    for track in &rig.pose_tracks {
        json.begin_object();
        json.key("pose").string(&track.pose);
        json.key("start").number(track.start);
        json.key("duration").number(track.duration);
        json.key("ease").string(&track.ease);
        json.end_object();
    }
    json.end_array();
    json.key("clips").begin_array();
    for clip in &rig.clips {
        clip_json(json, clip);
    }
    json.end_array();
    json.key("plays").begin_array();
    for play in &rig.plays {
        json.begin_object();
        json.key("clip").string(&play.clip);
        json.key("start").number(play.start);
        json.key("loops").int(i64::from(play.loops));
        json.key("speed").number(play.speed);
        json.end_object();
    }
    json.end_array();
    json.key("layers").begin_array();
    for layer in &rig.layers {
        json.begin_object();
        json.key("kind").string(&layer.kind);
        json.key("start").number(layer.start);
        json.key("period").number(layer.period);
        json.key("jitter").number(layer.jitter);
        json.key("duration").number(layer.duration);
        json.key("seed")
            .int(i64::try_from(layer.seed).unwrap_or(i64::MAX));
        json.key("values").begin_array();
        for value in &layer.values {
            property_value_json(json, value);
        }
        json.end_array();
        json.end_object();
    }
    json.end_array();
    json.key("aims").begin_array();
    for aim in &rig.aims {
        json.begin_object();
        json.key("part").string(&aim.part);
        json.key("property").string(&aim.property);
        json.key("radius").number(aim.radius);
        json.end_object();
    }
    json.end_array();
    json.key("machine");
    match &rig.machine {
        Some(machine) => {
            json.begin_object();
            json.key("default").string(&machine.default);
            json.key("inputs").begin_array();
            for input in &machine.inputs {
                json.begin_object();
                json.key("name").string(&input.name);
                json.key("kind").string(input.kind.name());
                json.key("values").begin_array();
                for value in &input.values {
                    json.string(value);
                }
                json.end_array();
                json.end_object();
            }
            json.end_array();
            json.key("states").begin_array();
            for state in &machine.states {
                json.begin_object();
                json.key("name").string(&state.name);
                json.key("pose").opt_string(state.pose.as_deref());
                json.key("clip").opt_string(state.clip.as_deref());
                json.key("speed").number(state.speed);
                json.key("looping").boolean(state.looping);
                json.end_object();
            }
            json.end_array();
            json.key("transitions").begin_array();
            for transition in &machine.transitions {
                json.begin_object();
                json.key("from").opt_string(transition.from.as_deref());
                json.key("to").string(&transition.to);
                json.key("input").string(&transition.input);
                json.key("compare").string(&transition.compare);
                json.key("value");
                value_json(json, &transition.value);
                json.key("duration").number(transition.duration);
                json.end_object();
            }
            json.end_array();
            json.key("events").begin_array();
            for event in &machine.events {
                json.begin_object();
                json.key("from").opt_string(event.from.as_deref());
                json.key("to").string(&event.to);
                json.end_object();
            }
            json.end_array();
            json.end_object();
        }
        None => {
            json.null();
        }
    }
    json.end_object();
}

fn property_json(json: &mut Json, property: &RigPropertyManifest) {
    json.begin_object();
    json.key("name").string(&property.name);
    json.key("kind").string(property.kind.name());
    json.key("angle").boolean(property.angle);
    json.key("default");
    value_json(json, &property.default);
    json.key("min");
    opt_number(json, property.min);
    json.key("max");
    opt_number(json, property.max);
    json.key("step").number(property.step);
    json.key("values").begin_array();
    for value in &property.values {
        json.string(value);
    }
    json.end_array();
    json.key("group").opt_string(property.group.as_deref());
    json.key("mirror");
    match &property.mirror {
        Some(mirror) => {
            json.begin_object();
            json.key("group").string(&mirror.group);
            json.key("side").string(mirror.side.name());
            json.key("flip").boolean(mirror.flip);
            json.end_object();
        }
        None => {
            json.null();
        }
    }
    json.key("keys").begin_array();
    for key in &property.keys {
        json.begin_object();
        json.key("frame").number(key.frame);
        json.key("value");
        value_json(json, &key.value);
        json.key("ease").string(&key.ease);
        json.end_object();
    }
    json.end_array();
    json.end_object();
}

fn clip_json(json: &mut Json, clip: &RigClipManifest) {
    json.begin_object();
    json.key("name").string(&clip.name);
    json.key("length").number(clip.length);
    json.key("keys").begin_array();
    for key in &clip.keys {
        json.begin_object();
        json.key("property").string(&key.property);
        json.key("frame").number(key.frame);
        json.key("value");
        value_json(json, &key.value);
        json.key("ease").string(&key.ease);
        json.end_object();
    }
    json.end_array();
    json.end_object();
}

fn property_value_json(json: &mut Json, value: &RigPropertyValue) {
    json.begin_object();
    json.key("property").string(&value.property);
    json.key("value");
    value_json(json, &value.value);
    json.end_object();
}

fn value_json(json: &mut Json, value: &RigValue) {
    match value {
        RigValue::Number(number) => {
            json.number(*number);
        }
        RigValue::Bool(flag) => {
            json.boolean(*flag);
        }
        RigValue::Text(text) => {
            json.string(text);
        }
        RigValue::Vec2(vector) => {
            json.begin_array();
            json.number(vector.x);
            json.number(vector.y);
            json.end_array();
        }
        RigValue::Color(color) => {
            json.string(&color.to_hex());
        }
    }
}

fn opt_number(json: &mut Json, value: Option<f64>) {
    match value {
        Some(value) => {
            json.number(value);
        }
        None => {
            json.null();
        }
    }
}
