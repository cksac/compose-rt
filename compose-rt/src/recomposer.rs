use crate::Composer;

pub struct Recomposer {
    pub(crate) composer: Composer,
}

impl Recomposer {
    pub fn new() -> Self {
        Recomposer {
            composer: Composer::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Recomposer {
            composer: Composer::with_capacity(capacity),
        }
    }

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

    pub fn compose<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut Composer) -> T,
    {
        let id = self.composer.id;
        self.composer.composing = true;
        let t = func(&mut self.composer);
        assert!(id == self.composer.id, "Composer changed");

        self.finalize();
        t
    }

    fn finalize(&mut self) {
        self.composer.tape.truncate(self.composer.cursor);
        self.composer.slot_depth.truncate(self.composer.cursor);
        self.composer.cursor = 0;
        self.composer.depth = 0;
        self.composer.recycle_bin.clear();

        self.composer
            .state_tape
            .truncate(self.composer.state_cursor);
        self.composer.state_cursor = 0;
        self.composer.composing = false;
    }
}

impl Default for Recomposer {
    fn default() -> Self {
        Self::new()
    }
}
