use crate::{CallId, Data, Slot, SlotId};
use log::trace;
use std::{cell::RefCell, collections::HashMap, fmt::Debug, panic::Location, rc::Rc};

#[derive(Debug)]
pub struct Composer {
    tape: RefCell<Vec<Slot>>,
    cursor: RefCell<usize>,
    slot_key: RefCell<Option<usize>>,
    recycle_bin: RefCell<HashMap<SlotId, Vec<Slot>>>,
}

impl Composer {
    pub fn new(capacity: usize) -> Self {
        Composer {
            tape: RefCell::new(Vec::with_capacity(capacity)),
            cursor: RefCell::new(0),
            slot_key: RefCell::new(None),
            recycle_bin: RefCell::new(HashMap::new()),
        }
    }
}

impl Composer {
    pub fn reset_cursor(&self) {
        *self.cursor.borrow_mut() = 0;
    }

    #[track_caller]
    pub fn tag<F, T>(&self, key: usize, func: F) -> T
    where
        F: FnOnce() -> T,
    {
        // set the key of first encountered group
        *self.slot_key.borrow_mut() = Some(key);
        func()
    }

    #[track_caller]
    pub fn group<N, C, S, U, Node>(&self, factory: N, content: C, skip: S, update: U) -> Rc<Node>
    where
        N: FnOnce() -> Node,
        C: FnOnce(),
        S: FnOnce(Rc<Node>) -> bool,
        U: FnOnce(Rc<Node>),
        Node: 'static + Debug,
    {
        let cursor = self.forward_cursor();
        let call_id = CallId::from(Location::caller());
        let key = self.slot_key.borrow_mut().take();
        let slot_id = SlotId::new(call_id, key);

        // found in recycle_bin, restore it
        let slot_group = self.recycle_bin.borrow_mut().remove(&slot_id);
        if let Some(group) = slot_group {
            let mut tape = self.tape.borrow_mut();
            let mut curr_idx = cursor;

            // TODO: use gap table?
            for slot in group {
                tape.insert(curr_idx, slot);
                curr_idx += 1;
            }
        }

        let cached: Option<(SlotId, usize, Rc<dyn Data>)> = self
            .tape
            .borrow()
            .get(cursor)
            .map(|s| (s.id, s.size, s.data.clone()));

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Ok(node) = p_data.downcast_rc::<Node>() {
                    trace!("{} - get cached {:?}", cursor, node);
                    if !skip(node.clone()) {
                        content();
                        update(node.clone());
                        self.end_group(cursor, slot_id, node.clone());
                    }
                    return node;
                }
            }

            // move previous cached slot to recycle bin
            self.recycle_slot(cursor, p_slot_id, p_size);
        }

        self.begin_group(cursor, slot_id);
        let node = Rc::new(factory());
        content();
        self.end_group(cursor, slot_id, node.clone());

        node
    }

    fn begin_group(&self, cursor: usize, slot_id: SlotId) {
        trace!("{} - group begin {:?}", cursor, slot_id);
        let slot = Slot::placeholder(slot_id);
        self.tape.borrow_mut().insert(cursor, slot);
    }

    fn end_group(&self, cursor: usize, slot_id: SlotId, data: Rc<dyn Data>) {
        trace!("{} - group end   {:?} {:?}", cursor, slot_id, data);
        if let Some(slot) = self.tape.borrow_mut().get_mut(cursor) {
            slot.data = data;
            slot.size = self.current_cursor() - cursor;
        }
    }

    #[inline]
    fn current_cursor(&self) -> usize {
        *self.cursor.borrow()
    }

    #[inline]
    fn forward_cursor(&self) -> usize {
        let cursor = self.current_cursor();
        *self.cursor.borrow_mut() = cursor + 1;
        cursor
    }

    fn recycle_slot(&self, cursor: usize, slot_id: SlotId, size: usize) {
        let slots = self
            .tape
            .borrow_mut()
            .drain(cursor..cursor + size)
            .collect();
        self.recycle_bin.borrow_mut().insert(slot_id, slots);
    }
}
