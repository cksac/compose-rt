use std::any::Any;
use std::fmt::{Debug, Formatter};

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
    pub(crate) initialized: bool,
    pub(crate) root_node_key: NodeKey,
    pub(crate) composables: Map<NodeKey, Box<dyn Composable>>,
    pub(crate) states: Map<NodeKey, Map<StateId, Box<dyn Any>>>,
    pub(crate) used_by: Map<StateId, Set<NodeKey>>,
    pub(crate) uses: Map<NodeKey, Set<StateId>>,
    pub(crate) current_node_key: NodeKey,
    pub(crate) key_stack: Vec<u32>,
    pub(crate) node_stack: Vec<(NodeKey, usize)>,
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
            node_stack: Vec::new(),
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
            node_stack: Vec::new(),
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
        let id = ScopeId::new();
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
        let node_key = self.nodes.insert(Node {
            scope: scope_id,
            data: None,
            parent: parent_node_key,
            children: Vec::new(),
        });
        self.node_stack.push((node_key, 0));
        self.current_node_key = node_key;
    }

    #[inline(always)]
    pub(crate) fn end_root(&mut self, scope_id: ScopeId) {
        let (node_key, child_count) = self.node_stack.pop().unwrap();
        assert_eq!(1, child_count, "Root scope must have exactly one child");
        self.root_node_key = self.nodes[node_key].children[0];
    }

    #[inline(always)]
    pub(crate) fn start_scope(&mut self, current_scope_id: ScopeId) {
        let parent = self.node_stack.last().cloned();
        if self.initialized {
            if let Some((parent_node_key, child_idx)) = parent {
                let parent_node = &mut self.nodes[parent_node_key];
                if child_idx < parent_node.children.len() {
                    let child_key = parent_node.children[child_idx];
                    let child_node = &mut self.nodes[child_key];
                    if child_node.scope == current_scope_id {
                        self.current_node_key = child_key;
                        self.mount_nodes.insert(child_key);
                        self.node_stack.push((self.current_node_key, 0));
                    } else {
                        let current_node_key = self.nodes.insert(Node {
                            scope: current_scope_id,
                            data: None,
                            parent: parent_node_key,
                            children: Vec::new(),
                        });
                        self.nodes[parent_node_key].children[child_idx] = current_node_key;
                        self.unmount_nodes.insert(child_key);
                        self.mount_nodes.insert(current_node_key);
                        self.current_node_key = current_node_key;
                        self.node_stack.push((self.current_node_key, 0));
                    }
                } else {
                    let current_node_key = self.nodes.insert(Node {
                        scope: current_scope_id,
                        data: None,
                        parent: parent_node_key,
                        children: Vec::new(),
                    });
                    self.nodes[parent_node_key].children.push(current_node_key);
                    self.mount_nodes.insert(current_node_key);
                    self.current_node_key = current_node_key;
                    self.node_stack.push((self.current_node_key, 0));
                }
            } else {
                // recompose root
                self.node_stack.push((self.current_node_key, 0));
            }
        } else {
            // first compose
            let (parent_node_key, _) = parent.unwrap();
            let current_node_key = self.nodes.insert(Node {
                scope: current_scope_id,
                data: None,
                parent: parent_node_key,
                children: Vec::new(),
            });
            self.nodes[parent_node_key].children.push(current_node_key);
            self.current_node_key = current_node_key;
            self.node_stack.push((current_node_key, 0));
        }
    }

    #[inline(always)]
    pub(crate) fn end_scope(&mut self, current_node_key: NodeKey) {
        let (_, child_count) = self.node_stack.pop().unwrap();
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
        if let Some((parent_node_key, parent_child_count)) = self.node_stack.last_mut() {
            *parent_child_count += 1;
            self.current_node_key = *parent_node_key;
        }
    }

    #[inline(always)]
    pub(crate) fn skip_scope(&mut self) {
        let (_, child_count) = self.node_stack.pop().unwrap();

        if let Some((_, parent_child_count)) = self.node_stack.last_mut() {
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
