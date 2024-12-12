use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry::{Occupied, Vacant};
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

#[allow(dead_code)]
#[derive(Debug)]
pub struct Group<N> {
    parent: ScopeId,
    children: Vec<ScopeId>,
    node: Option<N>,
}

pub(crate) struct StateData {
    #[allow(clippy::type_complexity)]
    pub(crate) states: Map<ScopeId, Map<StateId, Box<dyn Any>>>,
    pub(crate) subscribers: Map<StateId, Set<ScopeId>>,
    pub(crate) uses: Map<ScopeId, Set<StateId>>,
    pub(crate) dirty_states: Set<StateId>,
}

pub struct Composer<N> {
    is_initialized: Cell<bool>,
    pub(crate) composables: RefCell<Map<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RefCell<Map<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RefCell<Map<ScopeId, Group<N>>>,
    pub(crate) state_data: RefCell<StateData>,
    pub(crate) key_stack: RefCell<Vec<u32>>,
    current_scope: Cell<ScopeId>,
    dirty_scopes: RefCell<Set<ScopeId>>,
    child_count_stack: RefCell<Vec<usize>>,
    mount_scopes: RefCell<Set<ScopeId>>,
    unmount_scopes: RefCell<Set<ScopeId>>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            is_initialized: Cell::new(false),
            composables: RefCell::new(Map::new()),
            new_composables: RefCell::new(Map::new()),
            groups: RefCell::new(Map::new()),
            state_data: RefCell::new(StateData {
                states: Map::new(),
                subscribers: Map::new(),
                uses: Map::new(),
                dirty_states: Set::new(),
            }),
            current_scope: Cell::new(ScopeId::new()),
            key_stack: RefCell::new(Vec::new()),
            dirty_scopes: RefCell::new(Set::new()),
            child_count_stack: RefCell::new(Vec::new()),
            mount_scopes: RefCell::new(Set::new()),
            unmount_scopes: RefCell::new(Set::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            is_initialized: Cell::new(false),
            composables: RefCell::new(Map::with_capacity(capacity)),
            new_composables: RefCell::new(Map::with_capacity(capacity)),
            groups: RefCell::new(Map::with_capacity(capacity)),
            state_data: RefCell::new(StateData {
                states: Map::new(),
                subscribers: Map::new(),
                uses: Map::new(),
                dirty_states: Set::new(),
            }),
            current_scope: Cell::new(ScopeId::new()),
            key_stack: RefCell::new(Vec::new()),
            dirty_scopes: RefCell::new(Set::new()),
            child_count_stack: RefCell::new(Vec::new()),
            mount_scopes: RefCell::new(Set::new()),
            unmount_scopes: RefCell::new(Set::new()),
        }
    }

    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(1024));
        let c = composer.read();
        let id = ScopeId::new();
        let scope = Scope::new(id, composer);
        c.start_root(scope.id);
        root(scope);
        c.end_root(scope.id);
        let mut new_composables = c.new_composables.borrow_mut();
        let mut composables = c.composables.borrow_mut();
        composables.extend(new_composables.drain());
        c.is_initialized.set(true);
        Recomposer { owner, composer }
    }

    pub(crate) fn recompose(&self) {
        let affected_scopes = {
            let mut state_data = self.state_data.borrow_mut();
            let mut affected_scopes = Set::with_capacity(state_data.dirty_states.len());
            for state_id in state_data.dirty_states.drain().collect::<Vec<_>>() {
                if let Some(scopes) = state_data.subscribers.get(&state_id) {
                    affected_scopes.extend(scopes.iter().cloned());
                }
            }
            affected_scopes
        };
        {
            let mut dirty_scopes = self.dirty_scopes.borrow_mut();
            dirty_scopes.clear();
            dirty_scopes.extend(affected_scopes.iter().cloned());
        }
        {
            let composables = self.composables.borrow();
            for scope in affected_scopes {
                if let Some(composable) = composables.get(&scope) {
                    composable();
                }
            }
        }
        let mut composables = self.composables.borrow_mut();
        let mut groups = self.groups.borrow_mut();
        let mut state_data = self.state_data.borrow_mut();
        let mut unmount_scopes = self.unmount_scopes.borrow_mut();
        let mut mount_scopes = self.mount_scopes.borrow_mut();
        for s in unmount_scopes.difference(&mount_scopes) {
            composables.remove(s);
            groups.remove(s);
            if let Some(scope_states) = state_data.states.remove(s) {
                for state in scope_states.keys() {
                    state_data.subscribers.remove(state);
                }
            }
            let use_states = state_data.uses.remove(s);
            if let Some(use_states) = use_states {
                for state in use_states {
                    if let Some(subscribers) = state_data.subscribers.get_mut(&state) {
                        subscribers.remove(s);
                    }
                }
            }
        }
        unmount_scopes.clear();
        mount_scopes.clear();
        let mut new_composables = self.new_composables.borrow_mut();
        composables.extend(new_composables.drain());
    }

    pub(crate) fn create_node_scope<C, P, S, I, A, F, U>(
        &self,
        parent: Scope<P, N>,
        scope: Scope<S, N>,
        content: C,
        input: I,
        factory: F,
        update: U,
    ) where
        P: 'static,
        S: 'static,
        C: Fn(Scope<S, N>) + 'static,
        I: Fn() -> A + 'static,
        A: 'static,
        F: Fn(A) -> N + 'static,
        U: Fn(&mut N, A) + 'static,
    {
        let composable = move || {
            let parent = parent;
            let mut scope = scope;
            let c = parent.composer.read();
            if let Some(key) = c.key_stack.borrow().last().cloned() {
                scope.set_key(key);
            }
            let is_visited = c.is_visited(scope.id);
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && is_visited {
                c.skip_group();
                return;
            }
            let parent_child_idx = c.start_group(scope.id);
            {
                let mut groups = c.groups.borrow_mut();
                let input = input();
                match groups.entry(scope.id) {
                    Occupied(mut entry) => {
                        let group = entry.get_mut();
                        if let Some(node) = group.node.as_mut() {
                            update(node, input);
                        } else {
                            let node = factory(input);
                            group.node = Some(node);
                        }
                    }
                    Vacant(entry) => {
                        let node = factory(input);
                        entry.insert(Group {
                            node: Some(node),
                            parent: parent.id,
                            children: Vec::new(),
                        });
                    }
                }
                if c.is_initialized.get() {
                    if let Some(curr_child_idx) = parent_child_idx {
                        let parent_grp = groups.get_mut(&parent.id).unwrap();
                        if let Some(existing_child) =
                            parent_grp.children.get(curr_child_idx).cloned()
                        {
                            if existing_child != scope.id {
                                //println!("replace grp {:?} by {:?}", existing_child, scope.id);
                                parent_grp.children[curr_child_idx] = scope.id;
                                c.unmount_scopes.borrow_mut().insert(existing_child);
                            }
                        } else {
                            //println!("new grp {:?}", scope.id);
                            c.mount_scopes.borrow_mut().insert(scope.id);
                            parent_grp.children.push(scope.id);
                        }
                    }
                } else if let Some(parent_grp) = groups.get_mut(&parent.id) {
                    parent_grp.children.push(scope.id);
                }
            }
            content(scope);
            if is_dirty {
                c.clear_dirty(scope.id);
            }
            c.end_group(parent.id, scope.id);
        };
        composable();
        if !self.is_registered(scope.id) {
            let mut new_composables = self.new_composables.borrow_mut();
            new_composables
                .entry(scope.id)
                .or_insert_with(|| Box::new(composable));
        }
    }

    #[allow(dead_code)]
    pub(crate) fn create_scope<C, P, S>(&self, parent: Scope<P, N>, scope: Scope<S, N>, content: C)
    where
        P: 'static,
        S: 'static,
        C: Fn(Scope<S, N>) + 'static,
    {
        let composable = move || {
            let parent = parent;
            let mut scope = scope;
            let c = parent.composer.read();
            if let Some(key) = c.key_stack.borrow().last().cloned() {
                scope.set_key(key);
            }
            let is_visited = c.is_visited(scope.id);
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && is_visited {
                c.skip_group();
                return;
            }
            let parent_child_idx = c.start_group(scope.id);
            {
                let mut groups = c.groups.borrow_mut();
                groups.entry(scope.id).or_insert_with(|| Group {
                    node: None,
                    parent: parent.id,
                    children: Vec::new(),
                });
                if c.is_initialized.get() {
                    if let Some(curr_child_idx) = parent_child_idx {
                        let parent_grp = groups.get_mut(&parent.id).unwrap();
                        if let Some(existing_child) =
                            parent_grp.children.get(curr_child_idx).cloned()
                        {
                            if existing_child != scope.id {
                                //println!("replace grp {:?} by {:?}", existing_child, scope.id);
                                parent_grp.children[curr_child_idx] = scope.id;
                                c.unmount_scopes.borrow_mut().insert(existing_child);
                            }
                        } else {
                            //println!("new grp {:?}", scope.id);
                            c.mount_scopes.borrow_mut().insert(scope.id);
                            parent_grp.children.push(scope.id);
                        }
                    }
                } else if let Some(parent_grp) = groups.get_mut(&parent.id) {
                    parent_grp.children.push(scope.id);
                }
            }
            content(scope);
            if is_dirty {
                c.clear_dirty(scope.id);
            }
            c.end_group(parent.id, scope.id);
        };
        composable();
        if !self.is_registered(scope.id) {
            let mut new_composables = self.new_composables.borrow_mut();
            new_composables
                .entry(scope.id)
                .or_insert_with(|| Box::new(composable));
        }
    }

    #[inline(always)]
    fn start_root(&self, scope: ScopeId) {
        let parent = ScopeId::new();
        self.set_current_scope(scope);
        self.child_count_stack.borrow_mut().push(0);
        self.groups.borrow_mut().insert(
            scope,
            Group {
                node: None,
                parent,
                children: Vec::new(),
            },
        );
    }

    #[inline(always)]
    fn end_root(&self, scope: ScopeId) {
        let mut child_count_stack = self.child_count_stack.borrow_mut();
        let child_count = child_count_stack.pop().unwrap();
        let mut groups = self.groups.borrow_mut();
        let old_child_count = groups[&scope].children.len();
        if child_count < old_child_count {
            groups
                .get_mut(&scope)
                .unwrap()
                .children
                .truncate(child_count);
        }
    }

    #[inline(always)]
    fn start_group(&self, scope: ScopeId) -> Option<usize> {
        let parent_child_idx = self.child_count_stack.borrow().last().cloned();
        self.set_current_scope(scope);
        self.child_count_stack.borrow_mut().push(0);
        parent_child_idx
    }

    #[inline(always)]
    fn end_group(&self, parent: ScopeId, scope: ScopeId) {
        let mut child_count_stack = self.child_count_stack.borrow_mut();
        let child_count = child_count_stack.pop().unwrap();
        let mut groups = self.groups.borrow_mut();
        let old_child_count = groups[&scope].children.len();
        if child_count < old_child_count {
            let removed = groups
                .get_mut(&scope)
                .unwrap()
                .children
                .drain(child_count..)
                .collect::<Vec<_>>();
            for child in removed {
                groups.remove(&child);
            }
        }
        if let Some(parent_child_count) = child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
        self.set_current_scope(parent);
    }

    #[inline(always)]
    fn skip_group(&self) {
        let mut child_count_stack = self.child_count_stack.borrow_mut();
        if let Some(parent_child_count) = child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
    }

    #[inline(always)]
    pub(crate) fn get_current_scope(&self) -> ScopeId {
        self.current_scope.get()
    }

    #[inline(always)]
    fn set_current_scope(&self, scope: ScopeId) {
        self.current_scope.set(scope);
    }

    #[inline(always)]
    fn is_registered(&self, scope: ScopeId) -> bool {
        let composables = self.composables.borrow();
        composables.contains_key(&scope)
    }

    #[inline(always)]
    fn is_visited(&self, scope: ScopeId) -> bool {
        let groups = self.groups.borrow();
        groups.contains_key(&scope)
    }

    #[inline(always)]
    fn is_dirty(&self, scope: ScopeId) -> bool {
        let dirty_scopes = self.dirty_scopes.borrow();
        dirty_scopes.contains(&scope)
    }

    #[inline(always)]
    fn clear_dirty(&self, scope: ScopeId) {
        let mut dirty_scopes = self.dirty_scopes.borrow_mut();
        dirty_scopes.remove(&scope);
    }
}

pub struct Recomposer<N> {
    #[allow(dead_code)]
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N> {
    #[inline(always)]
    pub fn recompose(&self)
    where
        N: Debug + 'static,
    {
        let c = self.composer.read();
        c.recompose();
    }
}

impl<N> Debug for Composer<N>
where
    N: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("groups", &self.groups)
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
            .finish()
    }
}
