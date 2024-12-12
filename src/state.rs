use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;

use generational_box::GenerationalBox;

use crate::{Composer, Loc, ScopeId};

pub struct State<T, N> {
    ty: PhantomData<T>,
    id: StateId,
    composer: GenerationalBox<Composer<N>>,
}

impl<T, N> State<T, N>
where
    T: 'static,
    N: Debug + 'static,
{
    #[inline(always)]
    pub(crate) fn new(id: StateId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            ty: PhantomData,
            id,
            composer,
        }
    }

    #[inline(always)]
    pub fn scope_id(&self) -> ScopeId {
        self.id.scope_id
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let c = self.composer.read();
        let mut state_data = c.state_data.borrow_mut();
        // add current_scope to subscribers
        let current_scope = c.get_current_scope();
        let state_subscribers = state_data.subscribers.entry(self.id).or_default();
        state_subscribers.insert(current_scope);
        // add state to scope uses
        let scope_uses = state_data.uses.entry(current_scope).or_default();
        scope_uses.insert(self.id);
        // get state
        let scope_states = state_data.states.get(&self.scope_id()).unwrap();
        let any_state = scope_states.get(&self.id).unwrap();
        let state = any_state.downcast_ref::<T>().unwrap();
        state.clone()
    }

    pub fn set(&self, value: T) {
        let c = self.composer.read();
        let mut state_data = c.state_data.borrow_mut();
        // update dirty states
        state_data.dirty_states.insert(self.id);
        // update state
        let scope_states = state_data.states.entry(self.scope_id()).or_default();
        let val = scope_states.get_mut(&self.id).unwrap();
        *val = Box::new(value);
    }
}

impl<T, N> Debug for State<T, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("id", &self.id)
            .field("ty", &self.ty)
            .finish()
    }
}

impl<T, N> Clone for State<T, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, N> Copy for State<T, N> {}

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
        write!(f, "StateId({:?},{:?})", self.scope_id.0, self.loc)
    }
}
