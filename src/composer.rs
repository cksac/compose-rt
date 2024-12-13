use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::BuildHasher;

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Root, Scope, ScopeId, StateId};

pub(crate) trait HashMapExt {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, V, S> HashMapExt for std::collections::HashMap<K, V, S>
where
    S: BuildHasher + Default,
{
    fn new() -> Self {
        std::collections::HashMap::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashMap::with_capacity_and_hasher(capacity, S::default())
    }
}

pub(crate) trait HashSetExt {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, S> HashSetExt for std::collections::HashSet<K, S>
where
    S: BuildHasher + Default,
{
    fn new() -> Self {
        std::collections::HashSet::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashSet::with_capacity_and_hasher(capacity, S::default())
    }
}

pub(crate) type Map<K, V> = FxHashMap<K, V>;
pub(crate) type Set<K> = FxHashSet<K>;

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

#[allow(dead_code)]
#[derive(Debug)]
pub struct Group<N> {
    pub(crate) parent: ScopeId,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) node: Option<N>,
}

pub(crate) struct StateData {
    #[allow(clippy::type_complexity)]
    pub(crate) states: Map<ScopeId, Map<StateId, Box<dyn Any>>>,
    pub(crate) subscribers: Map<StateId, Set<ScopeId>>,
    pub(crate) uses: Map<ScopeId, Set<StateId>>,
    pub(crate) dirty_states: Set<StateId>,
}

pub struct Composer<N> {
    pub(crate) is_initialized: bool,
    pub(crate) composables: Map<ScopeId, Box<dyn Composable>>,
    pub(crate) groups: Map<ScopeId, Group<N>>,
    pub(crate) state_data: StateData,
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
            state_data: StateData {
                states: Map::new(),
                subscribers: Map::new(),
                uses: Map::new(),
                dirty_states: Set::new(),
            },
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
            state_data: StateData {
                states: Map::new(),
                subscribers: Map::new(),
                uses: Map::new(),
                dirty_states: Set::new(),
            },
            current_scope: ScopeId::new(),
            key_stack: Vec::new(),
            dirty_scopes: Set::new(),
            child_count_stack: Vec::new(),
            mount_scopes: Set::new(),
            unmount_scopes: Set::new(),
        }
    }

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
        Recomposer {
            owner,
            composer,
            composables: c
                .composables
                .iter()
                .map(|(k, v)| (*k, v.clone_box()))
                .collect(),
        }
    }

    // #[allow(dead_code)]
    // pub(crate) fn create_scope<C, P, S>(&self, parent: Scope<P, N>, scope: Scope<S, N>, content: C)
    // where
    //     P: 'static,
    //     S: 'static,
    //     C: Fn(Scope<S, N>) + 'static,
    // {
    //     let composable = move || {
    //         let parent = parent;
    //         let mut scope = scope;
    //         let mut c = parent.composer.write();
    //         if let Some(key) = c.key_stack.borrow().last().cloned() {
    //             scope.set_key(key);
    //         }
    //         let is_visited = c.is_visited(scope.id);
    //         let is_dirty = c.is_dirty(scope.id);
    //         if !is_dirty && is_visited {
    //             c.skip_group();
    //             return;
    //         }
    //         let parent_child_idx = c.start_group(scope.id);
    //         {
    //             let mut groups = c.groups.borrow_mut();
    //             groups.entry(scope.id).or_insert_with(|| Group {
    //                 node: None,
    //                 parent: parent.id,
    //                 children: Vec::new(),
    //             });
    //             if c.is_initialized.get() {
    //                 if let Some(curr_child_idx) = parent_child_idx {
    //                     let parent_grp = groups.get_mut(&parent.id).unwrap();
    //                     if let Some(existing_child) =
    //                         parent_grp.children.get(curr_child_idx).cloned()
    //                     {
    //                         if existing_child != scope.id {
    //                             //println!("replace grp {:?} by {:?}", existing_child, scope.id);
    //                             parent_grp.children[curr_child_idx] = scope.id;
    //                             c.unmount_scopes.borrow_mut().insert(existing_child);
    //                         }
    //                     } else {
    //                         //println!("new grp {:?}", scope.id);
    //                         c.mount_scopes.borrow_mut().insert(scope.id);
    //                         parent_grp.children.push(scope.id);
    //                     }
    //                 }
    //             } else if let Some(parent_grp) = groups.get_mut(&parent.id) {
    //                 parent_grp.children.push(scope.id);
    //             }
    //         }
    //         drop(c);
    //         content(scope);
    //         let mut c = parent.composer.write();
    //         if is_dirty {
    //             c.clear_dirty(scope.id);
    //         }
    //         c.end_group(parent.id, scope.id);
    //     };
    //     composable();
    //     let mut new_composables = self.composables.borrow_mut();
    //     new_composables
    //         .entry(scope.id)
    //         .or_insert_with(|| Box::new(composable));
    // }

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
    composables: Map<ScopeId, Box<dyn Composable>>,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N>
where
    N: Debug + 'static,
{
    pub fn recompose(&mut self) {
        let mut c = self.composer.write();
        let mut affected_scopes = Set::with_capacity(c.state_data.dirty_states.len());
        for state_id in c.state_data.dirty_states.drain().collect::<Vec<_>>() {
            if let Some(scopes) = c.state_data.subscribers.get(&state_id) {
                affected_scopes.extend(scopes.iter().cloned());
            }
        }
        c.dirty_scopes.clear();
        c.dirty_scopes.extend(affected_scopes.iter().cloned());
        drop(c);
        for scope in affected_scopes {
            if let Some(composable) = self.composables.get(&scope) {
                composable.compose();
            }
        }
        let mut c = self.composer.write();
        let diff = c
            .unmount_scopes
            .difference(&c.mount_scopes)
            .cloned()
            .collect::<Vec<_>>();
        for s in &diff {
            self.composables.remove(s);
            c.composables.remove(s);
            c.groups.remove(s);
            if let Some(scope_states) = c.state_data.states.remove(s) {
                for state in scope_states.keys() {
                    c.state_data.subscribers.remove(state);
                }
            }
            let use_states = c.state_data.uses.remove(s);
            if let Some(use_states) = use_states {
                for state in use_states {
                    if let Some(subscribers) = c.state_data.subscribers.get_mut(&state) {
                        subscribers.remove(s);
                    }
                }
            }
        }
        for s in c.mount_scopes.drain().collect::<Vec<_>>() {
            if let Some(composable) = c.composables.get(&s) {
                self.composables
                    .entry(s)
                    .or_insert_with(|| composable.clone_box());
            }
        }
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
            .field("states", &self.state_data.states)
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
            .field("states", &c.state_data.states)
            .finish()
    }
}
