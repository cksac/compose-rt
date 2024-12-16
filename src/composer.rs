use std::any::Any;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};
use slotmap::{new_key_type, KeyData, SlotMap};

use crate::map::{HashMapExt, HashSetExt, Map, Set};
use crate::{utils, Root, Scope, ScopeId, StateId};

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

new_key_type! {
    pub struct NodeKey;
}

impl NodeKey {
    pub fn new(val: u64) -> Self {
        NodeKey::from(KeyData::from_ffi(val))
    }
}

impl From<u64> for NodeKey {
    fn from(val: u64) -> Self {
        NodeKey::new(val)
    }
}

impl From<NodeKey> for u64 {
    fn from(key: NodeKey) -> Self {
        key.into()
    }
}

pub struct Composer<N>
where
    N: ComposeNode,
{
    pub context: N::Context,
    pub root_node: NodeKey,
    pub nodes: SlotMap<NodeKey, Node<N>>,
    pub scopes: Map<ScopeId, NodeKey>,
    pub(crate) initialized: bool,
    pub(crate) composables: Map<ScopeId, Box<dyn Composable>>,
    pub(crate) states: Map<ScopeId, Map<StateId, Box<dyn Any>>>,
    pub(crate) used_by: Map<StateId, Set<ScopeId>>,
    pub(crate) uses: Map<ScopeId, Set<StateId>>,
    pub(crate) current_node: NodeKey,
    pub(crate) key_stack: Vec<u32>,
    pub(crate) child_count_stack: Vec<usize>,
    pub(crate) dirty_states: Set<StateId>,
    pub(crate) dirty_scopes: Set<ScopeId>,
    pub(crate) mount_scopes: Set<NodeKey>,
    pub(crate) unmount_scopes: Set<NodeKey>,
}

impl<N> Composer<N>
where
    N: ComposeNode,
{
    pub fn new(context: N::Context) -> Self {
        Self {
            context,
            initialized: false,
            root_node: NodeKey::new(0),
            composables: Map::new(),
            nodes: SlotMap::with_key(),
            scopes: Map::new(),
            states: Map::new(),
            used_by: Map::new(),
            uses: Map::new(),
            current_node: NodeKey::new(0),
            key_stack: Vec::new(),
            child_count_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_scopes: Set::new(),
            mount_scopes: Set::new(),
            unmount_scopes: Set::new(),
        }
    }

    pub fn with_capacity(context: N::Context, capacity: usize) -> Self {
        Self {
            context,
            initialized: false,
            root_node: NodeKey::new(0),
            composables: Map::with_capacity(capacity),
            nodes: SlotMap::with_capacity_and_key(capacity),
            scopes: Map::with_capacity(capacity),
            states: Map::with_capacity(capacity),
            used_by: Map::with_capacity(capacity),
            uses: Map::with_capacity(capacity),
            current_node: NodeKey::new(0),
            child_count_stack: Vec::new(),
            key_stack: Vec::new(),
            dirty_states: Set::new(),
            dirty_scopes: Set::new(),
            mount_scopes: Set::with_capacity(capacity),
            unmount_scopes: Set::new(),
        }
    }

    // TODO: fine control over the capacity of the HashMaps
    #[track_caller]
    pub fn compose<F>(root: F, context: N::Context) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(context, 1024));
        let id = ScopeId::new(0);
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        root(scope);
        composer.write().end_root(scope.id);
        let mut c = composer.write();
        c.initialized = true;
        Recomposer { owner, composer }
    }

    #[inline(always)]
    pub(crate) fn start_root(&mut self, scope: ScopeId) {
        let parent = NodeKey::new(0);
        self.child_count_stack.push(0);
        let node_key = self.nodes.insert(Node {
            scope,
            data: None,
            parent,
            children: Vec::new(),
        });
        self.scopes.insert(scope, node_key);
        self.current_node = node_key;
    }

    #[inline(always)]
    pub(crate) fn end_root(&mut self, scope: ScopeId) {
        let child_count = self.child_count_stack.pop().unwrap();
        assert_eq!(1, child_count, "Root scope must have exactly one child");
        let node_key = self.scopes[&scope];
        self.root_node = self.nodes[node_key].children[0];
    }

    #[inline(always)]
    pub(crate) fn start_scope(&mut self, scope: ScopeId) -> Option<usize> {
        let parent_child_idx = self.child_count_stack.last().cloned();
        let current_node = self.scopes.entry(scope).or_insert_with(|| {
            let parent = self.current_node;
            let node_key = self.nodes.insert(Node {
                scope,
                data: None,
                parent,
                children: Vec::new(),
            });
            node_key
        });
        self.current_node = *current_node;
        self.child_count_stack.push(0);
        parent_child_idx
    }

    #[inline(always)]
    pub(crate) fn end_scope(&mut self, parent: ScopeId, scope: ScopeId) {
        let parent = self.scopes[&parent];
        let scope = self.scopes[&scope];
        let child_count = self.child_count_stack.pop().unwrap();
        let old_child_count = self.nodes[scope].children.len();
        if child_count < old_child_count {
            let unmount_scopes = self
                .nodes
                .get_mut(scope)
                .unwrap()
                .children
                .drain(child_count..);
            self.unmount_scopes.extend(unmount_scopes);
        }
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.current_node = parent;
    }

    #[inline(always)]
    pub(crate) fn skip_scope(&mut self) {
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
    }
}

