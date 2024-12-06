use std::{
    fmt::{Debug, Formatter, Result},
    hash::Hash,
    panic::Location,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    location: &'static Location<'static>,
}

impl Loc {
    #[track_caller]
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            location: Location::caller(),
        }
    }

    #[inline(always)]
    pub fn id(&self) -> usize {
        self.location as *const _ as usize
    }
}

impl Debug for Loc {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.location)
    }
}

impl Hash for Loc {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}
