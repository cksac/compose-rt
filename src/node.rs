use std::any::Any;
use std::fmt::Debug;
use std::ops::RangeBounds;
use std::vec::Drain;

use crate::{ComposeNode, NodeKey, ScopeId};

pub trait NodeData: Debug + 'static {
    type Context;
}

impl NodeData for () {
    type Context = ();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node<T> {
    pub scope_id: ScopeId,
    pub parent: NodeKey,
    pub children: Vec<NodeKey>,
    pub data: Option<T>,
}

impl<T> ComposeNode for Node<T>
where
    T: NodeData,
{
    type Context = T::Context;

    type Data = T;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn new(scope_id: ScopeId, parent: NodeKey) -> Self {
        Self {
            scope_id,
            parent,
            children: Vec::new(),
            data: None,
        }
    }

    fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    fn parent(&self) -> NodeKey {
        self.parent
    }

    fn data(&self) -> Option<&Self::Data> {
        self.data.as_ref()
    }

    fn data_mut(&mut self) -> Option<&mut Self::Data> {
        self.data.as_mut()
    }

    fn set_data(&mut self, data: Self::Data) {
        self.data = Some(data);
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn children_push(&mut self, node_key: NodeKey) {
        self.children.push(node_key);
    }

    fn children_len(&self) -> usize {
        self.children.len()
    }

    fn children_drain<R>(&mut self, range: R) -> Drain<NodeKey>
    where
        R: RangeBounds<usize>,
    {
        self.children.drain(range)
    }

    fn children_get(&self, index: usize) -> Option<NodeKey> {
        self.children.get(index).copied()
    }

    fn children_set(&mut self, index: usize, node_key: NodeKey) {
        self.children[index] = node_key;
    }
}
