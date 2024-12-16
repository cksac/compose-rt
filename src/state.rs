use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::ops::DerefMut;

use generational_box::GenerationalBox;

use crate::{ComposeNode, Composer, Loc, ScopeId};

pub struct State<T, N>
where
    N: ComposeNode,
{
    pub id: StateId,
    composer: GenerationalBox<Composer<N>>,
    ty: PhantomData<T>,
}

impl<T, N> State<T, N>
where
    T: 'static,
    N: ComposeNode,
{
    #[inline(always)]
    pub(crate) fn new(id: StateId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            id,
            composer,
            ty: PhantomData,
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let mut c = self.composer.write();
        let c = c.deref_mut();
        let current_node = c.current_node;
        let current_scope = c.nodes[current_node].scope;
        let used_by = c.used_by.entry(self.id).or_default();
        used_by.insert(current_scope);
        let uses = c.uses.entry(current_scope).or_default();
        uses.insert(self.id);
        let scope_states = c.states.get(&self.id.scope_id).unwrap();
        let any_state = scope_states.get(&self.id).unwrap();
        let state = any_state.downcast_ref::<T>().unwrap();
        state.clone()
    }

    pub fn set(&self, value: T) {
        let mut c = self.composer.write();
        let c = c.deref_mut();
        c.dirty_states.insert(self.id);
        let scope_states = c.states.entry(self.id.scope_id).or_default();
        let val = scope_states.get_mut(&self.id).unwrap();
        *val = Box::new(value);
    }
}

impl<T, N> Debug for State<T, N>
where
    N: ComposeNode,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("id", &self.id)
            .field("ty", &self.ty)
            .finish()
    }
}

impl<T, N> Clone for State<T, N>
where
    N: ComposeNode,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, N> Copy for State<T, N> where N: ComposeNode {}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId {
    pub(crate) scope_id: ScopeId,
    loc: Loc,
}

impl StateId {
    #[track_caller]
    #[inline(always)]
    pub fn new(scope_id: ScopeId) -> Self {
        Self {
            scope_id,
            loc: Loc::new(),
        }
    }
}

impl Debug for StateId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "StateId({:?},{:?})", self.scope_id, self.loc)
    }
}
