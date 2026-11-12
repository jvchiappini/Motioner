//! Core document model for Motioner.
//!
//! This crate owns the compiled project representation: persistent node
//! IDs, the node hierarchy, animatable values and keyframes, colors and
//! the rational time base. It is the root of the dependency graph and must
//! never depend on graphics, UI, AI or platform crates.
//!
//! The document is produced by executing an MDF program (see
//! `motioner_dsl`); every mutation goes through the [`Document`] API so
//! there is one validated path into the model.

mod animation;
mod color;
mod document;
mod edit;
mod effect;
mod error;
mod gradient;
mod mask;
mod math;
mod node;
mod overrides;
mod path;
mod rig_manifest;
mod symbol;
mod text;
mod time;

pub mod bounds;

pub use animation::{Animatable, AnimatableVec2, Axis, BezierEase, BezierError, Ease, Keyframe};
pub use color::{Color, ColorParseError};
pub use document::{Composition, Document, MAX_CANVAS_SIZE};
pub use edit::{EditCommand, KeyRef, MoveKey};
pub use effect::{
    BlurEffect, DropShadowEffect, Effect, EffectChannel, EffectChannelMut, EffectChannelRef,
    EffectKind, EffectParam, EffectValueInput, GlowEffect,
};
pub use error::CoreError;
pub use gradient::{Gradient, GradientKind, GradientStop, PaintSlot, StopPart, MAX_GRADIENT_STOPS};
pub use mask::{MaskMode, MaskSpec};
pub use math::Vec2;
pub use node::{
    is_valid_identity_name, AudioContent, BlendMode, CharOrder, CharProperty, CharValue,
    ContentChannel, ContentField, ContentValue, DecodedImage, ImageAsset, ImageContent, ImageFit,
    ImageIssue, ImageKey, ImageLayout, ImageResolver, NoImages, NoVideos, Node, NodeContent,
    NodeId, NodeIdentity, Paint, PaintSpec, Property, PropertyValue, Stroke, TextAlign,
    TextAnimator, TextContent, Transform, VideoAsset, VideoContent, VideoResolver,
};
pub use overrides::{
    format_number, quote, seconds_of, split_property, value_text, OverrideAction, OverrideEntry,
    OverrideIssue, OverrideSet, OverrideValue, EFFECTS, LOCKED, MASK, NAME, VISIBLE,
};
pub use path::{
    DashSpec, GeometryChannel, GeometryKey, LineCap, LineJoin, PathGeometry, PathOutline,
    PathPiece, PathPoint, TrimSpec, FLATTEN_TOLERANCE, MAX_ARC_SEGMENTS,
};
pub use rig_manifest::{
    RigAimManifest, RigClipKeyManifest, RigClipManifest, RigEventManifest, RigInputManifest,
    RigInstanceManifest, RigLayerManifest, RigMachineManifest, RigManifest, RigMirrorManifest,
    RigMirrorSide, RigPartManifest, RigPlayManifest, RigPoseManifest, RigPoseTrackManifest,
    RigPropertyKeyManifest, RigPropertyManifest, RigPropertyValue, RigStateManifest,
    RigTransitionManifest, RigValue, RigValueKind, RIG_MANIFEST_VERSION,
};
pub use symbol::{Symbol, SymbolPart, SymbolPath, SymbolShape};
pub use text::{NoTextMetrics, TextBounds, TextMeasurement, TextMetrics, TextRequest};
pub use time::{Frame, FrameRate, MAX_ABS_FRAME};
