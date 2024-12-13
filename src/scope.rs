use std::any::Any;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;

use generational_box::GenerationalBox;

use crate::composer::Group;
use crate::{offset_to_anchor, Composer, State, StateId};

pub struct Scope<S, N> {
    _scope: PhantomData<S>,
    pub id: ScopeId,
    pub(crate) composer: GenerationalBox<Composer<N>>,
}

impl<S, N> Clone for Scope<S, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, N> Copy for Scope<S, N> {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: Debug + 'static,
{
    #[inline(always)]
    pub(crate) fn new(id: ScopeId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            _scope: PhantomData,
            id,
            composer,
        }
    }

    #[inline(always)]
    pub(crate) fn set_key(&mut self, key: u32) {
        self.id.0 += key as u64;
    }

    #[track_caller]
    #[inline(always)]
    pub fn child_scope<C>(&self) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::new();
        Scope::new(id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn child_scope_with_key<C>(&self, key: u32) -> Scope<C, N>
    where
        C: 'static,
    {
        let id = ScopeId::with_key(key);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn use_state<F, T>(&self, init: F) -> State<T, N>
    where
        T: 'static,
        F: Fn() -> T + 'static,
    {
        let scope_id = self.id;
        let mut c = self.composer.write();
        let scope_states = c.state_data.states.entry(scope_id).or_default();
        let id = StateId::new(scope_id);
        let _ = scope_states.entry(id).or_insert_with(|| Box::new(init()));
        State::new(id, self.composer)
    }

    #[track_caller]
    #[inline(always)]
    pub fn key<C>(&self, key: u32, content: C)
    where
        C: Fn(Self) + 'static,
    {
        let mut c = self.composer.write();
        c.key_stack.push(key);
        drop(c);
        content(*self);
        let mut c = self.composer.write();
        c.key_stack.pop();
    }

    pub fn create_node<C, T, I, A, F, U>(
        &self,
        scope: Scope<T, N>,
        content: C,
        input: I,
        factory: F,
        update: U,
    ) where
        T: 'static,
        C: Fn(Scope<T, N>) + Clone + 'static,
        I: Fn() -> A + Clone + 'static,
        A: 'static,
        F: Fn(A) -> N + Clone + 'static,
        U: Fn(&mut N, A) + Clone + 'static,
    {
        let parent = *self;
        let composable = move || {
            let mut scope = scope;
            let mut c = parent.composer.write();
            if let Some(key) = c.key_stack.last().cloned() {
                scope.set_key(key);
            }
            let is_visited = c.groups.contains_key(&scope.id);
            let is_dirty = c.dirty_scopes.contains(&scope.id);
            let is_initialized = c.is_initialized;
            if !is_dirty && is_visited {
                c.skip_group();
                return;
            }
            let parent_child_idx = c.start_group(scope.id);
            {
                let input = input();
                match c.groups.entry(scope.id) {
                    Occupied(mut entry) => {
                        let group = entry.get_mut();
                        if let Some(node) = group.node.as_mut() {
                            update(node, input);
                        } else {
                            let node = factory(input);
                            group.node = Some(node);
                        }
                    }
                    Vacant(entry) => {
                        let node = factory(input);
                        entry.insert(Group {
                            node: Some(node),
                            parent: parent.id,
                            children: Vec::new(),
                        });
                    }
                }
                if is_initialized {
                    if let Some(curr_child_idx) = parent_child_idx {
                        let parent_grp = c.groups.get_mut(&parent.id).unwrap();
                        if let Some(existing_child) =
                            parent_grp.children.get(curr_child_idx).cloned()
                        {
                            if existing_child != scope.id {
                                //println!("replace grp {:?} by {:?}", existing_child, scope.id);
                                parent_grp.children[curr_child_idx] = scope.id;
                                c.unmount_scopes.insert(existing_child);
                            }
                        } else {
                            //println!("new grp {:?}", scope.id);
                            parent_grp.children.push(scope.id);
                            c.mount_scopes.insert(scope.id);
                        }
                    }
                } else if let Some(parent_grp) = c.groups.get_mut(&parent.id) {
                    parent_grp.children.push(scope.id);
                }
            }
            drop(c);
            content(scope);
            let mut c = parent.composer.write();
            if is_dirty {
                c.dirty_scopes.remove(&scope.id);
            }
            c.end_group(parent.id, scope.id);
        };
        composable();
        let mut c = self.composer.write();
        c.composables
            .entry(scope.id)
            .or_insert_with(|| Box::new(composable));
    }

    // pub fn create_any_node<C, T, I, A, E, F, U>(
    //     &self,
    //     scope: Scope<T, N>,
    //     content: C,
    //     input: I,
    //     factory: F,
    //     update: U,
    // ) where
    //     T: 'static,
    //     C: Fn(Scope<T, N>) + 'static,
    //     I: Fn() -> A + 'static,
    //     A: 'static,
    //     N: AnyNode<E>,
    //     E: 'static,
    //     F: Fn(A) -> E + 'static,
    //     U: Fn(&mut E, A) + 'static,
    // {
    //     let c = self.composer.read();
    //     c.create_node_scope(
    //         *self,
    //         scope,
    //         content,
    //         input,
    //         move |args| {
    //             let e = factory(args);
    //             AnyNode::new(e)
    //         },
    //         move |n, args| {
    //             let e = n.val_mut();
    //             update(e, args);
    //         },
    //     );
    // }
}

pub trait AnyNode<T> {
    fn new(val: T) -> Self;
    fn val(&self) -> &T;
    fn val_mut(&mut self) -> &mut T;
}

impl<T> AnyNode<T> for Box<dyn Any>
where
    T: 'static,
{
    fn new(val: T) -> Self {
        Box::new(val)
    }

    fn val(&self) -> &T {
        self.downcast_ref::<T>().unwrap()
    }

    fn val_mut(&mut self) -> &mut T {
        self.downcast_mut::<T>().unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub(crate) u64);

impl ScopeId {
    #[track_caller]
    #[inline(always)]
    pub fn new() -> Self {
        let id = (offset_to_anchor() as u64) << 32;
        Self(id)
    }

    #[track_caller]
    #[inline(always)]
    pub fn with_key(key: u32) -> Self {
        let mut scope = Self::new();
        scope.0 += key as u64;
        scope
    }
}

impl Debug for ScopeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeId({})", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Root;
