use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

use generational_box::GenerationalBox;
use rustc_hash::FxHasher;

use crate::map::Map;
use crate::{ComposeNode, Composer, NodeKey, Scope, ScopeId};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SlotId(u64);

impl SlotId {
    #[inline(always)]
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<u64> for SlotId {
    fn from(value: u64) -> Self {
        SlotId::new(value)
    }
}

impl From<usize> for SlotId {
    fn from(value: usize) -> Self {
        SlotId::new(value as u64)
    }
}

impl From<&'static str> for SlotId {
    fn from(value: &'static str) -> Self {
        let mut hasher = FxHasher::default();
        value.hash(&mut hasher);
        SlotId::new(hasher.finish())
    }
}

#[derive(Default)]
pub(crate) struct SubcompositionEntry {
    pub slots: Map<SlotId, SlotRecord>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SlotRecord {
    pub scope_id: ScopeId,
    pub key: usize,
    pub node_key: Option<NodeKey>,
}

impl SlotRecord {
    #[track_caller]
    fn new(slot_id: SlotId) -> Self {
        let scope_id = ScopeId::new();
        Self {
            scope_id,
            key: slot_id.as_usize(),
            node_key: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SubcomposeHandle {
    node_key: NodeKey,
}

impl SubcomposeHandle {
    #[inline(always)]
    pub fn node_key(&self) -> NodeKey {
        self.node_key
    }
}

pub struct Subcomposition<N>
where
    N: ComposeNode,
{
    composer: GenerationalBox<Composer<N>>,
    node_key: NodeKey,
}

impl<N> Subcomposition<N>
where
    N: ComposeNode,
{
    pub(crate) fn new(node_key: NodeKey, composer: GenerationalBox<Composer<N>>) -> Self {
        {
            let mut c = composer.write();
            c.subcompositions.entry(node_key).or_default();
        }
        Self { composer, node_key }
    }

    #[inline(always)]
    pub fn registry(&mut self) -> SubcomposeRegistry<'_, N> {
        SubcomposeRegistry::new(self)
    }

    #[track_caller]
    pub fn subcompose<T, C, F>(&mut self, slot_id: SlotId, ctx: C, content: F) -> SubcomposeHandle
    where
        T: 'static,
        C: Clone + 'static,
        F: Fn(SubcomposeScope<T, N, C>) + Clone + 'static,
    {
        let (scope_id, slot_key) = self.ensure_slot(slot_id);
        let child_scope = Scope::new(scope_id, self.composer);
        let composer = self.composer;
        let ctx_clone = ctx.clone();
        let content_clone = content.clone();

        let composable = move || {
            let mut current_scope = child_scope;
            let mut skip = false;
            let (parent_node_key, current_node_key, is_dirty) = {
                let mut c = composer.write();
                let combined_key = combine_slot_key(slot_key, c.key_stack.last().copied());
                current_scope.set_key(combined_key);
                let parent_node_key = c.current_node_key;
                c.start_node(parent_node_key, current_scope.id);
                let current_node_key = c.current_node_key;
                let is_visited = c.composables.contains_key(&current_node_key);
                let is_dirty = c.dirty_nodes.contains(&current_node_key);
                if !is_dirty && is_visited {
                    c.skip_node(parent_node_key);
                    skip = true;
                }
                drop(c);
                (parent_node_key, current_node_key, is_dirty)
            };
            if skip {
                return current_node_key;
            }
            let scope = SubcomposeScope::new(current_scope, ctx_clone.clone());
            content_clone(scope);
            let mut c = composer.write();
            let c = c.deref_mut();
            if is_dirty {
                c.dirty_nodes.remove(&current_node_key);
            }
            c.end_node(parent_node_key);
            current_node_key
        };
        let node_key = composable();
        {
            let mut c = self.composer.write();
            c.composables.insert(node_key, Box::new(composable));
            if let Some(entry) = c.subcompositions.get_mut(&self.node_key) {
                if let Some(slot) = entry.slots.get_mut(&slot_id) {
                    slot.node_key = Some(node_key);
                }
            }
        }
        SubcomposeHandle { node_key }
    }

    #[track_caller]
    fn ensure_slot(&mut self, slot_id: SlotId) -> (ScopeId, usize) {
        let mut c = self.composer.write();
        let entry = c.subcompositions.entry(self.node_key).or_default();
        let slot_rec = SlotRecord::new(slot_id);
        let slot = entry.slots.entry(slot_id).or_insert(slot_rec);
        (slot.scope_id, slot.key)
    }
}

pub struct SubcomposeRegistry<'a, N>
where
    N: ComposeNode,
{
    host: &'a mut Subcomposition<N>,
}

impl<'a, N> SubcomposeRegistry<'a, N>
where
    N: ComposeNode,
{
    pub(crate) fn new(host: &'a mut Subcomposition<N>) -> Self {
        Self { host }
    }

    #[inline(always)]
    #[track_caller]
    pub fn subcompose<T, C, F>(&mut self, slot_id: SlotId, ctx: C, content: F) -> SubcomposeHandle
    where
        T: 'static,
        C: Clone + 'static,
        F: Fn(SubcomposeScope<T, N, C>) + Clone + 'static,
    {
        self.host.subcompose(slot_id, ctx, content)
    }
}

pub struct SubcomposeScope<S, N, C>
where
    N: ComposeNode,
{
    scope: Scope<S, N>,
    context: C,
}

impl<S, N, C> SubcomposeScope<S, N, C>
where
    N: ComposeNode,
{
    fn new(scope: Scope<S, N>, context: C) -> Self {
        Self { scope, context }
    }

    #[inline(always)]
    pub fn scope(&self) -> Scope<S, N> {
        self.scope
    }

    #[inline(always)]
    pub fn context(&self) -> &C {
        &self.context
    }

    #[inline(always)]
    pub fn into_parts(self) -> (Scope<S, N>, C) {
        (self.scope, self.context)
    }
}

impl<S, N, C> Deref for SubcomposeScope<S, N, C>
where
    N: ComposeNode,
{
    type Target = Scope<S, N>;

    fn deref(&self) -> &Self::Target {
        &self.scope
    }
}

#[inline(always)]
fn combine_slot_key(base: usize, parent: Option<usize>) -> usize {
    parent.map(|p| p ^ base).unwrap_or(base)
}
