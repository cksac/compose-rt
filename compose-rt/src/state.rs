use std::{fmt::Debug, marker::PhantomData, panic::Location};

use crate::{composer::Cx, scope::Loc, ScopeId};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateId(Loc);

impl StateId {
    #[track_caller]
    pub fn new() -> Self {
        Self(Location::caller())
    }
}

impl Debug for StateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("StateId({})", self.0).as_str())
    }
}

pub struct State<N, T> {
    ty: PhantomData<T>,
    cx: Cx<N>,
    scope: ScopeId,
    id: StateId,
}

impl<N, T> State<N, T>
where
    N: 'static,
{
    pub fn new(cx: Cx<N>, scope: ScopeId, id: StateId) -> Self {
        Self {
            ty: PhantomData,
            cx,
            scope,
            id,
        }
    }

    pub fn get(&self) -> T
    where
        T: 'static + Clone,
    {
        let c = self.cx.composer.read();
        let current_scope = c.current_scope();
        // update subscribers
        let mut subscribers = c.subscribers.write().unwrap();
        let scope_subscribers = subscribers.entry(self.id).or_default();
        scope_subscribers.insert(current_scope);
        // get state
        let mut states = c.states.write().unwrap();
        let scope_states = states.entry(self.scope).or_default();
        let val: &T = scope_states.get(&self.id).unwrap().downcast_ref().unwrap();
        val.clone()
    }

    pub fn with<F, U>(&self, func: F) -> U
    where
        T: 'static,
        F: FnOnce(&T) -> U,
    {
        let c = self.cx.composer.read();
        let current_scope = c.current_scope();
        // update subscribers
        let mut subscribers = c.subscribers.write().unwrap();
        let scope_subscribers = subscribers.entry(self.id).or_default();
        scope_subscribers.insert(current_scope);
        // get state
        let mut states = c.states.write().unwrap();
        let scope_states = states.entry(self.scope).or_default();
        let val: &T = scope_states.get(&self.id).unwrap().downcast_ref().unwrap();
        func(val)
    }

    pub fn set(&self, value: T)
    where
        T: 'static,
    {
        let c = self.cx.composer.read();
        // get state
        let mut states = c.states.write().unwrap();
        let scope_states = states.entry(self.scope).or_default();
        let val = scope_states.get_mut(&self.id).unwrap();
        // udate dirty states
        let mut dirty_states = c.dirty_states.write().unwrap();
        dirty_states.insert(self.id);
        // set state
        *val = Box::new(value);
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn id(&self) -> StateId {
        self.id
    }

    pub fn debug_string(&self) -> String {
        format!("State({:?}, {})", self.scope, self.id.0)
    }
}

impl<N, T> Clone for State<N, T>
where
    N: 'static,
{
    fn clone(&self) -> Self {
        Self {
            ty: self.ty,
            cx: self.cx,
            scope: self.scope,
            id: self.id,
        }
    }
}

impl<N, T> Copy for State<N, T> where N: 'static {}
