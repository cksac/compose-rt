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
        if let Some(slots) = self.recycle_bin.remove(&curr_slot_id) {
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
            self.recycle_bin.insert(p_slot_id, slots);
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
        self.memo(|_| factory(), |_| true, |_| {}, |n| n.clone())
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
        UC: FnOnce(&mut Node, ChildrenView),
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
        UC: FnOnce(&mut Node, ChildrenView),
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
        if let Some(slots) = self.recycle_bin.remove(&curr_slot_id) {
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
            .get_mut(curr_cursor)
            .map(|slot| {
                if curr_slot_id == slot.id {
                    if let Some(node) = slot.data.as_mut().cast_mut::<Node>() {
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
            self.recycle_bin.insert(p_slot_id, slots);
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
        let c = children(self);
        assert!(
            id == self.id && self.composing && self.tape.len() > curr_cursor,
            "Composer is in inconsistent state"
        );

        let child_start = curr_cursor + 1;
        let (head_slots, tail_slots) = self.tape.split_at_mut(child_start);

        let slot = head_slots.last_mut().expect("slot");
        let node = slot
            .data
            .as_mut()
            .cast_mut::<Node>()
            .expect("downcast node");

        // update new slot size after children fn done
        slot.size = self.cursor - curr_cursor;

        apply_children(node, c);
        if require_use_children {
            let children_view = ChildrenView {
                tape: &mut tail_slots[..slot.size - 1],
                slot_depth: &self.slot_depth[child_start..self.cursor],
                depth: curr_depth + 1,
            };
            use_children(node, children_view);
        }
        update(node);

        if log_enabled!(Trace) && slot.size > 1 {
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

pub struct ChildrenView<'a> {
    pub(crate) tape: &'a [Slot<Box<dyn ComposeNode>>],
    pub(crate) slot_depth: &'a [usize],
    pub(crate) depth: usize,
}

impl<'a> ChildrenView<'a> {
    pub fn iter(&self) -> impl Iterator<Item = &dyn ComposeNode> {
        self.slot_depth
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(i, v)| if v == self.depth { Some(i) } else { None })
            .filter_map(|c| self.tape.get(c).map(|s| s.data.as_ref()))
    }

    pub fn with_ty<Node: 'static>(&self) -> impl Iterator<Item = &Node> {
        self.iter().filter_map(|c| c.cast_ref::<Node>())
    }
}
