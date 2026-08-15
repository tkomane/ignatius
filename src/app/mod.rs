//! Application model, messages, reducer and effects.
//!
//! The flow is one direction only:
//!
//! ```text
//! input or async event -> Message -> update(Model, Message) -> Vec<Effect>
//!                                          |                        |
//!                                          v                        v
//!                                      new Model              runtime performs
//!                                          |                   the effect, which
//!                                          v                   produces Messages
//!                                      View(Model)
//! ```
//!
//! [`update::update`] is pure, so every rule it enforces is provable in a unit
//! test rather than by running a database.

pub mod message;
pub mod model;
pub mod update;

pub use message::{Action, Direction, Effect, Message};
pub use model::{ConnectionState, Editor, Focus, Model, QueryPhase};
pub use update::update;
