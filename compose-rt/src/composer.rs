use crate::{CallId, Slot, SlotId};
use downcast_rs::{impl_downcast, Downcast};
use log::Level::Trace;
use log::{log_enabled, trace};
use lru::LruCache;
use std::{
    any::{type_name, TypeId},
    fmt::Debug,
    panic::Location,
    sync::atomic::{AtomicUsize, Ordering},
};

pub trait ComposeNode: Debug + Downcast {}
impl_downcast!(ComposeNode);
impl<T: Debug + Downcast> ComposeNode for T {}

impl dyn ComposeNode {
    #[inline]
    pub fn cast_ref<T: 'static>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    #[inline]
    pub fn cast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }
}

type Tape = Vec<Slot<Box<dyn ComposeNode>>>;

#[derive(Debug, Clone, Copy)]
struct SlotPlaceHolder;

static COMPOSER_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Composer {
    pub(crate) id: usize,
    pub(crate) composing: bool,
    pub(crate) tape: Tape,
    pub(crate) slot_depth: Vec<usize>,
    pub(crate) depth: usize,
    pub(crate) cursor: usize,
    pub(crate) slot_key: Option<usize>,
    pub(crate) recycle_bin: LruCache<SlotId, Tape>,
    pub(crate) state_tape: Tape,
    pub(crate) state_cursor: usize,
    pub(crate) child_cursors: Vec<Vec<usize>>,
}

impl Composer {
    pub(crate) fn new(capacity: usize) -> Self {
        Composer {
            id: COMPOSER_ID.fetch_add(1, Ordering::SeqCst),
            composing: true,
            tape: Vec::new(),
            slot_depth: Vec::new(),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: LruCache::new(capacity),
            state_tape: Vec::new(),
            state_cursor: 0,
            child_cursors: Vec::new(),
        }
    }

    #[track_caller]
    pub fn state<F, Node>(&mut self, factory: F) -> Node
    where
        F: FnOnce() -> Node,
        Node: ComposeNode + Clone,
    {
        self.state_with(None, factory)
    }

