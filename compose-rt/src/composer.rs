use crate::{CallId, Data, Slot, SlotId};
use log::trace;
use std::{collections::HashMap, fmt::Debug, panic::Location, rc::Rc};

#[derive(Debug)]
pub struct Composer {
    tape: Vec<Slot>,
    cursor: usize,
    slot_key: Option<usize>,
    recycle_bin: HashMap<SlotId, Vec<Slot>>,
}

impl Composer {
    pub fn new(capacity: usize) -> Self {
        Composer {
            tape: Vec::with_capacity(capacity),
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
    pub fn group<N, C, S, U, Node>(&mut self, factory: N, content: C, skip: S, update: U)
    where
        N: FnOnce(&mut Composer) -> Node,
        C: FnOnce(&mut Composer),
        S: FnOnce(Rc<Node>) -> bool,
        U: FnOnce(Rc<Node>),
        Node: 'static + Debug,
    {
        let cursor = self.forward_cursor();
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

        let cached: Option<(SlotId, usize, Rc<dyn Data>)> = self
            .tape
            .get(cursor)
            .map(|s| (s.id, s.size, s.data.clone()));

        if let Some((p_slot_id, p_size, p_data)) = cached {
            if slot_id == p_slot_id {
                if let Ok(node) = p_data.downcast_rc::<Node>() {
                    trace!("{: >15} {} - {:?} - {:?}", "get_cached", cursor, slot_id, node);
                    if skip(node.clone()) {
                        self.skip_group(cursor, p_size);
                    } else {
                        content(self);
                        update(node.clone());
                        self.end_update(cursor);                       
                    }
                    return;
                }
            }
            // move previous cached slot to recycle bin
            self.recycle_slot(cursor, p_slot_id, p_size);
        } 

        self.begin_group(cursor, slot_id);
        let node = Rc::new(factory(self));
        content(self);
        self.end_group(cursor, node);      
    }

    fn begin_group(&mut self, cursor: usize, slot_id: SlotId) {        
        let slot =Slot::placeholder(slot_id);
        self.tape.insert(cursor, slot);
        trace!("{: >15} {} - {:?}", "begin_group", cursor, slot_id);
    }

    fn end_group(&mut self, cursor: usize, data: Rc<dyn Data>) {
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.data = data;
            slot.size = curr_cursor - cursor;
            trace!("{: >15} {} - {:?} - {:?}", "end_group", cursor, slot.id, slot.data);
        }
    }

    fn end_update(&mut self, cursor: usize) {
        let curr_cursor = self.current_cursor();
        if let Some(slot) = self.tape.get_mut(cursor) {
            slot.size = curr_cursor - cursor;
            trace!("{: >15} {} - {:?}", "end_update", cursor, slot.id);
        }
    }

    fn skip_group(&mut self, cursor: usize, size: usize) {
        trace!("{: >15} {} - {} - {}", "skip_group", cursor, size, self.cursor);        
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
