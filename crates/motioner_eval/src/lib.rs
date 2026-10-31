//! Deterministic evaluation for Motioner.
//!
//! Turns a document version and a frame number into a flat render plan by
//! resolving timing, sampling animation curves, evaluating expressions
//! and propagating transforms. It is pure: no wall clock, no ambient
//! randomness and no I/O.
