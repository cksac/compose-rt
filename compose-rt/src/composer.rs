use crate::{CallId, Slot, SlotId};
use downcast_rs::{impl_downcast, Downcast};
use log::trace;
use std::{
    any::{type_name, TypeId},
    collections::HashMap,
    panic::Location,
    sync::atomic::{AtomicUsize, Ordering},
};

pub trait ComposeNode: Downcast {}
impl_downcast!(ComposeNode);
impl<T: Downcast> ComposeNode for T {}

type Tape = Vec<Slot<Box<dyn ComposeNode>>>;

static COMPOSER_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Composer {
    pub(crate) id: usize,
    pub(crate) composing: bool,
    pub(crate) tape: Tape,
    pub(crate) slot_depth: Vec<usize>,
    pub(crate) depth: usize,
    pub(crate) cursor: usize,
    pub(crate) slot_key: Option<usize>,
    pub(crate) recycle_bin: HashMap<SlotId, Tape>,
    pub(crate) state_tape: Tape,
    pub(crate) state_cursor: usize,
}

impl Composer {
    pub(crate) fn new() -> Self {
        Composer {
            id: COMPOSER_ID.fetch_add(1, Ordering::SeqCst),
            composing: true,
            tape: Vec::new(),
            slot_depth: Vec::new(),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: HashMap::new(),
            state_tape: Vec::new(),
            state_cursor: 0,
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Composer {
            id: COMPOSER_ID.fetch_add(1, Ordering::SeqCst),
            composing: true,
            tape: Vec::with_capacity(capacity),
            slot_depth: Vec::with_capacity(capacity),
            depth: 0,
            cursor: 0,
            slot_key: None,
            recycle_bin: HashMap::new(),
            state_tape: Vec::new(),
            state_cursor: 0,
        }
    }

    #[track_caller]
    pub fn tag<F, T>(&mut self, key: usize, func: F) -> T
    where
        F: FnOnce(&mut Composer) -> T,
    {
        let id = self.id;

        // set the key of first encountered group
        self.slot_key = Some(key);

        let result = func(self);
        assert!(id == self.id && self.composing, "Composer changed");
        result
    }

    #[track_caller]
    pub fn state<Node>(&mut self, val: Node) -> Node
    where
        Node: ComposeNode + Clone,
    {
        self.taged_state(None, val)
    }

    #[track_caller]
    pub fn taged_state<Node>(&mut self, key: Option<usize>, val: Node) -> Node
    where
        Node: ComposeNode + Clone,
    {
        let state_cursor = self.state_cursor;
        self.state_cursor += 1;
        // construct slot id
        let call_id = CallId::from(Location::caller());
        let slot_id = SlotId::new(call_id, key);

        // found in recycle_bin, restore it
        let slot_group = self.recycle_bin.remove(&slot_id);
        if let Some(group) = slot_group {
            let mut curr_idx = state_cursor;
            // TODO: use gap table?
            for slot in group {
                self.state_tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        let cached = self.state_tape.get(state_cursor).map(|s| {
            let data = s.data.as_ref().expect("state slot").as_ref();
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Some(node) = p_data.as_any().downcast_ref::<Node>() {
                    trace!(
                        "{: >25} {} - {:?}",
                        "get_cached_state",
                        state_cursor,
                        slot_id,
                    );
                    return node.clone();
                }
            }
            // move previous cached slot to recycle bin
            trace!(
                "{: >25} {} - {:?} - {:?}",
                "recycle_state_slot",
                state_cursor,
                slot_id,
                p_data.type_id()
            );
            self.recycle_state_slot(state_cursor, p_slot_id, p_size);
        }

        let node = Box::new(val.clone());
        let slot: Slot<Box<dyn ComposeNode>> = Slot::new(slot_id, node);
        self.state_tape.insert(state_cursor, slot);
        trace!(
            "{: >25} {} - {:?} - {:?}",
            "insert_state",
            state_cursor,
            slot_id,
            self.state_tape.len()
        );
        val
    }

    #[track_caller]
    pub fn remember<Node>(&mut self, val: Node) -> Node
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
        self.group(factory, |_| {}, skip, update, output)
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
            children,
            |_, _| {},
            false,
            |_, _| {},
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    pub fn group_apply_children<F, Node, C, Children, AC, S, U, O, Output>(
        &mut self,
        factory: F,
        children: C,
        apply_children: AC,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer) -> Children,
        AC: FnOnce(&mut Node, Children),
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        self.slot(
            factory,
            children,
            apply_children,
            false,
            |_, _| {},
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    pub fn group_use_children<F, Node, C, UC, S, U, O, Output>(
        &mut self,
        factory: F,
        children: C,
        use_children: UC,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer),
        UC: FnOnce(&mut Node, Vec<&dyn ComposeNode>),
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        self.slot(
            factory,
            children,
            |_, _| {},
            true,
            use_children,
            skip,
            update,
            output,
        )
    }

    #[track_caller]
    #[allow(clippy::too_many_arguments)]
    pub fn slot<F, Node, C, Children, AC, UC, S, U, O, Output>(
        &mut self,
        factory: F,
        children: C,
        apply_children: AC,
        require_use_children: bool,
        use_children: UC,
        skip: S,
        update: U,
        output: O,
    ) -> Output
    where
        F: FnOnce(&mut Composer) -> Node,
        Node: ComposeNode,
        C: FnOnce(&mut Composer) -> Children,
        AC: FnOnce(&mut Node, Children),
        UC: FnOnce(&mut Node, Vec<&dyn ComposeNode>),
        S: FnOnce(&mut Node) -> bool,
        U: FnOnce(&mut Node),
        O: FnOnce(&Node) -> Output,
    {
        let id = self.id;

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
                    trace!(
                        "C{: >6}:{}{} | {:?} | {:?}",
                        cursor,
                        "  ".repeat(self.depth),
                        type_name::<Node>(),
                        TypeId::of::<Node>(),
                        slot_id
                    );
                    if skip(node) {
                        trace!(
                            "S{: >6}:{}{} | {:?} | {:?}",
                            cursor,
                            "  ".repeat(self.depth),
                            type_name::<Node>(),
                            TypeId::of::<Node>(),
                            slot_id
                        );
                        self.skip_slot(cursor, p_size);
                    } else {
                        let c = children(self);
                        assert!(id == self.id && self.composing, "Composer changed");

                        apply_children(node, c);
                        if require_use_children {
                            let cn = self.children_of_slot_at(cursor);
                            use_children(node, cn);
                        }
                        update(node);

                        trace!(
                            "U{: >6}:{}{} | {:?} | {:?}",
                            cursor,
                            "  ".repeat(self.depth),
                            type_name::<Node>(),
                            TypeId::of::<Node>(),
                            slot_id
                        );
                        self.end_slot_update(cursor);
                    }
                    return output(node);
                }
            }
            // move previous cached slot to recycle bin
            self.recycle_slot(cursor, p_slot_id, p_size);
            trace!(
                "-{: >6}:{}{:?} | {:?} | {:?}",
                cursor,
                "  ".repeat(self.depth),
                p_data.type_id(),
                slot_id,
                p_size
            );
        }

        trace!(
            "+{: >6}:{}{} | {:?} | {:?}",
            cursor,
            "  ".repeat(self.depth),
            type_name::<Node>(),
            TypeId::of::<Node>(),
            slot_id
        );
        let slot = Slot::placeholder(slot_id);
        self.tape.insert(cursor, slot);

        let mut node = factory(self);
        assert!(id == self.id && self.composing, "Composer changed");

        let c = children(self);
        assert!(id == self.id && self.composing, "Composer changed");

        apply_children(&mut node, c);
        if require_use_children {
            let cn = self.children_of_slot_at(cursor);
            use_children(&mut node, cn);
        }

        let out = output(&node);
        let data = Box::new(node);

        let new_cursor = self.cursor;
        let slot = self.tape.get_mut(cursor).expect("slot");
        slot.data = Some(data);
        slot.size = new_cursor - cursor;
        // TODO: can skip check when not in trace?
        if slot.size > 1 {
            trace!(
                "+{: >6}:{}{} | {:?} | {:?}",
                cursor,
                "  ".repeat(self.depth),
                type_name::<Node>(),
                TypeId::of::<Node>(),
                slot_id
            );
        }
        self.depth -= 1;

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

    #[inline]
    fn end_slot_update(&mut self, cursor: usize) {
        self.depth -= 1;
        if let Some(slot) = self.tape.get_mut(self.cursor) {
            slot.size = self.cursor - cursor;
        }
    }

    #[inline]
    fn skip_slot(&mut self, cursor: usize, size: usize) {
        self.depth -= 1;
        self.cursor = cursor + size;
    }

    #[inline]
    fn forward_cursor(&mut self) -> usize {
        let cursor = self.cursor;
        self.cursor = cursor + 1;
        cursor
    }

    fn recycle_slot(&mut self, cursor: usize, slot_id: SlotId, size: usize) {
        let slot_end = cursor + size;
        let slots = self.tape.drain(cursor..slot_end).collect();
        self.recycle_bin.insert(slot_id, slots);
    }

    fn recycle_state_slot(&mut self, cursor: usize, slot_id: SlotId, size: usize) {
        let slots = self.state_tape.drain(cursor..cursor + size).collect();
        self.recycle_bin.insert(slot_id, slots);
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}