pub struct Recomposer<N>
where
    N: ComposeNode,
{
    #[allow(dead_code)]
    owner: Owner,
    pub(crate) composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N>
where
    N: ComposeNode,
{
    pub fn recompose(&mut self) {
        let mut c = self.composer.write();
        c.dirty_scopes.clear();
        for state_id in c.dirty_states.drain().collect::<Vec<_>>() {
            if let Some(scopes) = c.used_by.get(&state_id).cloned() {
                c.dirty_scopes.extend(scopes);
            }
        }
        let mut composables = Vec::with_capacity(c.dirty_scopes.len());
        for scope in &c.dirty_scopes {
            if let Some(composable) = c.composables.get(scope).cloned() {
                composables.push(composable);
            }
        }
        drop(c);
        for composable in composables {
            composable.compose();
        }
        let mut c = self.composer.write();
        let c = c.deref_mut();
        let unmount_scopes = c
            .unmount_scopes
            .difference(&c.mount_scopes)
            .cloned()
            .collect::<Vec<_>>();
        for s in unmount_scopes {
            if let Some(s) = c.nodes.remove(s).map(|n| n.scope) {
                c.composables.remove(&s);
                if let Some(scope_states) = c.states.remove(&s) {
                    for state in scope_states.keys() {
                        c.used_by.remove(state);
                    }
                }
                let use_states = c.uses.remove(&s);
                if let Some(use_states) = use_states {
                    for state in use_states {
                        if let Some(used_by) = c.used_by.get_mut(&state) {
                            used_by.remove(&s);
                        }
                    }
                }
            }
        }
        c.mount_scopes.clear();
        c.unmount_scopes.clear();
    }

    pub fn root_node(&self) -> NodeKey {
        self.composer.read().root_node
    }

    pub fn with_context<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&N::Context) -> T,
    {
        let c = self.composer.read();
        func(&c.context)
    }

    pub fn with_context_mut<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut N::Context) -> T,
    {
        let mut c = self.composer.write();
        func(&mut c.context)
    }

    pub fn with_composer<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&Composer<N>) -> T,
    {
        let c = self.composer.read();
        func(c.deref())
    }

    pub fn with_composer_mut<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut Composer<N>) -> T,
    {
        let mut c = self.composer.write();
        func(c.deref_mut())
    }

    #[inline(always)]
    pub fn print_tree(&self)
    where
        N: Debug,
    {
        self.print_tree_with(self.root_node(), |n| format!("{:?}", n));
    }

    pub fn print_tree_with<D>(&self, node: NodeKey, display_fn: D)
    where
        D: Fn(Option<&N>) -> String,
    {
        let c = self.composer.read();
        utils::print_tree(&c, node, display_fn);
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

impl<N> Debug for Recomposer<N>
where
    N: ComposeNode + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let c = self.composer.read();
        f.debug_struct("Recomposer")
            .field("nodes", &c.nodes)
            .field("states", &c.states)
            .finish()
    }
}
