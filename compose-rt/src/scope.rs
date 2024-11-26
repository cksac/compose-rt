use std::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
    panic::Location,
};

use crate::composer::Cx;

pub type Loc = &'static Location<'static>;

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopeId {
    loc: Loc,
    depth: usize,
    key: Option<usize>,
}

impl ScopeId {
    #[track_caller]
    pub fn new(depth: usize) -> Self {
        let loc = Location::caller();
        Self {
            loc,
            depth,
            key: None,
        }
    }

    #[track_caller]
    pub fn with_key(depth: usize, key: usize) -> Self {
        let loc = Location::caller();
        Self {
            loc,
            depth,
            key: Some(key),
        }
    }

    pub fn set_key(&mut self, key: usize) {
        self.key = Some(key);
    }

    pub fn loc(&self) -> Loc {
        self.loc
    }

    pub fn id(&self) -> usize {
        self.loc as *const _ as usize
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn key(&self) -> Option<usize> {
        self.key
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.key {
            Some(key) => write!(f, "{}@{}-{}", self.loc, self.depth, key),
            None => write!(f, "{}@{}", self.loc, self.depth),
        }
    }
}

pub struct Scope<T> {
    ty: PhantomData<T>,
    id: ScopeId,
}

impl<T> Scope<T> {
    #[track_caller]
    pub fn new() -> Self {
        Self {
            ty: PhantomData,
            id: ScopeId::new(0),
        }
    }

    #[track_caller]
    pub fn with_key(key: usize) -> Self {
        Self {
            ty: PhantomData,
            id: ScopeId::with_key(0, key),
        }
    }

    #[track_caller]
    pub fn child_scope<S>(&self) -> Scope<S> {
        Scope::<S> {
            ty: PhantomData,
            id: ScopeId::new(self.id.depth + 1),
        }
    }

    #[track_caller]
    pub fn child_scope_with_key<S>(&self, key: usize) -> Scope<S> {
        Scope::<S> {
            ty: PhantomData,
            id: ScopeId::with_key(self.id.depth + 1, key),
        }
    }

    pub fn id(&self) -> ScopeId {
        self.id
    }

    pub fn set_key(&mut self, key: usize) {
        self.id.set_key(key);
    }
}

impl<T> Debug for Scope<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope")
            .field("ty", &self.ty)
            .field("id", &self.id)
            .finish()
    }
}

impl<T> Clone for Scope<T> {
    fn clone(&self) -> Self {
        Self {
            ty: PhantomData,
            id: self.id,
        }
    }
}

impl<T> Copy for Scope<T> {}

#[track_caller]
pub fn key<N, C>(cx: Cx<N>, key: usize, content: C)
where
    N: 'static,
    C: Fn(),
{
    let c = cx.composer.read();
    c.start_key(key);
    content();
    c.end_key();
}
