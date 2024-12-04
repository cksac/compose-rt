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
    id: ScopeId,
    parent: ScopeId,
    scope_ty: TypeId,
    size: usize,
    node: Option<N>,
}

pub struct Composer<N> {
    ty: PhantomData<N>,
    pub(crate) composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RwLock<Vec<Group<N>>>,
    group_indexes: RwLock<HashMap<ScopeId, usize>>,
    pub(crate) states: RwLock<HashMap<ScopeId, HashMap<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RwLock<HashMap<StateId, HashSet<ScopeId>>>,
    pub(crate) dirty_states: RwLock<HashSet<StateId>>,
    dirty_scopes: RwLock<HashSet<ScopeId>>,
    current_scope: RwLock<ScopeId>,
    current_group_index: RwLock<usize>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            ty: PhantomData,
            composables: RwLock::new(HashMap::new()),
            new_composables: RwLock::new(HashMap::new()),
            groups: RwLock::new(Vec::new()),
            group_indexes: RwLock::new(HashMap::new()),
            current_scope: RwLock::new(ScopeId::new(0)),
            current_group_index: RwLock::new(0),
            states: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
            dirty_states: RwLock::new(HashSet::new()),
            dirty_scopes: RwLock::new(HashSet::new()),
        }
    }

    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>),
    {
        let id = ScopeId::new(0);
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let scope = Scope::new(id, composer);
        let c = composer.read();
        c.set_current_scope(scope.id);
        root(scope);
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

        println!("affected_scopes: {:#?}", affected_scopes);

        affected_scopes.sort_by(|a, b| b.depth.cmp(&a.depth));
        {
            let mut dirty_scopes = self.dirty_scopes.write().unwrap();
            dirty_scopes.clear();
            dirty_scopes.extend(affected_scopes.iter().cloned());
        }
        for scope in affected_scopes {
            let composables = self.composables.read().unwrap();
            if let Some(composable) = composables.get(&scope) {
                {
                    let trace = self.group_indexes.read().unwrap();
                    if let Some(start_trace) = trace.get(&scope) {
                        let mut current_trace = self.current_group_index.write().unwrap();
                        *current_trace = *start_trace;
                    } else {
                        continue;
                    }
                };
                composable();
            }
        }
        let mut new_composables = self.new_composables.write().unwrap();
        let mut composables = self.composables.write().unwrap();
        composables.extend(new_composables.drain());
        {
            let mut groups = self.groups.write().unwrap();
            let size = groups.first().unwrap().size;
            let removed_group = groups.drain(size..);
            let mut group_indexes = self.group_indexes.write().unwrap();
            for g in removed_group {
                if let Some(i) = group_indexes.get(&g.id) {
                    if i >= &size {
                        group_indexes.remove(&g.id);
                    }
                }
            }
        }
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
            c.set_current_scope(scope.id);
            let curr_grp_index = c.get_current_group_index();
            {
                let args = input();
                let mut groups = c.groups.write().unwrap();
                if let Some(g) = groups.get_mut(curr_grp_index) {
                    if g.id != scope.id {
                        if g.scope_ty != TypeId::of::<S>() {
                            // replace group
                            let new_grp = Group {
                                id: scope.id,
                                parent: parent.id,
                                scope_ty: TypeId::of::<S>(),
                                size: 1,
                                node: Some(factory(args)),
                            };
                            // TODO: use gap buffer
                            let old_grp = mem::replace(&mut groups[curr_grp_index], new_grp);
                            // update group indexes lookup
                            let mut group_indexes = c.group_indexes.write().unwrap();
                            group_indexes.insert(scope.id, curr_grp_index);
                            group_indexes.remove(&old_grp.id);
                        } else {
                            // reuse group
                            update(g.node.as_mut().unwrap(), args);
                            // update group indexes lookup
                            let mut group_indexes = c.group_indexes.write().unwrap();
                            group_indexes.insert(scope.id, curr_grp_index);
                            group_indexes.remove(&g.id);
                            g.id = scope.id;
                        }
                    } else {
                        // update group
                        let is_dirty = c.is_dirty(scope.id);
                        if is_dirty {
                            update(g.node.as_mut().unwrap(), args);
                            if is_dirty {
                                c.clear_dirty(scope.id);
                            }
                        }
                    }
                } else {
                    // new group
                    let new_grp = Group {
                        id: scope.id,
                        parent: parent.id,
                        scope_ty: TypeId::of::<S>(),
                        size: 1,
                        node: Some(factory(args)),
                    };
                    groups.push(new_grp);
                    let mut group_indexes = c.group_indexes.write().unwrap();
                    group_indexes.insert(scope.id, curr_grp_index);
                }
                c.set_current_group_index(curr_grp_index + 1);
            }
            content(scope);
            c.end_group(parent.id, scope.id, curr_grp_index);
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
            c.set_current_scope(scope.id);
            let curr_grp_index = c.get_current_group_index();
            {
                let mut groups = c.groups.write().unwrap();
                if let Some(g) = groups.get_mut(curr_grp_index) {
                    if g.id != scope.id {
                        // replace group
                        let new_grp = Group {
                            id: scope.id,
                            parent: parent.id,
                            scope_ty: TypeId::of::<S>(),
                            size: 1,
                            node: None::<N>,
                        };
                        // TODO: use gap buffer
                        let old_grp = mem::replace(&mut groups[curr_grp_index], new_grp);
                        let mut group_indexes = c.group_indexes.write().unwrap();
                        group_indexes.insert(scope.id, curr_grp_index);
                        group_indexes.remove(&old_grp.id);
                    }
                } else {
                    // new group
                    let new_grp = Group {
                        id: scope.id,
                        parent: parent.id,
                        scope_ty: TypeId::of::<S>(),
                        size: 1,
                        node: None::<N>,
                    };
                    groups.push(new_grp);
                    let mut group_indexes = c.group_indexes.write().unwrap();
                    group_indexes.insert(scope.id, curr_grp_index);
                }
                c.set_current_group_index(curr_grp_index + 1);
            }
            content(scope);
            c.end_group(parent.id, scope.id, curr_grp_index);
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
    fn end_group(&self, parent: ScopeId, scope: ScopeId, group_index: usize) {
        let current_group_index = *self.current_group_index.read().unwrap();
        let new_group_size = current_group_index - group_index;
        let old_group_size = {
            let mut groups = self.groups.write().unwrap();
            let g = &mut groups[group_index];
            let old_group_size = g.size;
            if g.size != new_group_size {
                g.size = new_group_size;
            }
            old_group_size
        };
        // TODO: don't need to propogate if first compose
        if new_group_size != old_group_size {
            // current group size changed, propagate to parent groups
            let mut groups = self.groups.write().unwrap();
            let lookup = self.group_indexes.read().unwrap();
            let mut parent_id = parent;
            loop {
                if parent_id.depth == 0 {
                    break;
                }

                let parent_group_idx = lookup.get(&parent_id).unwrap_or_else(|| {
                    panic!(
                        "parent group not found for scope: {:?}, parent: {:?}, lookup: {:?}",
                        scope, parent_id, lookup
                    )
                });
                let parent_group = &mut groups[*parent_group_idx];
                if new_group_size > old_group_size {
                    parent_group.size += new_group_size - old_group_size;
                } else {
                    parent_group.size -= old_group_size - new_group_size;
                }

                parent_id = parent_group.parent;
            }
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

    #[inline(always)]
    fn get_current_group_index(&self) -> usize {
        *self.current_group_index.read().unwrap()
    }

    #[inline(always)]
    fn set_current_group_index(&self, index: usize) {
        let mut current_trace = self.current_group_index.write().unwrap();
        *current_trace = index;
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
            .field("group_indexes", &self.group_indexes)
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
            .field("group_indexes", &c.group_indexes)
            .finish()
    }
}
