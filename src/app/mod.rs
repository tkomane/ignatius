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

pub mod completion;
pub mod connection_picker;
pub mod discovery;
pub mod editor;
pub mod grid;
pub mod inspect;
pub mod message;
pub mod model;
pub mod palette;
pub mod plan;
pub mod tree;
pub mod update;

pub use completion::{CatalogStatus, CompletionMenu, CompletionState};
pub use connection_picker::ConnectionProfileSummary;
pub use discovery::{
    ConnectionPosture, DiscoveryContext, HistoryPosture, KeyBindingSnapshot, KeyChordSnapshot,
    KeymapSnapshot, ObjectPosture, ResultPosture,
};
pub use inspect::{CellView, ExpandedField, Inspector};
pub use message::{Action, Direction, Effect, Message, PlanExecution};
pub use model::{ConnectionState, Editor, Focus, Model, ParameterPrompt, QueryPhase};
pub use palette::{Palette, PaletteCommand, PaletteEntry, Purpose};
pub use plan::{PlanStatus, PlanView};
pub use tree::{MetadataPayload, MetadataQuery, NodePath, ObjectTree, RequestId, RowKind, TreeRow};
pub use update::update;
