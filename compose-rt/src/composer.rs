use crate::{CallId, Data, Slot, SlotId};
use log::trace;
use std::{collections::HashMap, fmt::Debug, panic::Location, pin::Pin};

#[derive(Debug)]
pub struct Composer {
    tape: Vec<Slot>,
    slot_depth: Vec<usize>,
    depth: usize,
    cursor: usize,
    slot_key: Option<usize>,
    recycle_bin: HashMap<SlotId, Vec<Slot>>,
}

impl Composer {
    pub fn new(capacity: usize) -> Self {
        Composer {
            tape: Vec::with_capacity(capacity),
            slot_depth: Vec::with_capacity(capacity),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: HashMap::new(),
        }
    }
}

impl Composer {
    pub fn finalize(&mut self) {
        self.cursor = 0;
        // TODO: clear recycle bin?
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
    pub fn group<N, C, S, A, U, Node>(
        &mut self,
        factory: N,
        content: C,
        apply: A,
        skip: S,
        update: U,
    ) where
        N: FnOnce(&mut Composer) -> Node,
        C: FnOnce(&mut Composer),
        S: FnOnce(&Node) -> bool,
        A: FnOnce(&mut Node, Vec<&dyn Data>),
        U: FnOnce(&mut Node),
        Node: 'static + Debug + Unpin,
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
            let ptr = s.data.as_mut().get_mut() as *mut dyn Data;
            // `Composer` required to guarantee `content` function not able to access current slot in self.tape
            // Otherwise need to wrap self.tape with RefCell to remove this unsafe but come with cost
            let data = Box::leak(unsafe { Box::from_raw(ptr) });
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Some(node) = p_data.downcast_mut::<Node>() {
                    trace!(
                        "{: >15} {} - {:?} - {:?}",
                        "get_cached",
                        cursor,
                        slot_id,
                        node
                    );
                    if skip(node) {
                        self.skip_group(cursor, p_size);
                    } else {
                        content(self);
                        let children = self.children_of_slot_at(cursor);
                        apply(node, children);
                        update(node);
                        self.end_update(cursor);
                    }
                    return;
                }
            }
            // move previous cached slot to recycle bin
            self.recycle_slot(cursor, p_slot_id, p_size);
        }

        self.begin_group(cursor, slot_id);
        let mut node = Box::pin(factory(self));
        content(self);
        let children = self.children_of_slot_at(cursor);
        let node_mut = node.as_mut().get_mut();
        apply(node_mut, children);
        self.end_group(cursor, node);
    }

    fn children_of_slot_at(&mut self, cursor: usize) -> Vec<&dyn Data> {
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
            .filter_map(|c| self.tape.get(c).map(|s| &*s.data))
            .collect();
        children
    }

    fn begin_group(&mut self, cursor: usize, slot_id: SlotId) {
        let slot = Slot::placeholder(slot_id);
        self.tape.insert(cursor, slot);
        trace!("{: >15} {} - {:?}", "begin_group", cursor, slot_id);
    }

    fn end_group(&mut self, cursor: usize, data: Pin<Box<dyn Data>>) {
        self.depth -= 1;
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.data = data;
            slot.size = curr_cursor - cursor;
            trace!(
                "{: >15} {} - {:?} - {:?}",
                "end_group",
                cursor,
                slot.id,
                slot.data
            );
        }
    }

    fn end_update(&mut self, cursor: usize) {
        self.depth -= 1;
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.size = curr_cursor - cursor;
            trace!("{: >15} {} - {:?}", "end_update", cursor, slot.id);
        }
    }

    fn skip_group(&mut self, cursor: usize, size: usize) {
        self.depth -= 1;
        trace!(
            "{: >15} {} - {} - {}",
            "skip_group",
            cursor,
            size,
            self.cursor
        );
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
