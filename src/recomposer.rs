use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};

use generational_box::{GenerationalBox, Owner};

use crate::{utils, ComposeNode, Composer, NodeKey, State};

pub struct Recomposer<S, N>
where
    N: ComposeNode,
{
    #[allow(dead_code)]
    pub(crate) owner: Owner,
    pub(crate) composer: GenerationalBox<Composer<N>>,
    pub(crate) root_state: State<S, N>,
}

impl<S, N> Recomposer<S, N>
where
    S: 'static,
    N: ComposeNode,
{
    pub fn recompose(&mut self) {
        let mut c = self.composer.write();
        c.dirty_nodes.clear();
        for state_id in c.dirty_states.drain().collect::<Vec<_>>() {
            if let Some(nodes) = c.used_by.get(&state_id).cloned() {
                c.dirty_nodes.extend(nodes);
            }
        }
        let mut composables = Vec::with_capacity(c.dirty_nodes.len());
        for node_key in &c.dirty_nodes {
            if let Some(composable) = c.composables.get(node_key).cloned() {
                composables.push((*node_key, composable));
            }
        }
        drop(c);
        for (node_key, composable) in composables {
            {
                let mut c = self.composer.write();
                c.current_node_key = node_key;
            }
            composable.compose();
        }
        let mut c = self.composer.write();
        let c = c.deref_mut();
        let unmount_nodes = c
            .unmount_nodes
            .difference(&c.mount_nodes)
            .cloned()
            .collect::<Vec<_>>();
        for n in unmount_nodes {
            c.composables.remove(&n);
            c.nodes.remove(n);
            if let Some(node_states) = c.states.remove(&n) {
                for state in node_states.keys() {
                    c.used_by.remove(state);
                }
            }
            let use_states = c.uses.remove(&n);
            if let Some(use_states) = use_states {
                for state in use_states {
                    if let Some(used_by) = c.used_by.get_mut(&state) {
                        used_by.remove(&n);
                    }
                }
            }
        }
        c.mount_nodes.clear();
        c.unmount_nodes.clear();
    }

    #[inline(always)]
    pub fn recompose_with(&mut self, new_state: S) {
        self.root_state.set(new_state);
        self.recompose();
    }

    #[inline(always)]
    pub fn root_node_key(&self) -> NodeKey {
        self.composer.read().root_node_key
    }

    #[inline(always)]
    pub fn with_context<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&N::Context) -> T,
    {
        let c = self.composer.read();
        func(&c.context)
    }

    #[inline(always)]
    pub fn with_context_mut<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut N::Context) -> T,
    {
        let mut c = self.composer.write();
        func(&mut c.context)
    }

    #[inline(always)]
    pub fn with_composer<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&Composer<N>) -> T,
    {
        let c = self.composer.read();
        func(c.deref())
    }

    #[inline(always)]
    pub fn with_composer_mut<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(&mut Composer<N>) -> T,
    {
        let mut c = self.composer.write();
        func(c.deref_mut())
    }

    #[inline(always)]
    pub fn get_root_state(&self) -> S
    where
        S: Clone,
    {
        self.root_state.get_untracked()
    }

    #[inline(always)]
    pub fn set_root_state(&mut self, val: S) {
        self.root_state.set(val);
    }

    #[inline(always)]
    pub fn print_tree(&self)
    where
        N: Debug,
    {
        self.print_tree_with(self.root_node_key(), |n| format!("{:?}", n));
    }

    #[inline(always)]
    pub fn print_tree_with<D>(&self, node_key: NodeKey, display_fn: D)
    where
        D: Fn(Option<&N>) -> String,
    {
        let c = self.composer.read();
        utils::print_tree(&c, node_key, display_fn);
    }
}

impl<S, N> Debug for Recomposer<S, N>
where
    N: ComposeNode + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let c = self.composer.read();
        f.debug_struct("Recomposer")
            .field("nodes", &c.nodes)
            .field("states", &c.states)
            .field("dirty_states", &c.dirty_states)
            .finish()
    }
}
