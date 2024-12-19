use std::any::Any;
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::DerefMut;

use generational_box::GenerationalBox;
use slab::Slab;

use crate::composer::{Node, NodeKey};
use crate::{offset_to_anchor, ComposeNode, Composer, Loc, State, StateId};

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
        let id = ScopeId::new();
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let mut c = self.composer.write();
        let current_node_key = c.current_node_key;
        let id = StateId::new(current_node_key);
        let scope_states = c.states.entry(current_node_key).or_default();
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
        child_scope: Scope<T, N>,
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
        let parent_scope = *self;
        let composable = move || {
            let mut current_scope = child_scope;
            let (current_node_key, is_dirty) = {
                let mut c = parent_scope.composer.write();
                let c = c.deref_mut();
                if let Some(key) = c.key_stack.last().copied() {
                    current_scope.set_key(key);
                }
                let current_scope_id = current_scope.id;
                c.start_scope(current_scope_id);
                let current_node_key = c.current_node_key;
                let is_visited = c.composables.contains_key(&current_node_key);
                let is_dirty = c.dirty_nodes.contains(&current_node_key);
                if !is_dirty && is_visited {
                    c.skip_scope();
                    return current_node_key;
                }
                update_node(
                    current_node_key,
                    &mut c.context,
                    &mut c.nodes,
                    &input,
                    &factory,
                    &update,
                );
                (current_node_key, is_dirty)
            };
            content(current_scope);
            let mut c = parent_scope.composer.write();
            let c = c.deref_mut();
            if is_dirty {
                c.dirty_nodes.remove(&current_node_key);
            }
            c.end_scope(current_node_key);
            current_node_key
        };
        let current_node_key = composable();
        let mut c = parent_scope.composer.write();
        c.composables
            .entry(current_node_key)
            .or_insert_with(|| Box::new(composable));
    }

    #[inline(always)]
    pub fn create_any_node<C, T, I, A, E, F, U>(
        &self,
        child_scope: Scope<T, N>,
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
            child_scope,
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
    nodes: &mut Slab<Node<N>>,
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

// #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct CallId(u64);

// impl CallId {
//     #[track_caller]
//     #[inline(always)]
//     pub fn new() -> Self {
//         let offset = (offset_to_anchor() as u64) << 32;
//         Self(offset)
//     }

//     #[inline(always)]
//     pub fn set_key(&mut self, key: u32) {
//         self.0 = (self.0 | 0xFFFF_FFFF_0000_0000) + key as u64;
//     }

//     #[inline(always)]
//     pub fn get_key(&self) -> u32 {
//         (self.0 & 0x0000_0000_FFFF_FFFF) as u32
//     }
// }

// impl Debug for CallId {
//     fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
//         write!(f, "CallId({})", self.0)
//     }
// }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId {
    pub loc: Loc,
    pub key: u32,
    //pub id: CallId,
}

impl ScopeId {
    #[track_caller]
    #[inline(always)]
    pub fn new() -> Self {
        let loc = Loc::new();
        Self { loc, key: 0 }
        //Self { id: CallId::new() }
    }

    #[inline(always)]
    pub fn set_key(&mut self, key: u32) {
        self.key = key;
        //self.id.set_key(key);
    }

    #[inline(always)]
    pub fn get_key(&self) -> u32 {
        self.key
        //self.id.get_key()
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeId( {:?} , {})", self.loc, self.key)
        //write!(f, "ScopeId({:?})", self.id)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;
