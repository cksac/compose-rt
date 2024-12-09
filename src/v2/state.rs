use std::{
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
};

use generational_box::GenerationalBox;

use crate::{
    v2::{Composer, ScopeId},
    Loc,
};

pub struct State<T, N> {
    ty: PhantomData<T>,
    scope_id: ScopeId,
    id: StateId,
    composer: GenerationalBox<Composer<N>>,
}

impl<T, N> State<T, N>
where
    T: 'static,
    N: Debug + 'static,
{
    pub(crate) fn new(
        scope_id: ScopeId,
        id: StateId,
        composer: GenerationalBox<Composer<N>>,
    ) -> Self {
        Self {
            ty: PhantomData,
            scope_id,
            id,
            composer,
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let c = self.composer.read();
        // add current_scope to subscribers
        let (current_scope, scope_key) = c.get_current_scope();
        let mut subscribers = c.subscribers.borrow_mut();
        let state_subscribers = subscribers.entry(self.id).or_default();
        state_subscribers.insert(current_scope);
        // get state
        let state_scope_key = c.scope_keys.borrow().get(&self.scope_id).cloned().unwrap();
        let states = c.states.borrow();
        let scope_states = states.get(state_scope_key).unwrap();
        let any_state = scope_states.get(&self.id).unwrap();
        let state = any_state.downcast_ref::<T>().unwrap();
        state.clone()
    }

    pub fn set(&self, value: T) {
        let c = self.composer.read();
        // update dirty states
        let mut dirty_states = c.dirty_states.borrow_mut();
        dirty_states.insert(self.id);
        // update state
        let state_scope_key = c.scope_keys.borrow().get(&self.scope_id).cloned().unwrap();
        let mut states = c.states.borrow_mut();
        let scope_states = states.get_mut(state_scope_key).unwrap();
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
        Self {
            ty: PhantomData,
            scope_id: self.scope_id,
            id: self.id,
            composer: self.composer.clone(),
        }
    }
}

impl<T, N> Copy for State<T, N> {}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId {
    pub loc: Loc,
}

impl StateId {
    #[track_caller]
    pub fn new() -> Self {
        let loc = Loc::new();
        Self { loc }
    }
}

impl Debug for StateId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.loc)
    }
}
