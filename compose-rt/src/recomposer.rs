use generational_box::{AnyStorage, Owner, UnsyncStorage};

use crate::composer::Cx;

pub struct Recomposer<N> {
    #[allow(dead_code)]
    owner: Owner,
    pub(crate) cx: Cx<N>,
}

impl<N> Recomposer<N>
where
    N: 'static,
{
    pub(crate) fn new() -> Self {
        let owner = UnsyncStorage::owner();
        let cx = Cx::new_in(&owner);
        Self { owner, cx }
    }
}

impl<N> Recomposer<N>
where
    N: 'static,
{
    #[inline(always)]
    pub fn recompose(&self) {
        self.cx.recompose();
    }
}
