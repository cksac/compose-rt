use crate::{CallId, Slot, SlotId};
use downcast_rs::{impl_downcast, Downcast};
use log::Level::Trace;
use log::{log_enabled, trace};
use std::{
    any::{type_name, TypeId},
    collections::HashMap,
    fmt::Debug,
    panic::Location,
    sync::atomic::{AtomicUsize, Ordering},
};

pub trait ComposeNode: Debug + Downcast {}
impl_downcast!(ComposeNode);
impl<T: Debug + Downcast> ComposeNode for T {}

type Tape = Vec<Slot<Box<dyn ComposeNode>>>;

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
    pub fn state<Node>(&mut self, val: Node) -> Node
    where
        Node: ComposeNode + Clone,
    {
        self.state_with(None, val)
    }

    #[track_caller]
    pub fn state_with<Node>(&mut self, key: Option<usize>, val: Node) -> Node
    where
        Node: ComposeNode + Clone,
    {
        // save current states
        let curr_cursor = self.state_cursor;

        // forward cursors
        self.state_cursor += 1;

        // construct slot id
        let call_id = CallId::from(Location::caller());
        let slot_id = SlotId::new(call_id, key);

        // found in recycle_bin, restore it to current cursor
        let slot_group = self.recycle_bin.remove(&slot_id);
        if let Some(group) = slot_group {
            let mut curr_idx = curr_cursor;
            // TODO: use gap table?
            for slot in group {
                self.state_tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        let cached = self.state_tape.get(curr_cursor).map(|s| {
            let data = s.data.as_ref().expect("state slot").as_ref();
            (s.id, s.size, data)
        });

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Some(node) = p_data.as_any().downcast_ref::<Node>() {
                    if log_enabled!(Trace) {
                        trace!("{: >15} {} - {:?}", "get_state", curr_cursor, slot_id);
                    }
                    return node.clone();
                }
            }
            // move previous cached slot to recycle bin
            if log_enabled!(Trace) {
                trace!(
                    "{: >15} {} - {:?} - {:?}",
                    "recycle_state",
                    curr_cursor,
                    p_slot_id,
                    p_data.type_id()
                );
            }
            let slot_end = curr_cursor + p_size;
            let slots = self.state_tape.drain(curr_cursor..slot_end).collect();
            self.recycle_bin.insert(p_slot_id, slots);
        }

        let node = Box::new(val.clone());
        let slot: Slot<Box<dyn ComposeNode>> = Slot::new(slot_id, node);
        if log_enabled!(Trace) {
            trace!(
                "{: >15} {} - {:?} - {:?}",
                "insert_state",
                curr_cursor,
                slot_id,
                self.state_tape.len()
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

        // set the key of first encountered group
        self.slot_key = Some(key);

        let result = func(self);
        assert!(id == self.id && self.composing, "Composer changed");
        result
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

        // found in recycle_bin, restore it to curr_cursor
        let slot_group = self.recycle_bin.remove(&curr_slot_id);
        if let Some(group) = slot_group {
            let mut curr_idx = curr_cursor;
            // TODO: use gap table?
            for slot in group {
                self.tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        // use slot data in current cursor if slot id and type match
        let cached = self.tape.get_mut(curr_cursor).map(|slot| {
            // `Composer` required to guarantee `content` function not able to access current slot in self.tape
            let slot_ptr = slot as *mut Slot<Box<dyn ComposeNode>>;
            unsafe { &mut *slot_ptr }
        });

        if let Some(slot) = cached {
            // slot id match
            if curr_slot_id == slot.id {
                // node type match
                let slot_data = slot.data.as_mut().expect("slot data").as_mut();
                if let Some(node) = slot_data.as_any_mut().downcast_mut::<Node>() {
                    // use slot
                    if log_enabled!(Trace) {
                        trace!(
                            "C{: >6}:{}{} | {:?} | {:?}",
                            curr_cursor,
                            "  ".repeat(self.depth),
                            type_name::<Node>(),
                            TypeId::of::<Node>(),
                            curr_slot_id
                        );
                    }
                    if skip(node) {
                        if log_enabled!(Trace) {
                            trace!(
                                "S{: >6}:{}{} | {:?} | {:?}",
                                curr_cursor,
                                "  ".repeat(self.depth),
                                type_name::<Node>(),
                                TypeId::of::<Node>(),
                                curr_slot_id
                            );
                        }
                        // skip to slot end
                        self.cursor = curr_cursor + slot.size;
                    } else {
                        // rerun
                        let c = children(self);
                        assert!(id == self.id && self.composing, "Composer changed");

                        apply_children(node, c);
                        if require_use_children {
                            let cn = self.children_of_slot_at(curr_cursor);
                            use_children(node, cn);
                        }
                        update(node);
                        if log_enabled!(Trace) {
                            trace!(
                                "U{: >6}:{}{} | {:?} | {:?}",
                                curr_cursor,
                                "  ".repeat(self.depth),
                                type_name::<Node>(),
                                TypeId::of::<Node>(),
                                curr_slot_id
                            );
                        }
                        // update new slot size after children fn done
                        slot.size = self.cursor - curr_cursor;
                    }

                    self.depth -= 1;
                    return output(node);
                }
            }
            // if curr_slot_id != cached slot id and type mismatch
            // move cached slot to recycle bin
            if log_enabled!(Trace) {
                trace!(
                    "-{: >6}:{}{:?} | {:?} | {:?}",
                    curr_cursor,
                    "  ".repeat(self.depth),
                    slot.data.as_ref().expect("slot data").type_id(),
                    slot.id,
                    slot.size
                );
            }
            let slot_end = curr_cursor + slot.size;
            let slots = self.tape.drain(curr_cursor..slot_end).collect();
            self.recycle_bin.insert(slot.id, slots);
        }

        // if new or cache miss, insert slot
        if log_enabled!(Trace) {
            trace!(
                "+{: >6}:{}{} | {:?} | {:?}",
                curr_cursor,
                "  ".repeat(self.depth),
                type_name::<Node>(),
                TypeId::of::<Node>(),
                curr_slot_id
            );
        }
        let slot = Slot::placeholder(curr_slot_id);
        self.tape.insert(curr_cursor, slot);

        let mut node = factory(self);
        assert!(id == self.id && self.composing, "Composer changed");

        let c = children(self);
        assert!(id == self.id && self.composing, "Composer changed");

        apply_children(&mut node, c);
        if require_use_children {
            let cn = self.children_of_slot_at(curr_cursor);
            use_children(&mut node, cn);
        }

        let out = output(&node);
        let data = Box::new(node);

        let slot = self.tape.get_mut(curr_cursor).expect("slot");
        slot.data = Some(data);
        slot.size = self.cursor - curr_cursor;

        if log_enabled!(Trace) {
            if slot.size > 1 {
                trace!(
                    "+{: >6}:{}{} | {:?} | {:?}",
                    curr_cursor,
                    "  ".repeat(self.depth),
                    type_name::<Node>(),
                    TypeId::of::<Node>(),
                    curr_slot_id
                );
            }
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
}
