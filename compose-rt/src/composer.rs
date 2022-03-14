use crate::{CallId, Recomposer, Slot, SlotId};
use downcast_rs::{impl_downcast, Downcast};
use log::trace;
use std::{collections::HashMap, panic::Location};

pub trait ComposeNode: Downcast {}
impl_downcast!(ComposeNode);
impl<T: Downcast> ComposeNode for T {}

type Tape = Vec<Slot<Box<dyn ComposeNode>>>;

pub struct Composer {
    pub(crate) tape: Tape,
    pub(crate) slot_depth: Vec<usize>,
    pub(crate) depth: usize,
    pub(crate) cursor: usize,
    pub(crate) slot_key: Option<usize>,
    pub(crate) recycle_bin: HashMap<SlotId, Tape>,
}

impl Composer {
    pub fn new() -> Self {
        Composer {
            tape: Vec::new(),
            slot_depth: Vec::new(),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: HashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Composer {
            tape: Vec::with_capacity(capacity),
            slot_depth: Vec::with_capacity(capacity),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: HashMap::new(),
        }
    }

    #[track_caller]
    pub fn tag<F, T>(&mut self, key: usize, func: F) -> T
    where
        F: FnOnce(&mut Composer) -> T,
    {
        // set the key of first encountered group
        self.slot_key = Some(key);
        func(self)
    }

    #[track_caller]
    pub fn state<Node>(&mut self, val: Node) -> Node
    where
        Node: ComposeNode + Clone,
    {
        self.memo(|_| val, |_| true, |_| {}, |n| n.clone())
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
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        self.slot(
            factory,
            false,
            |_| {},
            false,
            |_, _| {},
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    pub fn group_use_children<F, Node, C, S, A, U, O, Output>(
        &mut self,
        factory: F,
        children: C,
        apply_children: A,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer),
        S: FnOnce(&mut Node) -> bool,
        A: FnOnce(&mut Node, Vec<&dyn ComposeNode>),
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        self.slot(
            factory,
            true,
            children,
            true,
            apply_children,
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    pub fn group<F, Node, C, S, U, O, Output>(
        &mut self,
        factory: F,
        children: C,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer),
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        self.slot(
            factory,
            true,
            children,
            false,
            |_, _| {},
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    #[allow(clippy::too_many_arguments)]
    pub fn slot<F, Node, C, S, A, U, O, Output>(
        &mut self,
        factory: F,
        has_children: bool,
        children: C,
        use_children: bool,
        apply_children: A,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer),
        S: FnOnce(&mut Node) -> bool,
        A: FnOnce(&mut Node, Vec<&dyn ComposeNode>),
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        // remember current cursor
        let cursor = self.forward_cursor();

        let curr_depth = self.depth;
        self.depth += 1;
        if let Some(d) = self.slot_depth.get_mut(cursor) {
            *d = curr_depth
        } else {
            self.slot_depth.insert(cursor, curr_depth);
        }

        // construct slot id
        let call_id = CallId::from(Location::caller());
        let key = self.slot_key.take();
        let slot_id = SlotId::new(call_id, key);

        // found in recycle_bin, restore it
        let slot_group = self.recycle_bin.remove(&slot_id);
        if let Some(group) = slot_group {
            let mut curr_idx = cursor;
            // TODO: use gap table?
            for slot in group {
                self.tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        let cached = self.tape.get_mut(cursor).map(|s| {
            // `Composer` required to guarantee `content` function not able to access current slot in self.tape
            // Otherwise need to wrap self.tape with RefCell to remove this unsafe but come with cost
            let ptr = s.data.as_mut().expect("slot data").as_mut() as *mut dyn ComposeNode;
            let data = unsafe { &mut *ptr };
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Some(node) = p_data.as_any_mut().downcast_mut::<Node>() {
                    trace!("{: >15} {} - {:?}", "get_cached", cursor, slot_id,);
                    if skip(node) {
                        trace!(
                            "{: >15} {} - {:?} - {}",
                            "skip_slot",
                            cursor,
                            slot_id,
                            p_size
                        );
                        self.skip_slot(cursor, p_size);
                    } else {
                        if has_children {
                            children(self);
                            if use_children {
                                let c = self.children_of_slot_at(cursor);
                                apply_children(node, c);
                            }
                        }
                        update(node);
                        self.end_slot_update(cursor);
                    }
                    return output(node);
                } else {
                    // NOTE:
                    // same slot_id can only return same type as before under same root fn
                    // However, this can happen when recompose with different root fn
                    trace!(
                        "{: >15} {} - {:?} - {:?}",
                        "downcast failed",
                        cursor,
                        slot_id,
                        p_data.type_id()
                    );
                }
            }
            // move previous cached slot to recycle bin
            self.recycle_slot(cursor, p_slot_id, p_size);
        }

        self.begin_slot(cursor, slot_id);
        let mut node = factory(self);

        if has_children {
            children(self);

            if use_children {
                let c = self.children_of_slot_at(cursor);
                apply_children(&mut node, c);
            }
        }

        let out = output(&node);
        // NOTE: expect node.into() will not change what node is
        let data = Box::new(node);
        self.end_slot(cursor, data);
        out
    }

    fn children_of_slot_at(&mut self, cursor: usize) -> Vec<&dyn ComposeNode> {
        let child_start = cursor + 1;
        let children = self.slot_depth[child_start..self.cursor]
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(i, v)| {
                if v == self.depth {
                    Some(child_start + i)
                } else {
                    None
                }
            })
            .filter_map(|c| {
                self.tape
                    .get(c)
                    .map(|s| s.data.as_ref().expect("slot data").as_ref())
            })
            .collect();
        children
    }

    fn begin_slot(&mut self, cursor: usize, slot_id: SlotId) {
        let slot = Slot::placeholder(slot_id);
        self.tape.insert(cursor, slot);
        trace!("{: >15} {} - {:?}", "begin_slot", cursor, slot_id);
    }

    fn end_slot(&mut self, cursor: usize, data: Box<dyn ComposeNode>) {
        self.depth -= 1;
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.data = Some(data);
            slot.size = curr_cursor - cursor;
            trace!("{: >15} {} - {:?}", "end_slot", cursor, slot.id);
        }
    }

    fn end_slot_update(&mut self, cursor: usize) {
        self.depth -= 1;
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.size = curr_cursor - cursor;
            trace!("{: >15} {} - {:?}", "end_slot_update", cursor, slot.id);
        }
    }

    fn skip_slot(&mut self, cursor: usize, size: usize) {
        self.depth -= 1;
        self.cursor = cursor + size;
    }

    #[inline]
    fn current_cursor(&mut self) -> usize {
        self.cursor
    }

    #[inline]
    fn forward_cursor(&mut self) -> usize {
        let cursor = self.current_cursor();
        self.cursor = cursor + 1;
        cursor
    }

    fn recycle_slot(&mut self, cursor: usize, slot_id: SlotId, size: usize) {
        let slots = self.tape.drain(cursor..cursor + size).collect();
        self.recycle_bin.insert(slot_id, slots);
    }
}
