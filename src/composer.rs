use std::{
    any::Any,
    cell::RefCell,
    collections::hash_map::Entry::{Occupied, Vacant},
    fmt::{Debug, Formatter},
    hash::BuildHasher,
};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Root, Scope, ScopeId, StateId};

trait HashMapExt {
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

trait HashSetExt {
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

type Map<K, V> = FxHashMap<K, V>;
type Set<K> = FxHashSet<K>;

#[derive(Debug)]
pub struct Group<N> {
    parent: ScopeId,
    children: Vec<ScopeId>,
    node: Option<N>,
}

pub struct Composer<N> {
    pub(crate) composables: RefCell<Map<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RefCell<Map<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RefCell<Map<ScopeId, Group<N>>>,
    pub(crate) states: RefCell<Map<ScopeId, Map<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RefCell<Map<StateId, Set<ScopeId>>>,
    pub(crate) uses: RefCell<Map<ScopeId, Set<StateId>>>,
    pub(crate) dirty_states: RefCell<Set<StateId>>,
    pub(crate) key_stack: RefCell<Vec<usize>>,
    dirty_scopes: RefCell<Set<ScopeId>>,
    current_scope: RefCell<ScopeId>,
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
            composables: RefCell::new(Map::new()),
            new_composables: RefCell::new(Map::new()),
            groups: RefCell::new(Map::new()),
            current_scope: RefCell::new(ScopeId::new()),
            states: RefCell::new(Map::new()),
            subscribers: RefCell::new(Map::new()),
            uses: RefCell::new(Map::new()),
            dirty_states: RefCell::new(Set::new()),
            key_stack: RefCell::new(Vec::new()),
            dirty_scopes: RefCell::new(Set::new()),
            child_count_stack: RefCell::new(Vec::new()),
            mount_scopes: RefCell::new(Set::new()),
            unmount_scopes: RefCell::new(Set::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            composables: RefCell::new(Map::with_capacity(capacity)),
            new_composables: RefCell::new(Map::with_capacity(capacity)),
            groups: RefCell::new(Map::with_capacity(capacity)),
            current_scope: RefCell::new(ScopeId::new()),
            states: RefCell::new(Map::new()),
            subscribers: RefCell::new(Map::new()),
            uses: RefCell::new(Map::new()),
            dirty_states: RefCell::new(Set::new()),
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
        let id = ScopeId::new();
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::with_capacity(1024));
        let scope = Scope::new(id, composer);
        let c = composer.read();
        c.start_root(scope.id);
        root(scope);
        c.end_root(scope.id);
        let mut new_composables = c.new_composables.borrow_mut();
        let mut composables = c.composables.borrow_mut();
        composables.extend(new_composables.drain());
        Recomposer { owner, composer }
    }

    pub(crate) fn recompose(&self) {
        let affected_scopes = {
            let mut dirty_states = self.dirty_states.borrow_mut();
            let mut affected_scopes = Set::with_capacity(dirty_states.len());
            let subscribers = self.subscribers.borrow_mut();
            for state_id in dirty_states.drain() {
                if let Some(scopes) = subscribers.get(&state_id) {
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
        let mut states = self.states.borrow_mut();
        let mut subs = self.subscribers.borrow_mut();
        let mut uses = self.uses.borrow_mut();
        let mut unmount_scopes = self.unmount_scopes.borrow_mut();
        let mut mount_scopes = self.mount_scopes.borrow_mut();
        for s in unmount_scopes.difference(&mount_scopes) {
            composables.remove(&s);
            groups.remove(&s);
            if let Some(scope_states) = states.remove(&s) {
                for state in scope_states.keys() {
                    subs.remove(state);
                }
            }
            let use_states = uses.remove(&s);
            if let Some(use_states) = use_states {
                for state in use_states {
                    if let Some(subscribers) = subs.get_mut(&state) {
                        subscribers.remove(&s);
                    }
                }
            }
        }
        unmount_scopes.clear();
        mount_scopes.clear();
        let mut new_composables = self.new_composables.borrow_mut();
        composables.extend(new_composables.drain());
    }

    pub(crate) fn create_scope_with_node<C, P, S, I, A, F, U>(
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
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && c.is_visited(scope.id) {
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
                if let Some(curr_child_idx) = parent_child_idx {
                    let parent_grp = groups.get_mut(&parent.id).unwrap();
                    if let Some(existing_child) = parent_grp.children.get(curr_child_idx).cloned() {
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

    pub(crate) fn create_scope<C, P, S>(&self, parent: Scope<P, N>, scope: Scope<S, N>, content: C)
    where
        P: 'static,
        S: 'static,
        C: Fn(Scope<S, N>) + 'static,
    {
        let composable = move || {
            let parent = parent;
            let scope = scope;
            let c = parent.composer.read();
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && c.is_visited(scope.id) {
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
                if let Some(curr_child_idx) = parent_child_idx {
                    let parent_grp = groups.get_mut(&parent.id).unwrap();
                    if let Some(existing_child) = parent_grp.children.get(curr_child_idx).cloned() {
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
        *self.current_scope.borrow()
    }

    #[inline(always)]
    fn set_current_scope(&self, scope: ScopeId) {
        let mut current_scope = self.current_scope.borrow_mut();
        *current_scope = scope;
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
