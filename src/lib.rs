#![allow(clippy::new_without_default)]

mod loc;
pub use loc::Loc;

mod composer;
pub use composer::{AnyData, Composable, ComposeNode, Composer, Node, NodeKey};

mod subcompose;
pub use subcompose::{
    SlotId, SubcomposeHandle, SubcomposeRegistry, SubcomposeScope, Subcomposition,
};

mod recomposer;
pub use recomposer::Recomposer;

mod state;
pub use state::{State, StateId};

mod scope;
pub use scope::{Root, Scope, ScopeId};

pub mod utils;

mod map;
