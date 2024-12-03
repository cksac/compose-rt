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
        root(scope);

        let c = composer.read();
        let mut new_composables = c.new_composables.write().unwrap();
        let mut composables = c.composables.write().unwrap();
        composables.extend(new_composables.drain());
        Recomposer { owner, composer }
    }

    pub(crate) fn create_group<C, P, S, I, A, F, U>(
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
            content(scope);

            let args = input();
            factory(args);
        };
        composable();
        let registered = {
            let composables = self.composables.read().unwrap();
            composables.contains_key(&scope.id)
        };
        if registered {
            let mut new_composables = self.new_composables.write().unwrap();
            if !new_composables.contains_key(&scope.id) {
                new_composables.insert(scope.id, Box::new(composable));
            }
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
