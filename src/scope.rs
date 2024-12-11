use std::any::Any;
use std::cell::RefCell;
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;

use generational_box::GenerationalBox;

use crate::composer::{HashMapExt, Map};
use crate::{Composer, Loc, State, StateId};

pub struct Scope<S, N> {
    _scope: PhantomData<S>,
    pub id: ScopeId,
    pub(crate) composer: GenerationalBox<Composer<N>>,
}

impl<S, N> Clone for Scope<S, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, N> Copy for Scope<S, N> {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: Debug + 'static,
{
    #[inline(always)]
    pub(crate) fn new(id: ScopeId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            _scope: PhantomData,
            id,
            composer,
        }
    }

    #[inline(always)]
    pub(crate) fn set_key(&mut self, key: u32) {
        self.id.0 += key as u64;
    }

    #[track_caller]
    #[inline(always)]
    pub fn child_scope<C>(&self) -> Scope<C, N>
    where
        C: 'static,
    {
        let c = self.composer.read();
        let id = c.new_scope();
        Scope::new(id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn child_scope_with_key<C>(&self, key: u32) -> Scope<C, N>
    where
        C: 'static,
    {
        let c = self.composer.read();
        let id = c.new_keyed_scope(key);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let c = self.composer.read();
        let scope_id = self.id;
        let id = StateId::new();
        let mut state_data = c.state_data.borrow_mut();
        let scope_states = state_data.states.entry(scope_id).or_default();
        let _ = scope_states.entry(id).or_insert_with(|| Box::new(init()));
        State::new(scope_id, id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn key<C>(&self, key: u32, content: C)
    where
        C: Fn(Self) + 'static,
    {
        let c = self.composer.read();
        c.key_stack.borrow_mut().push(key);
        content(*self);
        c.key_stack.borrow_mut().pop();
    }

    pub fn create_node<C, T, I, A, F, U>(
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
        c.create_node_scope(*self, scope, content, input, factory, update);
    }

    pub fn create_any_node<C, T, I, A, E, F, U>(
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
        N: AnyNode<E>,
        E: 'static,
        F: Fn(A) -> E + 'static,
        U: Fn(&mut E, A) + 'static,
    {
        let c = self.composer.read();
        c.create_node_scope(
            *self,
            scope,
            content,
            input,
            move |args| {
                let e = factory(args);
                AnyNode::new(e)
            },
            move |n, args| {
                let e = n.val_mut();
                update(e, args);
            },
        );
    }
}

pub trait AnyNode<T> {
    fn new(val: T) -> Self;
    fn val(&self) -> &T;
    fn val_mut(&mut self) -> &mut T;
}

impl<T> AnyNode<T> for Box<dyn Any>
where
    T: 'static,
{
    fn new(val: T) -> Self {
        Box::new(val)
    }

    fn val(&self) -> &T {
        self.downcast_ref::<T>().unwrap()
    }

    fn val_mut(&mut self) -> &mut T {
        self.downcast_mut::<T>().unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u64);

impl ScopeId {
    #[track_caller]
    #[inline]
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;
