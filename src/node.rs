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

    #[inline(always)]
    fn new(scope_id: ScopeId, parent: NodeKey) -> Self {
        Self {
            scope_id,
            parent,
            children: Vec::new(),
            data: None,
        }
    }

    #[inline(always)]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline(always)]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[inline(always)]
    fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    #[inline(always)]
    fn set_scope_id(&mut self, scope_id: ScopeId) {
        self.scope_id = scope_id;
    }

    #[inline(always)]
    fn parent(&self) -> NodeKey {
        self.parent
    }

    #[inline(always)]
    fn set_parent(&mut self, parent: NodeKey) {
        self.parent = parent;
    }

    #[inline(always)]
    fn data(&self) -> Option<&Self::Data> {
        self.data.as_ref()
    }

    #[inline(always)]
    fn data_mut(&mut self) -> Option<&mut Self::Data> {
        self.data.as_mut()
    }

    #[inline(always)]
    fn set_data(&mut self, data: Self::Data) {
        self.data = Some(data);
    }

    #[inline(always)]
    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    #[inline(always)]
    fn children_mut(&mut self) -> &mut [NodeKey] {
        &mut self.children
    }

    #[inline(always)]
    fn children_push(&mut self, node_key: NodeKey) {
        self.children.push(node_key);
    }

    #[inline(always)]
    fn children_len(&self) -> usize {
        self.children.len()
    }

    #[inline(always)]
    fn children_drain<R>(&mut self, range: R) -> Drain<NodeKey>
    where
        R: RangeBounds<usize>,
    {
        self.children.drain(range)
    }
}
