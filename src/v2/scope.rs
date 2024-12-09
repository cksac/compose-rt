use std::{
    fmt::{self, Debug, Formatter},
    hash::Hash,
    marker::PhantomData,
    usize,
};

use ahash::AHashMap;
use generational_box::GenerationalBox;

use crate::{
    v2::{Composer, State, StateId},
    Loc,
};

pub struct Scope<S, N> {
    _scope: PhantomData<S>,
    pub id: ScopeId,
    pub(crate) composer: GenerationalBox<Composer<N>>,
}

impl<S, N> Clone for Scope<S, N> {
    fn clone(&self) -> Self {
        Self {
            _scope: PhantomData,
            id: self.id,
            composer: self.composer.clone(),
        }
    }
}

impl<S, N> Copy for Scope<S, N> {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: Debug + 'static,
{
    pub fn new(id: ScopeId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            _scope: PhantomData,
            id,
            composer,
        }
    }

    #[track_caller]
    pub fn child_scope<C>(&self) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::with_key(self.id.key, self.id.depth + 1);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn child_scope_with_key<C>(&self, key: usize) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::with_key(key, self.id.depth + 1);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let c = self.composer.read();
        let (scope, scope_key) = c.get_current_scope();
        let id = StateId::new();
        let mut states = c.states.borrow_mut();
        if !states.contains_key(scope_key) {
            states.insert(scope_key, AHashMap::new());
        }
        let scope_states = states.get_mut(scope_key).unwrap();
        scope_states.insert(id, Box::new(init()));
        State::new(scope, id, self.composer.clone())
    }

    #[track_caller]
    pub fn key<C>(&self, key: usize, content: C)
    where
        C: Fn(Scope<Key<S>, N>) + 'static,
    {
        let c = self.composer.read();
        let scope = self.child_scope_with_key::<Key<S>>(key);
        c.create_scope(*self, scope, content);
    }

    pub fn build_child<C, T, I, A, F, U>(
        &self,
        scope: Scope<T, N>,
        content: C,
        input: I,
        factory: F,
        update: U,
    ) where
        T: 'static,
        C: Fn(Scope<T, N>) + 'static,
        I: Fn() -> A + 'static,
        A: 'static,
        F: Fn(A) -> N + 'static,
        U: Fn(&mut N, A) + 'static,
    {
        let c = self.composer.read();
        c.create_scope_with_node(*self, scope, content, input, factory, update);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopeId {
    pub depth: usize,
    pub loc: Loc,
    pub key: usize,
}

impl Hash for ScopeId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.key.hash(state);
    }
}

impl ScopeId {
    #[track_caller]
    #[inline]
    pub fn new(depth: usize) -> Self {
        let loc = Loc::new();
        Self { loc, key: 0, depth }
    }

    #[track_caller]
    #[inline]
    pub fn with_key(key: usize, depth: usize) -> Self {
        let loc = Loc::new();
        Self { loc, key, depth }
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}-{}-{}", self.loc, self.key, self.depth)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;

#[derive(Debug, Clone, Copy)]
pub struct Key<S> {
    _scope: PhantomData<S>,
}

impl<S> Key<S> {
    pub fn new() -> Self {
        Self {
            _scope: PhantomData,
        }
    }
}
