use std::any::Any;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;

use generational_box::GenerationalBox;

use crate::composer::Node;
use crate::{offset_to_anchor, Composer, State, StateId};

pub struct Scope<S, N> {
    pub id: ScopeId,
    composer: GenerationalBox<Composer<N>>,
    ty: PhantomData<S>,
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
            id,
            composer,
            ty: PhantomData,
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
        let id = ScopeId::new();
        Scope::new(id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn child_scope_with_key<C>(&self, key: u32) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::with_key(key);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let scope_id = self.id;
        let mut c = self.composer.write();
        let scope_states = c.states.entry(scope_id).or_default();
        let id = StateId::new(scope_id);
        let _ = scope_states.entry(id).or_insert_with(|| Box::new(init()));
        State::new(id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn key<C>(&self, key: u32, content: C)
    where
        C: Fn(Self) + 'static,
    {
        self.composer.write().key_stack.push(key);
        content(*self);
        self.composer.write().key_stack.pop();
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
        C: Fn(Scope<T, N>) + Clone + 'static,
        I: Fn() -> A + Clone + 'static,
        A: 'static,
        F: Fn(A) -> N + Clone + 'static,
        U: Fn(&mut N, A) + Clone + 'static,
    {
        let parent = *self;
        let composable = move || {
            let mut scope = scope;
            let mut c = parent.composer.write();
            if let Some(key) = c.key_stack.last().cloned() {
                scope.set_key(key);
            }
            let is_visited = c.nodes.contains_key(&scope.id);
            let is_dirty = c.dirty_scopes.contains(&scope.id);
            let is_initialized = c.initialized;
            if !is_dirty && is_visited {
                c.skip_scope();
                return;
            }
            let parent_child_idx = c.start_scope(scope.id);
            {
                let input = input();
                match c.nodes.entry(scope.id) {
                    Occupied(mut entry) => {
                        let node = entry.get_mut();
                        if let Some(data) = node.data.as_mut() {
                            update(data, input);
                        } else {
                            let data = factory(input);
                            node.data = Some(data);
                        }
                    }
                    Vacant(entry) => {
                        let data = factory(input);
                        entry.insert(Node {
                            data: Some(data),
                            parent: parent.id,
                            children: Vec::new(),
                        });
                    }
                }
                if is_initialized {
                    if let Some(curr_child_idx) = parent_child_idx {
                        let parent_node = c.nodes.get_mut(&parent.id).unwrap();
                        if let Some(existing_child) =
                            parent_node.children.get(curr_child_idx).cloned()
                        {
                            if existing_child != scope.id {
                                parent_node.children[curr_child_idx] = scope.id;
                                c.unmount_scopes.insert(existing_child);
                            }
                        } else {
                            parent_node.children.push(scope.id);
                            c.mount_scopes.insert(scope.id);
                        }
                    }
                } else if let Some(parent_node) = c.nodes.get_mut(&parent.id) {
                    parent_node.children.push(scope.id);
                }
            }
            drop(c);
            content(scope);
            let mut c = parent.composer.write();
            if is_dirty {
                c.dirty_scopes.remove(&scope.id);
            }
            c.end_scope(parent.id, scope.id);
        };
        composable();
        let mut c = self.composer.write();
        c.composables
            .entry(scope.id)
            .or_insert_with(|| Box::new(composable));
    }

    #[inline(always)]
    pub fn create_any_node<C, T, I, A, E, F, U>(
        &self,
        scope: Scope<T, N>,
        content: C,
        input: I,
        factory: F,
        update: U,
    ) where
        T: 'static,
        C: Fn(Scope<T, N>) + Clone + 'static,
        I: Fn() -> A + Clone + 'static,
        A: 'static,
        N: AnyNode<E>,
        E: 'static,
        F: Fn(A) -> E + Clone + 'static,
        U: Fn(&mut E, A) + Clone + 'static,
    {
        self.create_node(
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
pub struct ScopeId(pub(crate) u64);

impl ScopeId {
    #[track_caller]
    #[inline(always)]
    pub fn new() -> Self {
        let id = (offset_to_anchor() as u64) << 32;
        Self(id)
    }

    #[track_caller]
    #[inline(always)]
    pub fn with_key(key: u32) -> Self {
        let mut scope = Self::new();
        scope.0 += key as u64;
        scope
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeId({})", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;