    #[track_caller]
    pub fn state_with<F, Node>(&mut self, key: Option<usize>, factory: F) -> Node
    where
        F: FnOnce() -> Node,
        Node: ComposeNode + Clone,
    {
        // save current states
        let curr_cursor = self.state_cursor;

        // forward cursors
        self.state_cursor += 1;

        // construct slot id
        let call_id = CallId::from(Location::caller());
        let curr_slot_id = SlotId::new(call_id, key);

        // found in recycle_bin, restore it to current cursor
        if let Some(slots) = self.recycle_bin.pop(&curr_slot_id) {
            let mut curr_idx = curr_cursor;
            // TODO: use gap table?
            for slot in slots {
                self.state_tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        let cached = self.state_tape.get(curr_cursor).map(|s| {
            let data = s.data.as_ref();
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if curr_slot_id == p_slot_id {
                if let Some(node) = p_data.cast_ref::<Node>() {
                    if log_enabled!(Trace) {
                        trace!(
                            "{}:{}{} | {:?} | {:?}",
                            format!("{}{: >6}", "cs", curr_cursor),
                            "  ".repeat(self.depth),
                            type_name::<Node>(),
                            TypeId::of::<Node>(),
                            curr_slot_id
                        );
                    }
                    return node.clone();
                }
            }
            // move previous cached slot to recycle bin
            if log_enabled!(Trace) {
                trace!(
                    "{}:{}{} | {:?} | {:?}",
                    format!("{}{: >6}", "-s", curr_cursor),
                    "  ".repeat(self.depth),
                    type_name::<Node>(),
                    TypeId::of::<Node>(),
                    curr_slot_id
                );
            }
            let slot_end = curr_cursor + p_size;
            let slots = self.state_tape.drain(curr_cursor..slot_end).collect();
            self.recycle_bin.put(p_slot_id, slots);
        }

        let val = factory();
        let node = Box::new(val.clone());
        let slot: Slot<Box<dyn ComposeNode>> = Slot::new(curr_slot_id, node);
        if log_enabled!(Trace) {
            trace!(
                "{}:{}{} | {:?} | {:?}",
                format!("{}{: >6}", "+s", curr_cursor),
                "  ".repeat(self.depth),
                type_name::<Node>(),
                TypeId::of::<Node>(),
                curr_slot_id
            );
        }
        self.state_tape.insert(curr_cursor, slot);
        val
    }

    #[track_caller]
    pub fn tag<F, T>(&mut self, key: usize, func: F) -> T
    where
        F: FnOnce(&mut Composer) -> T,
    {
        let id = self.id;
        let curr_cursor = self.cursor;

        // set the key of next encountered slot
        self.slot_key = Some(key);

        let result = func(self);
        assert!(
            // len >= curr_cursor, if func don't create any slot
            id == self.id && self.composing && self.tape.len() >= curr_cursor,
            "Composer is in inconsistent state"
        );
        result
    }

    #[track_caller]
    pub fn remember<F, Node>(&mut self, factory: F) -> Node
    where
        F: FnOnce() -> Node,
        Node: ComposeNode + Clone,
    {
        self.group(|_| factory(), |_| true, |_| {}, |_, _| {}, |n| n.clone())
    }

    #[track_caller]
    pub fn memo<F, Node, S, U, O, Output>(
        &mut self,
        factory: F,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&Composer) -> Node,
        Node: ComposeNode,
        S: FnOnce(&Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&mut Node) -> Output,
    {
        self.group(factory, skip, |_| {}, |n, _| update(n), output)
    }

    #[track_caller]
    pub fn group<F, Node, S, C, CO, U, O, Output>(
        &mut self,
        factory: F,
        skip: S,
        content: C,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer) -> CO,
        S: FnOnce(&Node) -> bool,
        U: FnOnce(&mut Node, CO),
        O: FnOnce(&mut Node) -> Output,
    {
        self.slot(factory, skip, content, false, |_, _| {}, update, output)
    }

    #[track_caller]
    pub fn group_use_children<F, Node, S, C, CO, UC, U, O, Output>(
        &mut self,
        factory: F,
        skip: S,
        content: C,
        use_children: UC,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&Composer) -> Node,
        Node: ComposeNode,
        S: FnOnce(&Node) -> bool,
        C: FnOnce(&mut Composer) -> CO,
        UC: FnOnce(&mut Node, Children),
        U: FnOnce(&mut Node, CO),
        O: FnOnce(&mut Node) -> Output,
    {
        self.slot(factory, skip, content, true, use_children, update, output)
    }

    #[track_caller]
    #[allow(clippy::too_many_arguments)]
    pub fn slot<F, Node, S, C, CO, UC, U, O, Output>(
        &mut self,
        factory: F,
        skip: S,
        content: C,
        require_use_children: bool,
        use_children: UC,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&Composer) -> Node,
        Node: ComposeNode,
        S: FnOnce(&Node) -> bool,
        C: FnOnce(&mut Composer) -> CO,
        UC: FnOnce(&mut Node, Children),
        U: FnOnce(&mut Node, CO),
        O: FnOnce(&mut Node) -> Output,
    {
        // remember current slot states
        let id = self.id;
        let curr_cursor = self.cursor;
        let curr_depth = self.depth;

        // construct slot id
        let call_id = CallId::from(Location::caller());
        let key = self.slot_key.take();
        let curr_slot_id = SlotId::new(call_id, key);

        // forward cursor and depth
        self.cursor += 1;
        self.depth += 1;

        // update current slot depth
        if let Some(d) = self.slot_depth.get_mut(curr_cursor) {
            *d = curr_depth
        } else {
            self.slot_depth.insert(curr_cursor, curr_depth);
        }

        // save curr_cursor to parent's children list
        if let Some(p) = self.child_cursors.last_mut() {
            p.push(curr_cursor);
        }

        // found in recycle_bin, restore it to curr_cursor
        if let Some(slots) = self.recycle_bin.pop(&curr_slot_id) {
            let mut curr_idx = curr_cursor;
            // TODO: use gap table?
            for slot in slots {
                self.tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        // cache and skip check
        let (is_exist, is_match, is_skip) = self
            .tape
            .get(curr_cursor)
            .map(|slot| {
                if curr_slot_id == slot.id {
                    if let Some(node) = slot.data.as_ref().cast_ref::<Node>() {
                        (true, true, skip(node))
                    } else {
                        (true, false, false)
                    }
                } else {
                    (true, false, false)
                }
            })
            .unwrap_or((false, false, false));

        // exist but cache miss
        if is_exist && !is_match {
            // move current cached slot to recycle bin
            let p_slot = self.tape.get(curr_cursor).expect("slot");
            let p_ty_id = p_slot.data.as_ref().type_id();
            let p_slot_id = p_slot.id;
            let p_slot_size = p_slot.size;

            if log_enabled!(Trace) {
                trace!(
                    "-{: >7}:{}{:?} | {:?} | {:?}",
                    curr_cursor,
                    "  ".repeat(self.depth),
                    p_ty_id,
                    p_slot_id,
                    p_slot_size
                );
            }
            let slot_end = curr_cursor + p_slot_size;
            let slots: Tape = self.tape.drain(curr_cursor..slot_end).collect();
            self.recycle_bin.put(p_slot_id, slots);
        }

        // exist and cache hit and skip
        if is_exist && is_match && is_skip {
            if log_enabled!(Trace) {
                trace!(
                    "S{: >7}:{}{} | {:?} | {:?}",
                    curr_cursor,
                    "  ".repeat(self.depth),
                    type_name::<Node>(),
                    TypeId::of::<Node>(),
                    curr_slot_id
                );
            }

            let slot = self.tape.get_mut(curr_cursor).expect("slot");
            let node = slot
                .data
                .as_mut()
                .cast_mut::<Node>()
                .expect("downcast node");

            // skip to slot end
            self.cursor = curr_cursor + slot.size;
            self.depth -= 1;
            return output(node);
        }

        // call factory if not exist or mismatch
        if !is_exist || !is_match {
            self.tape.insert(
                curr_cursor,
                Slot::new(curr_slot_id, Box::new(SlotPlaceHolder)),
            );
            let node = Box::new(factory(self));
            assert!(
                id == self.id && self.composing && self.tape.len() > curr_cursor,
                "Composer is in inconsistent state"
            );
            // update current slot data to return of factory
            self.tape[curr_cursor].data = node;
        }

        if log_enabled!(Trace) {
            trace!(
                "{}{: >7}:{}{} | {:?} | {:?}",
                if is_exist && is_match { "C" } else { "+" },
                curr_cursor,
                "  ".repeat(self.depth),
                type_name::<Node>(),
                TypeId::of::<Node>(),
                curr_slot_id
            );
        }

        // new or update case
        if require_use_children {
            self.child_cursors.push(Vec::new());
        }
        let co = content(self);
        assert!(
            id == self.id && self.composing && self.tape.len() > curr_cursor,
            "Composer is in inconsistent state"
        );

        let child_start = curr_cursor + 1;
        let (head_slots, tail_slots) = self.tape.split_at_mut(child_start);

        // NOTE: can use head_slots.split_last_mut() to get current_slot and its previous slots in case require to mutate parent
        let current_slot = head_slots.last_mut().expect("slots");
        let node = current_slot
            .data
            .as_mut()
            .cast_mut::<Node>()
            .expect("downcast node");

        // update new slot size after children fn done
        current_slot.size = self.cursor - curr_cursor;

        if require_use_children {
            let child_cursors = self.child_cursors.pop().expect("child_cursors");
            let node_children = Children::new(
                &mut tail_slots[..current_slot.size - 1],
                child_start,
                child_cursors,
            );
            use_children(node, node_children);
        }
        update(node, co);

        if log_enabled!(Trace) && current_slot.size > 1 {
            trace!(
                "{}{: >7}:{}{} | {:?} | {:?}",
                if is_exist && is_match { "C" } else { "+" },
                curr_cursor,
                "  ".repeat(self.depth),
                type_name::<Node>(),
                TypeId::of::<Node>(),
                curr_slot_id
            );
        }
        self.depth -= 1;
        output(node)
    }
}

#[derive(Debug)]
pub struct Children<'a> {
    pub(crate) tape: &'a mut [Slot<Box<dyn ComposeNode>>],
    pub(crate) cursors: Vec<usize>,
}

impl<'a> Children<'a> {
    fn new(
        tape: &'a mut [Slot<Box<dyn ComposeNode>>],
        child_start: usize,
        mut child_cursors: Vec<usize>,
    ) -> Self {
        // tape only contains slot start from child_start
        // child_cursors is global, offset to child start
        child_cursors.iter_mut().for_each(|v| *v -= child_start);
        Children {
            tape,
            cursors: child_cursors,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn ComposeNode> {
        self.cursors
            .iter()
            .filter_map(|c| self.tape.get(*c).map(|s| s.data.as_ref()))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut dyn ComposeNode> {
        // Note: self.cursors should be sorted to allow iter_mut to work properly
        let mut offset = 0;
        let mut tape = self.tape.iter_mut();
        self.cursors
            .iter()
            .filter_map(move |&c| {
                let translated = c - offset;
                let value = tape.nth(translated);
                offset += translated + 1;
                value
            })
            .map(|s| s.data.as_mut())
    }
}
