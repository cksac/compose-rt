use std::any::Any;
use std::fmt::{Debug, Formatter};
use std::ops::RangeBounds;
use std::vec::Drain;

use generational_box::{AnyStorage, UnsyncStorage};
use slab::Slab;

use crate::map::{HashMapExt, HashSetExt, Map, Set};
use crate::{Recomposer, Root, Scope, ScopeId, State, StateId};

pub trait Composable {
    fn compose(&self) -> NodeKey;
    fn clone_box(&self) -> Box<dyn Composable>;
}

impl<T> Composable for T
where
    T: Fn() -> NodeKey + Clone + 'static,
{
    fn compose(&self) -> NodeKey {
        self()
    }

    fn clone_box(&self) -> Box<dyn Composable> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Composable> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub trait ComposeNode: 'static {
    type Context;
    type Data;

    fn new(scope_id: ScopeId, parent: NodeKey) -> Self;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn scope_id(&self) -> ScopeId;
    fn set_scope_id(&mut self, scope_id: ScopeId);

    fn parent(&self) -> NodeKey;
    fn set_parent(&mut self, parent: NodeKey);

    fn data(&self) -> Option<&Self::Data>;
    fn data_mut(&mut self) -> Option<&mut Self::Data>;
    fn set_data(&mut self, data: Self::Data);

    fn children(&self) -> &[NodeKey];
    fn children_mut(&mut self) -> &mut [NodeKey];
    fn children_push(&mut self, node_key: NodeKey);
    fn children_len(&self) -> usize;
    fn children_drain<R>(&mut self, range: R) -> Drain<NodeKey>
    where
        R: RangeBounds<usize>;
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
    #[inline(always)]
    fn new(val: T) -> Self {
        Box::new(val)
    }

    #[inline(always)]
    fn value(&self) -> &T {
        self.downcast_ref::<T>().unwrap()
    }

    #[inline(always)]
    fn value_mut(&mut self) -> &mut T {
        self.downcast_mut::<T>().unwrap()
    }
}

pub type NodeKey = usize;

pub struct Composer<N>
where
    N: ComposeNode,
{
    pub context: N::Context,
    pub nodes: Slab<N>,
    pub(crate) initialized: bool,
    pub(crate) root_node_key: NodeKey,
    pub(crate) composables: Map<NodeKey, Box<dyn Composable>>,
    pub(crate) states: Map<NodeKey, Map<StateId, Box<dyn Any>>>,
    pub(crate) used_by: Map<StateId, Set<NodeKey>>,
    pub(crate) uses: Map<NodeKey, Set<StateId>>,
    pub(crate) current_node_key: NodeKey,
    pub(crate) key_stack: Vec<usize>,
    pub(crate) child_idx_stack: Vec<usize>,
    pub(crate) dirty_states: Set<StateId>,
    pub(crate) dirty_nodes: Set<NodeKey>,
    pub(crate) mount_nodes: Set<NodeKey>,
    pub(crate) unmount_nodes: Set<NodeKey>,
}

