use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    fmt::{Debug, Formatter},
    marker::PhantomData,
    mem,
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
    pub(crate) composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RwLock<HashMap<ScopeId, Group<N>>>,
    pub(crate) states: RwLock<HashMap<ScopeId, HashMap<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RwLock<HashMap<StateId, HashSet<ScopeId>>>,
    pub(crate) dirty_states: RwLock<HashSet<StateId>>,
    dirty_scopes: RwLock<HashSet<ScopeId>>,
    current_scope: RwLock<ScopeId>,
    child_count_stack: RwLock<Vec<usize>>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            composables: RwLock::new(HashMap::new()),
            new_composables: RwLock::new(HashMap::new()),
            groups: RwLock::new(HashMap::new()),
            current_scope: RwLock::new(ScopeId::new(0)),
            states: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
            dirty_states: RwLock::new(HashSet::new()),
            dirty_scopes: RwLock::new(HashSet::new()),
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
        let mut affected_scopes = HashSet::new();
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
            c.start_group(parent.id, scope.id);
            content(scope);
            c.end_group(parent.id, scope.id);
            c.set_current_scope(parent.id);
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
            c.start_group(parent.id, scope.id);
            content(scope);
            c.end_group(parent.id, scope.id);
            c.set_current_scope(parent.id);
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
    fn start_group(&self, parent: ScopeId, scope: ScopeId) {
        self.set_current_scope(scope);
        self.child_count_stack.write().unwrap().push(0);
        self.groups.write().unwrap().insert(scope, Group {
            node: None,
            parent,
            children: Vec::new(),
        });
    }

    #[inline(always)]
    fn end_group(&self, parent: ScopeId, scope: ScopeId) {
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
        if let Some(parent_child_count) = child_count_stack.last_mut() {
            *parent_child_count += 1;
            groups.get_mut(&parent).unwrap().children.push(scope);
        }
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
