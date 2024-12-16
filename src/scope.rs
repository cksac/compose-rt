use std::any::Any;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::DerefMut;

use generational_box::GenerationalBox;
use slotmap::SlotMap;

use crate::composer::{Node, NodeKey};
use crate::map::Map;
use crate::{offset_to_anchor, ComposeNode, Composer, State, StateId};

pub struct Scope<S, N>
where
    N: ComposeNode,
{
    pub id: ScopeId,
    composer: GenerationalBox<Composer<N>>,
    ty: PhantomData<S>,
}

impl<S, N> Clone for Scope<S, N>
where
    N: ComposeNode,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, N> Copy for Scope<S, N> where N: ComposeNode {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: ComposeNode,
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
        self.id.set_key(key);
    }

    #[track_caller]
    #[inline(always)]
    pub fn child<C>(&self) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::new(self.id.id);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let mut c = self.composer.write();
        let id = StateId::new(self.id);
        let scope_states = c.states.entry(self.id).or_default();
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
        I: Fn(&mut N::Context) -> A + Clone + 'static,
        A: 'static,
        F: Fn(A, &mut N::Context) -> N + Clone + 'static,
        U: Fn(&mut N, A, &mut N::Context) + Clone + 'static,
    {
        let parent = *self;
        let composable = move || {
            let mut scope = scope;
            let (is_dirty, node_key) = {
                let mut c = parent.composer.write();
                let c = c.deref_mut();
                if let Some(key) = c.key_stack.last().copied() {
                    scope.set_key(key);
                }
                let is_visited = c.scopes.contains_key(&scope.id);
                let is_dirty = c.dirty_scopes.contains(&scope.id);
                let is_initialized = c.initialized;

                if !is_dirty && is_visited {
                    c.skip_scope();
                    return;
                }
                let parent_child_idx = c.start_scope(scope.id);
                let node_key = c.scopes.get(&scope.id).cloned().unwrap();
                let parent_key = c.scopes.get(&parent.id).cloned().unwrap();
                {
                    update_node(
                        node_key,
                        &mut c.context,
                        &mut c.nodes,
                        &input,
                        &factory,
                        &update,
                    );
                    if is_initialized {
                        if let Some(curr_child_idx) = parent_child_idx {
                            let parent_node = c.nodes.get_mut(parent_key).unwrap();
                            if let Some(existing_child) =
                                parent_node.children.get(curr_child_idx).cloned()
                            {
                                if existing_child != node_key {
                                    parent_node.children[curr_child_idx] = node_key;
                                    c.unmount_scopes.insert(existing_child);
                                }
                            } else {
                                parent_node.children.push(node_key);
                                c.mount_scopes.insert(node_key);
                            }
                        }
                    } else if let Some(parent_node) = c.nodes.get_mut(parent_key) {
                        parent_node.children.push(node_key);
                    }
                };
                (is_dirty, node_key)
            };
            content(scope);
            let mut c = parent.composer.write();
            let c = c.deref_mut();
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
        I: Fn(&mut N::Context) -> A + Clone + 'static,
        A: 'static,
        N: AnyData<E>,
        E: 'static,
        F: Fn(A, &mut N::Context) -> E + Clone + 'static,
        U: Fn(&mut E, A, &mut N::Context) + Clone + 'static,
    {
        self.create_node(
            scope,
            content,
            input,
            move |args, ctx| {
                let e = factory(args, ctx);
                AnyData::new(e)
            },
            move |n, args, ctx| {
                let e = n.value_mut();
                update(e, args, ctx);
            },
        );
    }
}

// workaround of borrowing both context and nodes from Composer
// https://smallcultfollowing.com/babysteps/blog/2018/11/01/after-nll-interprocedural-conflicts/
#[inline(always)]
fn update_node<N, I, A, F, U>(
    node_key: NodeKey,
    context: &mut N::Context,
    nodes: &mut SlotMap<NodeKey, Node<N>>,
    input: &I,
    factory: &F,
    update: &U,
) where
    N: ComposeNode,
    I: Fn(&mut N::Context) -> A + Clone + 'static,
    A: 'static,
    F: Fn(A, &mut N::Context) -> N + Clone + 'static,
    U: Fn(&mut N, A, &mut N::Context) + Clone + 'static,
{
    let args = input(context);
    let node = nodes.get_mut(node_key).unwrap();
    if let Some(data) = node.data.as_mut() {
        update(data, args, context);
    } else {
        let data = factory(args, context);
        node.data = Some(data);
    }
}

pub trait AnyData<T> {
    fn new(val: T) -> Self;
    fn value(&self) -> &T;
    fn value_mut(&mut self) -> &mut T;
}

impl<T> AnyData<T> for Box<dyn Any>
where
    T: 'static,
{
    fn new(val: T) -> Self {
        Box::new(val)
    }

    fn value(&self) -> &T {
        self.downcast_ref::<T>().unwrap()
    }

    fn value_mut(&mut self) -> &mut T {
        self.downcast_mut::<T>().unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId {
    pub parent: u64,
    pub id: u64,
}

impl ScopeId {
    #[inline(always)]
    pub fn with(parent: u64, id: u64) -> Self {
        Self { parent, id }
    }

    #[track_caller]
    #[inline(always)]
    pub fn new(parent: u64) -> Self {
        let id = (offset_to_anchor() as u64) << 32;
        Self { parent, id }
    }

    #[inline(always)]
    pub fn set_key(&mut self, key: u32) {
        self.id = self.id + key as u64;
    }
}

// impl From<u64> for ScopeId {
//     fn from(id: u64) -> Self {
//         Self(id)
//     }
// }

// impl From<ScopeId> for u64 {
//     fn from(id: ScopeId) -> Self {
//         id.0
//     }
// }

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeId({}, {})", self.parent, self.id)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;
