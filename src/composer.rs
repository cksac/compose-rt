use std::{any::Any, collections::HashMap, marker::PhantomData, sync::RwLock};

use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};

use crate::{Loc, Root, Scope, ScopeId};

pub struct Group<N> {
    pub(crate) node: Option<N>,
}

pub struct Composer<N> {
    ty: PhantomData<N>,
    pub(crate) composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) new_composables: RwLock<HashMap<ScopeId, Box<dyn Fn()>>>,
    pub(crate) groups: RwLock<Vec<Group<N>>>,
}

impl<N> Composer<N> {
    pub fn new() -> Self {
        Self {
            ty: PhantomData,
            composables: RwLock::new(HashMap::new()),
            new_composables: RwLock::new(HashMap::new()),
            groups: RwLock::new(Vec::new()),
        }
    }

    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        N: 'static,
        F: Fn(Scope<Root, N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let scope = Scope::new(composer);
        root(scope);

        let c = composer.read();
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
        Recomposer { owner, composer }
    }

    #[track_caller]
    pub fn start_group<C>(&self, composable: C)
    where
        N: 'static,
        C: Fn() + 'static,
    {
        let id = ScopeId::new();
        let mut groups = self.groups.write().unwrap();
        let g = Group { node: None };
        groups.push(g);

        let composables = self.composables.read().unwrap();
        if !composables.contains_key(&id) {
            let mut new_composables = self.new_composables.write().unwrap();
            new_composables.insert(id, Box::new(composable));
        }
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
