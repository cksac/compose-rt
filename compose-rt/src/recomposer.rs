use crate::Composer;

pub struct Recomposer {
    pub(crate) composer: Composer,
}

impl Recomposer {
    pub fn root<R: 'static>(&self) -> Option<&R> {
        self.composer
            .tape
            .get(0)
            .and_then(|s| s.data.as_ref())
            .and_then(|n| n.as_any().downcast_ref::<R>())
    }

    pub fn root_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.composer
            .tape
            .get_mut(0)
            .and_then(|s| s.data.as_mut())
            .and_then(|n| n.as_any_mut().downcast_mut::<R>())
    }

    pub fn compose(self) -> Composer {
        self.composer
    }
}
