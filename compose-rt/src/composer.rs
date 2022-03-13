use crate::{CallId, Slot, SlotId};
use log::trace;
use std::{any::Any, collections::HashMap, fmt::Debug, panic::Location, pin::Pin};

pub trait ComposeNode {
    fn cast_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static + Unpin + Debug;
}

#[derive(Debug)]
pub struct Composer<N: ?Sized> {
    tape: Vec<Slot<Pin<Box<N>>>>,
    slot_depth: Vec<usize>,
    depth: usize,
    cursor: usize,
    slot_key: Option<usize>,
    recycle_bin: HashMap<SlotId, Vec<Slot<Pin<Box<N>>>>>,
}

impl<N> Composer<N>
where
    N: 'static + ?Sized + Any + Unpin + Debug,
    for<'a> &'a mut N: ComposeNode,
{
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

impl<N> Composer<N>
where
    N: 'static + ?Sized + Any + Unpin + Debug,
    for<'a> &'a mut N: ComposeNode,
{
    pub fn finalize(mut self) -> Composer<N> {
        self.tape.truncate(self.cursor);
        self.slot_depth.truncate(self.cursor);
        self.cursor = 0;
        self.depth = 0;
        self.recycle_bin.clear();
        self
    }

    pub fn finalize_with<F>(mut self, func: F) -> Composer<N>
    where
        F: FnOnce(&mut Self),
    {
        func(&mut self);
        self
    }

    #[track_caller]
    pub fn tag<F>(&mut self, key: usize, func: F)
    where
        F: FnOnce(&mut Composer<N>),
    {
        // set the key of first encountered group
        self.slot_key = Some(key);
        func(self);
    }

    #[track_caller]
    pub fn memo<F, S, U, Node>(&mut self, factory: F, skip: S, update: U)
    where
        F: FnOnce(&mut Composer<N>) -> Node,
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        Node: Any + Debug + Unpin + Into<Box<N>>,
    {
        self.slot(factory, false, |_| {}, false, |_, _| {}, skip, update);
    }

    #[track_caller]
    pub fn group_use_children<F, C, S, A, U, Node>(
        &mut self,
        factory: F,
        children: C,
        apply_children: A,
        skip: S,
        update: U,
    ) where
        F: FnOnce(&mut Composer<N>) -> Node,
        C: FnOnce(&mut Composer<N>),
        S: FnOnce(&mut Node) -> bool,
        A: FnOnce(&mut Node, Vec<&N>),
        U: FnOnce(&mut Node),
        Node: Any + Debug + Unpin + Into<Box<N>>,
    {
        self.slot(factory, true, children, true, apply_children, skip, update);
    }

    #[track_caller]
    pub fn group<F, C, S, A, U, Node>(&mut self, factory: F, children: C, skip: S, update: U)
    where
        F: FnOnce(&mut Composer<N>) -> Node,
        C: FnOnce(&mut Composer<N>),
        S: FnOnce(&mut Node) -> bool,
        A: FnOnce(&mut Node, Vec<&N>),
        U: FnOnce(&mut Node),
        Node: Any + Debug + Unpin + Into<Box<N>>,
    {
        self.slot(factory, true, children, false, |_, _| {}, skip, update);
    }

    #[track_caller]
    pub fn slot<F, C, S, A, U, Node>(
        &mut self,
        factory: F,
        has_children: bool,
        children: C,
        use_children: bool,
        apply_children: A,
        skip: S,
        update: U,
    ) where
        F: FnOnce(&mut Composer<N>) -> Node,
        C: FnOnce(&mut Composer<N>),
        S: FnOnce(&mut Node) -> bool,
        A: FnOnce(&mut Node, Vec<&N>),
        U: FnOnce(&mut Node),
        Node: Any + Debug + Unpin + Into<Box<N>>,
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
            let ptr = s.data.as_mut().expect("slot data").as_mut().get_mut();
            let data = Box::leak(unsafe { Box::from_raw(ptr) });
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, mut p_data)) = cached {
            if slot_id == p_slot_id {
                if let Some(node) = p_data.cast_mut::<Node>() {
                    trace!(
                        "{: >15} {} - {:?} - {:?}",
                        "get_cached",
                        cursor,
                        slot_id,
                        node
                    );
                    if skip(node) {
                        trace!("{: >15} {} - {:?}", "skip_slot", cursor, slot_id);
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
                } else {
                    trace!(
                        "{: >15} {} - {:?} - {:?}",
                        "downcast failed",
                        cursor,
                        slot_id,
                        p_data.type_id()
                    );
                }
                return;
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

        let data = Pin::new(node.into());
        self.end_slot(cursor, data);
    }

    fn children_of_slot_at(&mut self, cursor: usize) -> Vec<&N> {
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
                    .map(|s| s.data.as_ref().expect("slot data").as_ref().get_ref())
            })
            .collect();
        children
    }

    fn begin_slot(&mut self, cursor: usize, slot_id: SlotId) {
        let slot = Slot::placeholder(slot_id);
        self.tape.insert(cursor, slot);
        trace!("{: >15} {} - {:?}", "begin_slot", cursor, slot_id);
    }

    fn end_slot(&mut self, cursor: usize, data: Pin<Box<N>>) {
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
        trace!(
            "{: >15} {} - {} - {}",
            "skip_slot",
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
