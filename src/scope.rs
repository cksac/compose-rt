use std::{
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
};

use generational_box::GenerationalBox;

use crate::{Composer, Loc, composer::Group};

pub struct Scope<S, N> {
    _scope: PhantomData<S>,
    composer: GenerationalBox<Composer<N>>,
}

impl<S, N> Clone for Scope<S, N> {
    fn clone(&self) -> Self {
        Self {
            _scope: PhantomData,
            composer: self.composer.clone(),
        }
    }
}

impl<S, N> Copy for Scope<S, N> {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: 'static,
{
    pub fn new(composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            _scope: PhantomData,
            composer,
        }
    }

    pub fn child_scope<C>(&self) -> Scope<C, N>
    where
        C: 'static,
    {
        Scope::new(self.composer)
    }

    pub fn build_container<C>(self, content: C)
    where
        C: Fn(Self) + 'static,
    {
        let c = self.composer.read();
        let composable = move || {
            content(self);
        };
        c.start_group(composable);
    }

    pub fn build(self) {
        let c = self.composer.read();
        let composable = move || {};
        c.start_group(composable);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId {
    pub loc: Loc,
    pub key: usize,
}

impl ScopeId {
    #[track_caller]
    pub fn new() -> Self {
        let loc = Loc::new();
        Self { loc, key: 0 }
    }

    #[track_caller]
    pub fn with_key(key: usize) -> Self {
        let loc = Loc::new();
        Self { loc, key }
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}-{}", self.loc, self.key)
    }
}

pub struct Root;
