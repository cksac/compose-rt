#![allow(clippy::new_without_default)]

mod loc;
pub use loc::Loc;

mod composer;
pub use composer::{Composable, ComposeNode, Composer, NodeKey};

mod recomposer;
pub use recomposer::Recomposer;

mod state;
pub use state::{State, StateId};

mod scope;
pub use scope::{AnyData, Root, Scope, ScopeId};

pub mod utils;

mod map;

const ANCHOR: Loc = Loc::new();

#[track_caller]
#[inline(always)]
pub(crate) fn offset_to_anchor() -> i64 {
    let anchor = ANCHOR.id() as i64;
    let loc = Loc::new().id() as i64;
    anchor - loc
}
