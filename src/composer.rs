use std::{
    any::TypeId,
    collections::HashMap,
    fmt::{Debug, Formatter},
    marker::PhantomData,
    mem,
    sync::RwLock,
};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};

use crate::{Root, Scope, ScopeId};

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
    current_scope: RwLock<ScopeId>,
    current_group_index: RwLock<usize>,
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

impl<N> Composer<N> {
    pub fn new() -> Self {
        Self {
            ty: PhantomData,
            composables: RwLock::new(HashMap::new()),
            new_composables: RwLock::new(HashMap::new()),
            groups: RwLock::new(Vec::new()),
            current_scope: RwLock::new(ScopeId::new()),
            current_group_index: RwLock::new(0),
        }
    }

    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        N: 'static,
        F: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let id = ScopeId::new();
        let scope = Scope::new(id, composer);
        let c = composer.read();
        c.set_current_scope(scope.id);
        root(scope);
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
        Recomposer { owner, composer }
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
        N: 'static,
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
            let mut grp_size = 0;
            {
                let args = input();
                let mut groups = c.groups.write().unwrap();
                if let Some(g) = groups.get_mut(curr_grp_index) {
                    grp_size = g.size;
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
                        } else {
                            // reuse group
                            g.id = scope.id;
                            update(g.node.as_mut().unwrap(), args);
                        }
                    } else {
                        // update group
                        update(g.node.as_mut().unwrap(), args);
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
                }
                c.set_current_group_index(curr_grp_index + 1);
            }
            content(scope);
            c.end_group(curr_grp_index);
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
        N: 'static,
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
            let mut grp_size = 0;
            {
                let mut groups = c.groups.write().unwrap();
                if let Some(g) = groups.get_mut(curr_grp_index) {
                    grp_size = g.size;
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
                }
                c.set_current_group_index(curr_grp_index + 1);
            }
            content(scope);
            c.end_group(curr_grp_index);
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
    fn end_group(&self, group_index: usize) {
        let current_group_index = *self.current_group_index.read().unwrap();
        let mut groups = self.groups.write().unwrap();
        let size = current_group_index - group_index;
        groups[group_index].size = size;
        // TODO: detect group size change, and propagate to parent
    }

    #[inline(always)]
    fn get_current_scope(&self) -> ScopeId {
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
}

pub struct Recomposer<N> {
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N> {
    pub fn recompose(&self)
    where
        N: 'static,
    {
        let c = self.composer.read();
        {
            let composables = c.composables.read().unwrap();
            for (_, c) in composables.iter() {
                c();
            }
        }
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
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
