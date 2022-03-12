use downcast_rs::{impl_downcast, Downcast};
use std::{
    fmt::{self, Debug},
    hash::Hash,
    panic::Location,
    pin::Pin,
};

pub trait Data: Debug + Downcast + Unpin {}
impl_downcast!(Data);

impl<T: 'static + Debug + Unpin> Data for T {}

#[derive(Debug)]
struct PlaceHolder;

#[derive(Clone, Copy)]
pub struct CallId {
    pub(crate) loc: &'static Location<'static>,
}

impl CallId {
    fn loc_ptr(&self) -> *const Location<'static> {
        self.loc
    }
}

impl From<&'static Location<'static>> for CallId {
    fn from(loc: &'static Location<'static>) -> Self {
        CallId { loc }
    }
}

impl Debug for CallId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!(
            "{}:{}:{}",
            self.loc.file(),
            self.loc.line(),
            self.loc.column()
        ))
    }
}

impl PartialEq for CallId {
    fn eq(&self, other: &CallId) -> bool {
        self.loc_ptr() == other.loc_ptr()
    }
}

impl Eq for CallId {}

impl Hash for CallId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.loc_ptr().hash(state)
    }
}

impl PartialOrd for CallId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.loc_ptr().partial_cmp(&other.loc_ptr())
    }
}

impl Ord for CallId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc_ptr().cmp(&other.loc_ptr())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct SlotId {
    pub call_id: CallId,
    pub key: Option<usize>,
}

impl SlotId {
    pub fn new(call_id: impl Into<CallId>, key: impl Into<Option<usize>>) -> Self {
        Self {
            call_id: call_id.into(),
            key: key.into(),
        }
    }
}

impl From<&'static Location<'static>> for SlotId {
    fn from(loc: &'static Location<'static>) -> Self {
        SlotId::new(loc, None)
    }
}

impl Debug for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            match self.key {
                Some(key) => f.write_fmt(format_args!("{:#?} |{}", self.call_id, key)),
                None => f.write_fmt(format_args!("{:#?} |*", self.call_id)),
            }
        } else {
            match self.key {
                Some(key) => f.write_fmt(format_args!("{:?} |{}", self.call_id, key)),
                None => f.write_fmt(format_args!("{:?} |*", self.call_id)),
            }
        }
    }
}

pub struct Slot {
    pub id: SlotId,
    pub size: usize,
    pub data: Pin<Box<dyn Data>>,
}

impl Slot {
    pub fn new<T>(call_id: impl Into<CallId>, data: Box<T>) -> Self
    where
        T: 'static + Data,
    {
        Slot {
            id: SlotId::new(call_id, None),
            data: Box::pin(data),
            size: 1,
        }
    }

    pub fn placeholder(slot_id: SlotId) -> Self {
        Slot {
            id: slot_id,
            data: Box::pin(PlaceHolder),
            size: 1,
        }
    }
}

impl Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Slot");
        s.field("id", &self.id);
        s.field("data", &self.data);
        if self.size > 1 {
            s.field("size", &self.size);
        }
        s.finish()
    }
}
