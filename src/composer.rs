use std::any::Any;
use std::fmt::{Debug, Formatter};

use generational_box::{AnyStorage, UnsyncStorage};
use slab::Slab;

use crate::map::{HashMapExt, HashSetExt, Map, Set};
use crate::scope::CallId;
use crate::{Recomposer, Root, Scope, ScopeId, State, StateId};

pub trait Composable {
    fn compose(&self);
    fn clone_box(&self) -> Box<dyn Composable>;
}

impl<T> Composable for T
where
    T: Fn() + Clone + 'static,
{
    fn compose(&self) {
        self();
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

#[derive(Debug)]
pub struct Node<T> {
    pub scope: ScopeId,
    pub parent: NodeKey,
    pub children: Vec<NodeKey>,
    pub data: Option<T>,
}

pub trait ComposeNode: 'static {
    type Context;
}

pub type NodeKey = usize;

pub struct Composer<N>
where
    N: ComposeNode,
{
    pub context: N::Context,
    pub nodes: Slab<Node<N>>,
    pub scopes: Map<ScopeId, NodeKey>,
    pub(crate) initialized: bool,
    pub(crate) root_node_key: NodeKey,
    pub(crate) composables: Map<ScopeId, Box<dyn Composable>>,
    pub(crate) states: Map<ScopeId, Map<StateId, Box<dyn Any>>>,
    pub(crate) used_by: Map<StateId, Set<ScopeId>>,
    pub(crate) uses: Map<ScopeId, Set<StateId>>,
    pub(crate) current_node_key: NodeKey,
    pub(crate) key_stack: Vec<u32>,
    pub(crate) child_count_stack: Vec<usize>,
    pub(crate) dirty_states: Set<StateId>,
    pub(crate) dirty_scopes: Set<ScopeId>,
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
            scopes: Map::new(),
            initialized: false,
            root_node_key: 0,
            composables: Map::new(),
            states: Map::new(),
            used_by: Map::new(),
            uses: Map::new(),
            current_node_key: 0,
            key_stack: Vec::new(),
            child_count_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_scopes: Set::new(),
            mount_nodes: Set::new(),
            unmount_nodes: Set::new(),
        }
    }

    pub fn with_capacity(context: N::Context, capacity: usize) -> Self {
        Self {
            context,
            nodes: Slab::with_capacity(capacity),
            scopes: Map::with_capacity(capacity),
            initialized: false,
            root_node_key: 0,
            composables: Map::with_capacity(capacity),
            states: Map::with_capacity(capacity),
            used_by: Map::with_capacity(capacity),
            uses: Map::with_capacity(capacity),
            current_node_key: 0,
            child_count_stack: Vec::new(),
            key_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_scopes: Set::new(),
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
        let id = ScopeId::new(CallId::new());
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        let root_state = scope.use_state(|| {});
        root(scope);
        composer.write().end_root(scope.id);
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
        let id = ScopeId::new(CallId::new());
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        let root_state = scope.use_state(state_fn);
        root(scope, root_state);
        composer.write().end_root(scope.id);
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
        self.child_count_stack.push(0);
        let node_key = self.nodes.insert(Node {
            scope: scope_id,
            data: None,
            parent: parent_node_key,
            children: Vec::new(),
        });
        self.scopes.insert(scope_id, node_key);
        self.current_node_key = node_key;
    }

    #[inline(always)]
    pub(crate) fn end_root(&mut self, scope_id: ScopeId) {
        let child_count = self.child_count_stack.pop().unwrap();
        assert_eq!(1, child_count, "Root scope must have exactly one child");
        let node_key = self.scopes[&scope_id];
        self.root_node_key = self.nodes[node_key].children[0];
    }

    #[inline(always)]
    pub(crate) fn start_scope(&mut self, parent_node_key: NodeKey, current_scope_id: ScopeId) {
        let current_node_key = self.scopes.entry(current_scope_id).or_insert_with(|| {
            self.nodes.insert(Node {
                scope: current_scope_id,
                data: None,
                parent: parent_node_key,
                children: Vec::new(),
            })
        });
        self.current_node_key = *current_node_key;
        self.child_count_stack.push(0);
    }

    #[inline(always)]
    pub(crate) fn end_scope(&mut self, parent_node_key: NodeKey, current_node_key: NodeKey) {
        let child_count = self.child_count_stack.pop().unwrap();
        let old_child_count = self.nodes[current_node_key].children.len();
        if child_count < old_child_count {
            let unmount_nodes = self
                .nodes
                .get_mut(current_node_key)
                .unwrap()
                .children
                .drain(child_count..);
            self.unmount_nodes.extend(unmount_nodes);
        }
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.current_node_key = parent_node_key;
    }

    #[inline(always)]
    pub(crate) fn skip_scope(&mut self) {
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
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
            .finish()
    }
}
