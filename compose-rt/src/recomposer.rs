use crate::{ComposeNode, Composer};
use std::any::Any;

pub struct Recomposer<N: ?Sized> {
    pub(crate) composer: Composer<N>,
}

impl<N> Recomposer<N>
where
    N: 'static + ?Sized + Any + Unpin,
    for<'a> &'a mut N: ComposeNode,
{
    pub fn root_mut(&mut self) -> Option<&mut N> {
        self.composer
            .tape
            .get_mut(0)
            .and_then(|s| s.data.as_mut().map(|d| d.as_mut().get_mut()))
    }

    pub fn compose(self) -> Composer<N> {
        self.composer
    }
}
