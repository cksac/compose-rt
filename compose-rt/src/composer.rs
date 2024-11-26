use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    fmt::{Debug, Formatter},
    mem,
    sync::RwLock,
};

use generational_box::{GenerationalBox, Owner};

use crate::{state::StateId, Composable, Recomposer, Scope, ScopeId, State};

#[derive(Debug)]
pub struct Cx<N> {
    pub(crate) composer: GenerationalBox<Composer<N>>,
}

impl<N> Clone for Cx<N>
where
    N: 'static,
{
    fn clone(&self) -> Self {
        Self {
            composer: self.composer.clone(),
        }
    }
}

impl<N> Copy for Cx<N> where N: 'static {}

impl<N> Cx<N>
where
    N: 'static,
{
    pub fn new_in(owner: &Owner) -> Self {
        let composer = Composer::new();
        Self {
            composer: owner.insert(composer),
        }
    }

    pub fn create_scope<P, S, C, A, I, F, U>(
        &self,
        parent: Scope<P>,
        scope: Scope<S>,
        content: C,
        input: A,
        factory: F,
        update: U,
    ) where
        P: 'static,
        S: 'static,
        C: Fn(Cx<N>, Scope<S>) + 'static,
        A: Fn() -> I + 'static,
        I: 'static,
        F: Fn(I) -> N + 'static,
        U: Fn(&mut N, I) + 'static,
    {
        let composable = move |cx: Cx<N>| {
            let c = cx.composer.read();
            let mut scope = scope;
            {
                let key = c.keys.read().unwrap().last().cloned();
                if let Some(key) = key {
                    scope.set_key(key);
                }
            }
            let parent_id = parent.id();
            let current_id = scope.id();
            let group_index = c.current_group_index();
            let mut group_size = 0;
            c.start_scope(current_id);
            // start_group
            {
                let input = input();
                let mut groups = c.groups.write().unwrap();
                if let Some(group) = groups.get_mut(group_index) {
                    group_size = group.size;

                    if group.id != current_id {
                        if group.ty != TypeId::of::<N>() {
                            // replace group
                            let new_group = Group {
                                id: current_id,
                                ty: TypeId::of::<S>(),
                                size: 1,
                                node: factory(input),
                                parent: parent_id,
                            };
                            let old_group = mem::replace(&mut groups[group_index], new_group);
                            let mut group_indexes = c.group_indexes.write().unwrap();
                            group_indexes.insert(current_id, group_index);
                            group_indexes.remove(&old_group.id);
                        } else {
                            // reuse group
                            update(&mut group.node, input);
                            let mut group_indexes = c.group_indexes.write().unwrap();
                            group_indexes.remove(&group.id);
                            group_indexes.insert(current_id, group_index);
                            group.id = current_id;
                        }
                    } else {
                        // update group
                        let is_dirty = c.is_dirty_scope(current_id);
                        if is_dirty {
                            update(&mut group.node, input);
                            if is_dirty {
                                c.clear_dirty_flag(current_id);
                            }
                        }
                    }
                } else {
                    // new group
                    groups.push(Group {
                        id: current_id,
                        ty: TypeId::of::<S>(),
                        size: 1,
                        node: factory(input),
                        parent: parent_id,
                    });
                    let mut group_indexes = c.group_indexes.write().unwrap();
                    group_indexes.insert(current_id, group_index);
                }
                let mut current_group_index = c.current_group_index.write().unwrap();
                *current_group_index += 1;
            }
            // release all locks before call children composable
            content(cx, scope);
            c.end_group(parent_id, group_index, group_size);
            c.end_scope(parent_id);
        };
        composable(*self);
        let c = self.composer.read();
        c.register_composable(scope.id(), composable);
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, factory: F) -> State<N, T>
    where
        F: FnOnce() -> T,
        T: 'static,
    {
        let c = self.composer.read();
        let scope = c.current_scope();
        let id = StateId::new();
        let mut states = c.states.write().unwrap();
        let scope_states = states.entry(scope).or_default();
        let _state = scope_states
            .entry(id)
            .or_insert_with(|| Box::new(factory()));
        State::<N, T>::new(*self, scope, id)
    }

    pub(crate) fn recompose(&self) {
        let c = self.composer.read();
        let mut affected_scopes = HashSet::new();
        {
            let mut dirty_states = c.dirty_states.write().unwrap();
            for state_id in dirty_states.drain() {
                let subscribers = c.subscribers.write().unwrap();
                if let Some(scopes) = subscribers.get(&state_id) {
                    affected_scopes.extend(scopes.iter().cloned());
                }
            }
        }
        let mut affected_scopes = affected_scopes.into_iter().collect::<Vec<_>>();
        affected_scopes.sort_by(|a, b| b.depth().cmp(&a.depth()));
        {
            let mut dirty_scopes = c.dirty_scopes.write().unwrap();
            dirty_scopes.clear();
            dirty_scopes.extend(affected_scopes.iter().cloned());
        }
        for scope in affected_scopes {
            let composables = c.composables.read().unwrap();
            if let Some(composable) = composables.get(&scope) {
                {
                    let trace = c.group_indexes.read().unwrap();
                    if let Some(start_trace) = trace.get(&scope) {
                        let mut current_trace = c.current_group_index.write().unwrap();
                        *current_trace = *start_trace;
                    } else {
                        continue;
                    }
                };
                composable.compose(*self);
            }
        }
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
        {
            let mut groups = c.groups.write().unwrap();
            let size = groups.first().unwrap().size;
            let removed_group = groups.drain(size..);
            let mut group_indexes = c.group_indexes.write().unwrap();
            for g in removed_group {
                if let Some(i) = group_indexes.get(&g.id) {
                    if i >= &size {
                        group_indexes.remove(&g.id);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Group<N> {
    id: ScopeId,
    ty: TypeId,
    size: usize,
    node: N,
    parent: ScopeId,
}

pub struct Composer<N> {
    composables: RwLock<HashMap<ScopeId, Box<dyn Composable<N>>>>,
    new_composables: RwLock<HashMap<ScopeId, Box<dyn Composable<N>>>>,
    pub(crate) states: RwLock<HashMap<ScopeId, HashMap<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RwLock<HashMap<StateId, HashSet<ScopeId>>>,
    pub(crate) dirty_states: RwLock<HashSet<StateId>>,
    dirty_scopes: RwLock<HashSet<ScopeId>>,
    current_scope: RwLock<ScopeId>,
    groups: RwLock<Vec<Group<N>>>,
    group_indexes: RwLock<HashMap<ScopeId, usize>>,
    current_group_index: RwLock<usize>,
    keys: RwLock<Vec<usize>>,
}

impl<N> Composer<N>
where
    N: 'static,
{
    pub fn new() -> Self {
        Self {
            composables: RwLock::new(HashMap::new()),
            new_composables: RwLock::new(HashMap::new()),
            groups: RwLock::new(Vec::new()),
            group_indexes: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
            dirty_states: RwLock::new(HashSet::new()),
            dirty_scopes: RwLock::new(HashSet::new()),
            current_scope: RwLock::new(ScopeId::new(0)),
            current_group_index: RwLock::new(0),
            keys: RwLock::new(Vec::new()),
        }
    }

    pub fn compose<F>(func: F) -> Recomposer<N>
    where
        F: Fn(Cx<N>) + 'static,
    {
        let recomposer = Recomposer::new();
        func(recomposer.cx);
        {
            let c = recomposer.cx.composer.read();
            let mut new_composables = c.new_composables.write().unwrap();
            let mut composables = c.composables.write().unwrap();
            composables.extend(new_composables.drain());
        }
        recomposer
    }

    #[inline(always)]
    pub(crate) fn start_key(&self, key: usize) {
        let mut keys = self.keys.write().unwrap();
        keys.push(key);
    }

    #[inline(always)]
    pub(crate) fn end_key(&self) {
        let mut keys = self.keys.write().unwrap();
        keys.pop();
    }

    #[inline(always)]
    fn start_scope(&self, scope: ScopeId) {
        let mut current_scope = self.current_scope.write().unwrap();
        *current_scope = scope;
    }

    #[inline(always)]
    fn end_scope(&self, parent: ScopeId) {
        let mut current_scope = self.current_scope.write().unwrap();
        *current_scope = parent;
    }

    #[inline(always)]
    fn current_group_index(&self) -> usize {
        *self.current_group_index.read().unwrap()
    }

    #[inline(always)]
    fn end_group(&self, parent: ScopeId, group_index: usize, old_group_size: usize) {
        let current_group_index = *self.current_group_index.read().unwrap();
        let new_group_size = if current_group_index - group_index > 1 {
            let mut groups = self.groups.write().unwrap();
            let size = current_group_index - group_index;
            groups[group_index].size = size;
            size
        } else {
            1
        };
        if new_group_size != old_group_size && old_group_size != 0 {
            // current group size changed, propagate to parent groups
            let mut groups = self.groups.write().unwrap();
            let lookup = self.group_indexes.read().unwrap();
            let mut parent_id = parent;
            loop {
                let parent_group_idx = lookup.get(&parent_id).unwrap();
                let parent_group = &mut groups[*parent_group_idx];
                if new_group_size > old_group_size {
                    parent_group.size += new_group_size - old_group_size;
                } else {
                    parent_group.size -= old_group_size - new_group_size;
                }
                if parent_group.parent == parent_id {
                    break;
                }
                parent_id = parent_group.parent;
            }
        }
    }

    fn is_dirty_scope(&self, scope: ScopeId) -> bool {
        let dirty_scopes = self.dirty_scopes.read().unwrap();
        dirty_scopes.contains(&scope)
    }

    fn clear_dirty_flag(&self, scope: ScopeId) {
        let mut dirty_scopes = self.dirty_scopes.write().unwrap();
        dirty_scopes.remove(&scope);
    }

    fn register_composable<C>(&self, scope: ScopeId, composable: C)
    where
        C: Composable<N>,
    {
        let composables = self.composables.read().unwrap();
        let mut new_composables = self.new_composables.write().unwrap();
        if !composables.contains_key(&scope) {
            new_composables.insert(scope, Box::new(composable));
        }
    }

    pub(crate) fn current_scope(&self) -> ScopeId {
        *self.current_scope.read().unwrap()
    }
}

impl<N> Debug for Composer<N>
where
    N: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("groups", &self.groups)
            .field("group_indexes", &self.group_indexes)
            .field("subscribers", &self.subscribers)
            .finish()
    }
}

impl<N> Debug for Recomposer<N>
where
    N: 'static + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let composer = self.cx.composer.read();
        f.debug_struct("Recomposer")
            .field("groups", &composer.groups)
            .field("group_indexes", &composer.group_indexes)
            .field("subscribers", &composer.subscribers)
            .finish()
    }
}