impl<N> Composer<N>
where
    N: ComposeNode,
{
    pub fn new(context: N::Context) -> Self {
        Self {
            context,
            nodes: Slab::new(),
            initialized: false,
            root_node_key: 0,
            composables: Map::new(),
            states: Map::new(),
            used_by: Map::new(),
            uses: Map::new(),
            current_node_key: 0,
            key_stack: Vec::new(),
            child_idx_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_nodes: Set::new(),
            mount_nodes: Set::new(),
            unmount_nodes: Set::new(),
        }
    }

    pub fn with_capacity(context: N::Context, capacity: usize) -> Self {
        Self {
            context,
            nodes: Slab::with_capacity(capacity),
            initialized: false,
            root_node_key: 0,
            composables: Map::with_capacity(capacity),
            states: Map::with_capacity(capacity),
            used_by: Map::with_capacity(capacity),
            uses: Map::with_capacity(capacity),
            current_node_key: 0,
            child_idx_stack: Vec::new(),
            key_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_nodes: Set::new(),
            mount_nodes: Set::with_capacity(capacity),
            unmount_nodes: Set::new(),
        }
    }

    // TODO: fine control over the capacity of the HashMaps
    #[track_caller]
    pub fn compose<R>(root: R, context: N::Context) -> Recomposer<(), N>
    where
        R: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(context, 1024));
        let id = ScopeId::new();
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        let root_state = scope.use_state(|| {});
        root(scope);
        composer.write().end_root();
        let mut c = composer.write();
        c.initialized = true;
        Recomposer {
            owner,
            composer,
            root_state,
        }
    }

    #[track_caller]
    pub fn compose_with<R, F, T>(root: R, context: N::Context, state_fn: F) -> Recomposer<T, N>
    where
        R: Fn(Scope<Root, N>, State<T, N>),
        F: Fn() -> T + 'static,
        T: 'static,
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(context, 1024));
        let id = ScopeId::new();
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        let root_state = scope.use_state(state_fn);
        root(scope, root_state);
        composer.write().end_root();
        let mut c = composer.write();
        c.initialized = true;
        Recomposer {
            owner,
            composer,
            root_state,
        }
    }

    #[inline(always)]
    pub(crate) fn start_root(&mut self, scope_id: ScopeId) {
        let parent_node_key = 0;
        let node_key = self.nodes.insert(N::new(scope_id, parent_node_key));
        self.child_idx_stack.push(0);
        self.current_node_key = node_key;
    }

    #[inline(always)]
    pub(crate) fn end_root(&mut self) {
        let child_count = self.child_idx_stack.pop().unwrap();
        assert_eq!(1, child_count, "Root scope must have exactly one child");
        self.root_node_key = self.nodes[self.current_node_key].children()[0];
    }

    #[inline(always)]
    pub(crate) fn start_node(&mut self, parent_node_key: NodeKey, scope_id: ScopeId) {
        if self.initialized {
            let child_idx = self.child_idx_stack.last().cloned();
            if let Some(child_idx) = child_idx {
                let parent_node = &mut self.nodes[parent_node_key];
                if child_idx < parent_node.children_len() {
                    let child_key = parent_node.children()[child_idx];
                    let child_node = &mut self.nodes[child_key];
                    if child_node.scope_id() == scope_id {
                        // reuse existing node
                        self.current_node_key = child_key;
                        self.mount_nodes.insert(child_key);
                        self.child_idx_stack.push(0);
                    } else {
                        // replace existing node
                        let node_key = self.nodes.insert(N::new(scope_id, parent_node_key));
                        self.nodes[parent_node_key].children_mut()[child_idx] = node_key;
                        self.unmount_nodes.insert(child_key);
                        self.mount_nodes.insert(node_key);
                        self.current_node_key = node_key;
                        self.child_idx_stack.push(0);
                    }
                } else {
                    // append new node
                    let node_key = self.nodes.insert(N::new(scope_id, parent_node_key));
                    self.nodes[parent_node_key].children_push(node_key);
                    self.mount_nodes.insert(node_key);
                    self.current_node_key = node_key;
                    self.child_idx_stack.push(0);
                }
            } else {
                // recompose root
                self.child_idx_stack.push(0);
            }
        } else {
            // first compose
            let node_key = self.nodes.insert(N::new(scope_id, parent_node_key));
            self.nodes[parent_node_key].children_push(node_key);
            self.current_node_key = node_key;
            self.child_idx_stack.push(0);
        }
    }

    #[inline(always)]
    pub(crate) fn end_node(&mut self, parent_node_key: NodeKey) {
        let child_count = self.child_idx_stack.pop().unwrap();
        let node = &mut self.nodes[self.current_node_key];
        let old_child_count = node.children_len();
        if child_count < old_child_count {
            let unmount_nodes = node.children_drain(child_count..);
            self.unmount_nodes.extend(unmount_nodes);
        }
        if let Some(parent_child_count) = self.child_idx_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.current_node_key = parent_node_key;
    }

    #[inline(always)]
    pub(crate) fn skip_node(&mut self, parent_node_key: NodeKey) {
        let _ = self.child_idx_stack.pop().unwrap();
        if let Some(parent_child_count) = self.child_idx_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.current_node_key = parent_node_key;
    }
}

impl<N> Debug for Composer<N>
where
    N: ComposeNode + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("nodes", &self.nodes)
            .field("states", &self.states)
            .field("dirty_states", &self.dirty_states)
            .field("used_by", &self.used_by)
            .finish()
    }
}
