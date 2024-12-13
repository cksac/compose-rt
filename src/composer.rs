use std::any::Any;
use std::fmt::{Debug, Formatter};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};

use crate::map::{HashMapExt, HashSetExt, Map, Set};
use crate::{Root, Scope, ScopeId, StateId};

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

#[allow(dead_code)]
#[derive(Debug)]
pub struct Group<N> {
    pub(crate) parent: ScopeId,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) node: Option<N>,
}

pub struct Composer<N> {
    pub(crate) is_initialized: bool,
    pub(crate) composables: Map<ScopeId, Box<dyn Composable>>,
    pub(crate) groups: Map<ScopeId, Group<N>>,
    pub(crate) states: Map<ScopeId, Map<StateId, Box<dyn Any>>>,
    pub(crate) subscribers: Map<StateId, Set<ScopeId>>,
    pub(crate) uses: Map<ScopeId, Set<StateId>>,
    pub(crate) dirty_states: Set<StateId>,
    pub(crate) key_stack: Vec<u32>,
    pub(crate) current_scope: ScopeId,
    pub(crate) dirty_scopes: Set<ScopeId>,
    pub(crate) child_count_stack: Vec<usize>,
    pub(crate) mount_scopes: Set<ScopeId>,
    pub(crate) unmount_scopes: Set<ScopeId>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            is_initialized: false,
            composables: Map::new(),
            groups: Map::new(),
            states: Map::new(),
            subscribers: Map::new(),
            uses: Map::new(),
            dirty_states: Set::new(),
            current_scope: ScopeId::new(),
            key_stack: Vec::new(),
            dirty_scopes: Set::new(),
            child_count_stack: Vec::new(),
            mount_scopes: Set::new(),
            unmount_scopes: Set::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            is_initialized: false,
            composables: Map::with_capacity(capacity),
            groups: Map::with_capacity(capacity),
            states: Map::with_capacity(capacity),
            subscribers: Map::with_capacity(capacity),
            uses: Map::with_capacity(capacity),
            dirty_states: Set::new(),
            current_scope: ScopeId::new(),
            key_stack: Vec::new(),
            dirty_scopes: Set::new(),
            child_count_stack: Vec::new(),
            mount_scopes: Set::with_capacity(capacity),
            unmount_scopes: Set::new(),
        }
    }

    // TODO: fine control over the capacity of the HashMaps
    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(1024));
        let id = ScopeId::new();
        let scope = Scope::new(id, composer);
        composer.write().start_root(scope.id);
        root(scope);
        composer.write().end_root(scope.id);
        let mut c = composer.write();
        c.is_initialized = true;
        Recomposer { owner, composer }
    }

    #[inline(always)]
    pub(crate) fn start_root(&mut self, scope: ScopeId) {
        let parent = ScopeId::new();
        self.current_scope = scope;
        self.child_count_stack.push(0);
        self.groups.insert(
            scope,
            Group {
                node: None,
                parent,
                children: Vec::new(),
            },
        );
    }

    #[inline(always)]
    pub(crate) fn end_root(&mut self, scope: ScopeId) {
        let child_count = self.child_count_stack.pop().unwrap();
        let old_child_count = self.groups[&scope].children.len();
        if child_count < old_child_count {
            self.groups
                .get_mut(&scope)
                .unwrap()
                .children
                .truncate(child_count);
        }
    }

    #[inline(always)]
    pub(crate) fn start_group(&mut self, scope: ScopeId) -> Option<usize> {
        let parent_child_idx = self.child_count_stack.last().cloned();
        self.current_scope = scope;
        self.child_count_stack.push(0);
        parent_child_idx
    }

    #[inline(always)]
    pub(crate) fn end_group(&mut self, parent: ScopeId, scope: ScopeId) {
        let child_count = self.child_count_stack.pop().unwrap();
        let old_child_count = self.groups[&scope].children.len();
        if child_count < old_child_count {
            let removed = self
                .groups
                .get_mut(&scope)
                .unwrap()
                .children
                .drain(child_count..)
                .collect::<Vec<_>>();
            for child in removed {
                self.groups.remove(&child);
            }
        }
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.current_scope = parent;
    }

    #[inline(always)]
    pub(crate) fn skip_group(&mut self) {
        if let Some(parent_child_count) = self.child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
    }
}

pub struct Recomposer<N> {
    #[allow(dead_code)]
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N>
where
    N: Debug + 'static,
{
    pub fn recompose(&mut self) {
        let mut c = self.composer.write();
        c.dirty_scopes.clear();
        for state_id in c.dirty_states.drain().collect::<Vec<_>>() {
            if let Some(scopes) = c.subscribers.get(&state_id).cloned() {
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
        let unmount_scopes = c
            .unmount_scopes
            .difference(&c.mount_scopes)
            .cloned()
            .collect::<Vec<_>>();
        for s in unmount_scopes {
            c.composables.remove(&s);
            c.groups.remove(&s);
            if let Some(scope_states) = c.states.remove(&s) {
                for state in scope_states.keys() {
                    c.subscribers.remove(state);
                }
            }
            let use_states = c.uses.remove(&s);
            if let Some(use_states) = use_states {
                for state in use_states {
                    if let Some(subscribers) = c.subscribers.get_mut(&state) {
                        subscribers.remove(&s);
                    }
                }
            }
        }
        c.mount_scopes.clear();
        c.unmount_scopes.clear();
    }
}

impl<N> Debug for Composer<N>
where
    N: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("groups", &self.groups)
            .field("states", &self.states)
            .finish()
    }
}

impl<N> Debug for Recomposer<N>
where
    N: 'static + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let c = self.composer.read();
        f.debug_struct("Recomposer")
            .field("groups", &c.groups)
            .field("states", &c.states)
            .finish()
    }
}
