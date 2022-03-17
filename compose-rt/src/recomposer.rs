use crate::Composer;

#[derive(Debug)]
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
            .map(|s| &s.data)
            .and_then(|n| n.cast_ref::<R>())
    }

    pub fn root_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.composer
            .tape
            .get_mut(0)
            .map(|s| &mut s.data)
            .and_then(|n| n.cast_mut::<R>())
    }

    pub fn compose<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut Composer) -> T,
    {
        let composer = &mut self.composer;
        let id = composer.id;
        let curr_cursor = composer.cursor;
        composer.composing = true;
        let t = func(composer);
        assert!(
            // len >= curr_cursor, if func don't create any slot
            id == composer.id && composer.composing && composer.tape.len() >= curr_cursor,
            "Composer is in inconsistent state"
        );
        self.finalize();
        t
    }

    fn finalize(&mut self) {
        let composer = &mut self.composer;
        // TODO: move all extra slots to recycle_bin and allow user to decide housekeeping or not?
        composer.tape.truncate(composer.cursor);
        composer.slot_depth.truncate(composer.cursor);
        composer.state_tape.truncate(composer.state_cursor);
        composer.recycle_bin.clear();

        composer.cursor = 0;
        composer.depth = 0;
        composer.state_cursor = 0;
        composer.composing = false;
    }
}

impl Default for Recomposer {
    fn default() -> Self {
        Self::new()
    }
}
