use rustc_hash::{FxHashMap, FxHashSet};

use std::{
    any::Any,
    collections::hash_map::Entry::{Occupied, Vacant},
    fmt::{Debug, Formatter},
    sync::RwLock,
};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};

use crate::{Root, Scope, ScopeId, state::StateId};

#[derive(Debug)]
pub struct Group<N> {
    parent: ScopeId,
    children: Vec<ScopeId>,
    node: Option<N>,
}

pub struct Composer<N> {
    pub(crate) composables: RwLock<FxHashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RwLock<FxHashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RwLock<FxHashMap<ScopeId, Group<N>>>,
    pub(crate) states: RwLock<FxHashMap<ScopeId, FxHashMap<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RwLock<FxHashMap<StateId, FxHashSet<ScopeId>>>,
    pub(crate) dirty_states: RwLock<FxHashSet<StateId>>,
    dirty_scopes: RwLock<FxHashSet<ScopeId>>,
    current_scope: RwLock<ScopeId>,
    child_count_stack: RwLock<Vec<usize>>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            composables: RwLock::new(FxHashMap::default()),
            new_composables: RwLock::new(FxHashMap::default()),
            groups: RwLock::new(FxHashMap::default()),
            current_scope: RwLock::new(ScopeId::new(0)),
            states: RwLock::new(FxHashMap::default()),
            subscribers: RwLock::new(FxHashMap::default()),
            dirty_states: RwLock::new(FxHashSet::default()),
            dirty_scopes: RwLock::new(FxHashSet::default()),
            child_count_stack: RwLock::new(Vec::new()),
        }
    }

    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>),
    {
        let id = ScopeId::new(1);
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let scope = Scope::new(id, composer);
        let c = composer.read();
        c.start_root(scope.id);
        root(scope);
        c.end_root(scope.id);
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
        Recomposer { owner, composer }
    }

    pub(crate) fn recompose(&self) {
        let mut affected_scopes = FxHashSet::default();
        {
            let mut dirty_states = self.dirty_states.write().unwrap();
            for state_id in dirty_states.drain() {
                let subscribers = self.subscribers.write().unwrap();
                if let Some(scopes) = subscribers.get(&state_id) {
                    affected_scopes.extend(scopes.iter().cloned());
                }
            }
        }
        let mut affected_scopes = affected_scopes.into_iter().collect::<Vec<_>>();
        affected_scopes.sort_by(|a, b| b.depth.cmp(&a.depth));
        {
            let mut dirty_scopes = self.dirty_scopes.write().unwrap();
            dirty_scopes.clear();
            dirty_scopes.extend(affected_scopes.iter().cloned());
        }
        for scope in affected_scopes {
            let composables = self.composables.read().unwrap();
            if let Some(composable) = composables.get(&scope) {
                composable();
            }
        }
        let mut new_composables = self.new_composables.write().unwrap();
        let mut composables = self.composables.write().unwrap();
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
            let scope = scope;
            let c = parent.composer.read();
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && c.visited_scope(scope.id) {
                return;
            }
            let parent_child_idx = c.start_group(scope.id);
            {
                let mut groups = c.groups.write().unwrap();
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
                            parent_grp.children[curr_child_idx] = scope.id;
                            groups.remove(&existing_child);
                        }
                    } else {
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
            let mut new_composables = self.new_composables.write().unwrap();
            if !new_composables.contains_key(&scope.id) {
                new_composables.insert(scope.id, Box::new(composable));
            }
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
            if !is_dirty && c.visited_scope(scope.id) {
                return;
            }
            let parent_child_idx = c.start_group(scope.id);
            {
                let mut groups = c.groups.write().unwrap();
                groups.entry(scope.id).or_insert_with(|| Group {
                    node: None,
                    parent: parent.id,
                    children: Vec::new(),
                });
                if let Some(curr_child_idx) = parent_child_idx {
                    let parent_grp = groups.get_mut(&parent.id).unwrap();
                    if let Some(existing_child) = parent_grp.children.get(curr_child_idx).cloned() {
                        if existing_child != scope.id {
                            parent_grp.children[curr_child_idx] = scope.id;
                            groups.remove(&existing_child);
                        }
                    } else {
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
            let mut new_composables = self.new_composables.write().unwrap();
            if !new_composables.contains_key(&scope.id) {
                new_composables.insert(scope.id, Box::new(composable));
            }
        }
    }

    #[inline(always)]
    fn start_root(&self, scope: ScopeId) {
        let parent = ScopeId::new(0);
        self.set_current_scope(scope);
        self.child_count_stack.write().unwrap().push(0);
        self.groups.write().unwrap().insert(scope, Group {
            node: None,
            parent,
            children: Vec::new(),
        });
    }

    #[inline(always)]
    fn end_root(&self, scope: ScopeId) {
        let mut child_count_stack = self.child_count_stack.write().unwrap();
        let child_count = child_count_stack.pop().unwrap();
        let mut groups = self.groups.write().unwrap();
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
        let parent_child_idx = self.child_count_stack.read().unwrap().last().cloned();
        self.set_current_scope(scope);
        self.child_count_stack.write().unwrap().push(0);
        parent_child_idx
    }

    #[inline(always)]
    fn end_group(&self, parent: ScopeId, scope: ScopeId) {
        let mut child_count_stack = self.child_count_stack.write().unwrap();
        let child_count = child_count_stack.pop().unwrap();
        let mut groups = self.groups.write().unwrap();
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
    pub(crate) fn get_current_scope(&self) -> ScopeId {
        *self.current_scope.read().unwrap()
    }

    #[inline(always)]
    fn set_current_scope(&self, scope: ScopeId) {
        let mut current_scope = self.current_scope.write().unwrap();
        *current_scope = scope;
    }

    fn is_registered(&self, scope: ScopeId) -> bool {
        let composables = self.composables.read().unwrap();
        composables.contains_key(&scope)
    }

    fn visited_scope(&self, scope: ScopeId) -> bool {
        let groups = self.groups.read().unwrap();
        groups.contains_key(&scope)
    }

    fn is_dirty(&self, scope: ScopeId) -> bool {
        let dirty_scopes = self.dirty_scopes.read().unwrap();
        dirty_scopes.contains(&scope)
    }

    fn clear_dirty(&self, scope: ScopeId) {
        let mut dirty_scopes = self.dirty_scopes.write().unwrap();
        dirty_scopes.remove(&scope);
    }
}

pub struct Recomposer<N> {
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N> {
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
