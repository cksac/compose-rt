use std::{
    any::Any,
    cell::RefCell,
    fmt::{Debug, Formatter},
};

use ahash::{AHashMap, AHashSet};
use generational_box::{AnyStorage, GenerationalBox, Owner, UnsyncStorage};
use slotmap::{new_key_type, SecondaryMap, SlotMap};

use crate::v2::{Root, Scope, ScopeId, State, StateId};

new_key_type! { pub(crate) struct ScopeKey; }

pub struct Composer<N> {
    pub(crate) new_composables: RefCell<AHashMap<ScopeId, Box<dyn Fn()>>>,

    scopes: RefCell<SlotMap<ScopeKey, ScopeId>>,
    pub(crate) scope_keys: RefCell<AHashMap<ScopeId, ScopeKey>>,
    composables: RefCell<SecondaryMap<ScopeKey, Box<dyn Fn()>>>,
    nodes: RefCell<SecondaryMap<ScopeKey, N>>,
    children: RefCell<SecondaryMap<ScopeKey, Vec<ScopeKey>>>,
    parent: RefCell<SecondaryMap<ScopeKey, ScopeKey>>,
    pub(crate) states: RefCell<SecondaryMap<ScopeKey, AHashMap<StateId, Box<dyn Any>>>>,
    pub(crate) subscribers: RefCell<AHashMap<StateId, AHashSet<ScopeId>>>,
    current_scope: RefCell<(ScopeId, ScopeKey)>,
    child_count_stack: RefCell<Vec<usize>>,
    pub(crate) dirty_states: RefCell<AHashSet<StateId>>,
    pub(crate) dirty_scopes: RefCell<AHashSet<ScopeId>>,
    unmount_scopes: RefCell<AHashSet<ScopeKey>>,
}

pub struct Recomposer<N> {
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Recomposer<N>
where
    N: Debug + 'static,
{
    pub fn recompose(&self) {
        let c = self.composer.read();
        c.recompose();
    }
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Self {
            scopes: RefCell::new(SlotMap::with_key()),
            scope_keys: RefCell::new(AHashMap::new()),
            composables: RefCell::new(SecondaryMap::new()),
            nodes: RefCell::new(SecondaryMap::new()),
            children: RefCell::new(SecondaryMap::new()),
            parent: RefCell::new(SecondaryMap::new()),
            states: RefCell::new(SecondaryMap::new()),
            subscribers: RefCell::new(AHashMap::new()),
            current_scope: RefCell::new((ScopeId::new(0), ScopeKey::default())),
            child_count_stack: RefCell::new(Vec::new()),
            dirty_states: RefCell::new(AHashSet::new()),
            dirty_scopes: RefCell::new(AHashSet::new()),
            unmount_scopes: RefCell::new(AHashSet::new()),
            new_composables: RefCell::new(AHashMap::new()),
        }
    }

    #[track_caller]
    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: Fn(Scope<Root, N>) + 'static,
    {
        let id = ScopeId::new(0);
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let scope = Scope::new(id, composer);
        let c = composer.read();
        let key = c.start_root(scope.id, move || {
            root(scope);
        });
        c.end_root(scope.id, key);
        let mut new_composables = c.new_composables.borrow_mut();
        let mut composables = c.composables.borrow_mut();
        let keys = c.scope_keys.borrow();
        for (s, f) in new_composables.drain() {
            let key = keys.get(&s).cloned().unwrap();
            if composables.contains_key(key) {
                continue;
            }
            composables.insert(key, f);
        }
        Recomposer { owner, composer }
    }

    pub(crate) fn recompose(&self) {
        let mut affected_scopes = AHashSet::default();
        {
            let mut dirty_states = self.dirty_states.borrow_mut();
            let subscribers = self.subscribers.borrow_mut();
            for state_id in dirty_states.drain() {
                if let Some(scopes) = subscribers.get(&state_id) {
                    affected_scopes.extend(scopes.iter().cloned());
                }
            }
        }
        let mut affected_scopes = affected_scopes.into_iter().collect::<Vec<_>>();
        affected_scopes.sort_by(|a, b| b.depth.cmp(&a.depth));
        {
            let mut dirty_scopes = self.dirty_scopes.borrow_mut();
            dirty_scopes.clear();
            dirty_scopes.extend(affected_scopes.iter().cloned());
        }
        {
            let composables = self.composables.borrow();
            for scope in affected_scopes {
                let key = self.scope_keys.borrow().get(&scope).cloned().unwrap();
                if let Some(composable) = composables.get(key) {
                    composable();
                }
            }
        }
        let mut composables = self.composables.borrow_mut();
        let mut states = self.states.borrow_mut();
        let mut subs = self.subscribers.borrow_mut();
        for s in self.unmount_scopes.borrow_mut().drain() {
            composables.remove(s);
            if let Some(scope_states) = states.remove(s) {
                for state in scope_states.keys() {
                    subs.remove(state);
                }
            }
        }
        let mut new_composables = self.new_composables.borrow_mut();
        let keys = self.scope_keys.borrow();
        for (s, f) in new_composables.drain() {
            let key = keys.get(&s).cloned().unwrap();
            if composables.contains_key(key) {
                continue;
            }
            composables.insert(key, f);
        }
    }

    pub(crate) fn create_scope<C, P, S>(&self, parent: Scope<P, N>, scope: Scope<S, N>, content: C)
    where
        P: 'static,
        S: 'static,
        C: Fn(Scope<S, N>) + 'static,
    {
        let composable = move || {
            let parent = parent;
            let scope = scope;
            let c = parent.composer.read();
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && c.is_visited(scope.id) {
                c.skip_group();
                return;
            }
            let (parent_scope_id, parent_scope_key) = c.get_current_scope();
            let (scope_key, parent_child_idx) =
                c.start_group(parent.id, parent_scope_key, scope.id);
            {
                if let Some(curr_child_idx) = parent_child_idx {
                    let mut children = c.children.borrow_mut();
                    let parent_children = children.get_mut(parent_scope_key).unwrap();
                    if let Some(existing_child) = parent_children.get(curr_child_idx).cloned() {
                        let existint_scope =
                            c.scopes.borrow().get(existing_child).cloned().unwrap();
                        if existint_scope != scope.id {
                            //println!("replace grp {:?} by {:?}", existing_child, scope.id);
                            parent_children[curr_child_idx] = existing_child;
                            c.unmount_scopes.borrow_mut().insert(existing_child);
                        }
                    } else {
                        //println!("new grp {:?}", scope.id);
                        parent_children.push(scope_key);
                    }
                }
            }
            content(scope);
            if is_dirty {
                c.clear_dirty(scope.id);
            }
            c.end_group(parent.id, scope.id);
        };
        composable();
        let mut new_composables = self.new_composables.borrow_mut();
        if !new_composables.contains_key(&scope.id) {
            new_composables.insert(scope.id, Box::new(composable));
        }
    }

    pub(crate) fn create_scope_with_node<C, P, S, I, A, F, U>(
        &self,
        parent: Scope<P, N>,
        scope: Scope<S, N>,
        content: C,
        input: I,
        factory: F,
        update: U,
    ) where
        P: 'static,
        S: 'static,
        C: Fn(Scope<S, N>) + 'static,
        I: Fn() -> A + 'static,
        A: 'static,
        F: Fn(A) -> N + 'static,
        U: Fn(&mut N, A) + 'static,
    {
        let composable = move || {
            let parent = parent;
            let scope = scope;
            let c = parent.composer.read();
            let is_dirty = c.is_dirty(scope.id);
            if !is_dirty && c.is_visited(scope.id) {
                c.skip_group();
                return;
            }
            let (parent_scope_id, parent_scope_key) = c.get_current_scope();
            let (scope_key, parent_child_idx) =
                c.start_group(parent.id, parent_scope_key, scope.id);
            {
                let mut nodes = c.nodes.borrow_mut();
                match nodes.get_mut(scope_key) {
                    Some(node) => {
                        let input = input();
                        update(node, input);
                    }
                    None => {
                        let input = input();
                        let node = factory(input);
                        nodes.insert(scope_key, node);
                    }
                }

                if let Some(curr_child_idx) = parent_child_idx {
                    let mut children = c.children.borrow_mut();
                    if let Some(parent_children) = children.get_mut(parent_scope_key) {
                        if let Some(existing_child) = parent_children.get(curr_child_idx).cloned() {
                            let existint_scope =
                                c.scopes.borrow().get(existing_child).cloned().unwrap();
                            if existint_scope != scope.id {
                                //println!("replace grp {:?} by {:?}", existing_child, scope.id);
                                parent_children[curr_child_idx] = existing_child;
                                c.unmount_scopes.borrow_mut().insert(existing_child);
                            }
                        } else {
                            //println!("new grp {:?}", scope.id);
                            parent_children.push(scope_key);
                        }
                    }
                }
            }
            content(scope);
            if is_dirty {
                c.clear_dirty(scope.id);
            }
            c.end_group(parent.id, scope.id);
        };
        composable();
        let mut new_composables = self.new_composables.borrow_mut();
        if !new_composables.contains_key(&scope.id) {
            new_composables.insert(scope.id, Box::new(composable));
        }
    }

    #[inline(always)]
    fn start_group(
        &self,
        parent: ScopeId,
        parent_key: ScopeKey,
        scope: ScopeId,
    ) -> (ScopeKey, Option<usize>) {
        let mut scope_keys = self.scope_keys.borrow_mut();
        let scope_key = scope_keys.entry(scope).or_insert_with(|| {
            let key = self.scopes.borrow_mut().insert(scope);
            self.children.borrow_mut().insert(key, Vec::new());
            self.states.borrow_mut().insert(key, AHashMap::new());
            self.parent.borrow_mut().insert(key, parent_key);
            key
        });
        self.set_current_scope(scope, *scope_key);
        let parent_child_idx = self.child_count_stack.borrow().last().cloned();
        self.child_count_stack.borrow_mut().push(0);
        (*scope_key, parent_child_idx)
    }

    #[inline(always)]
    fn end_group(&self, parent: ScopeId, scope: ScopeId) {}

    #[inline(always)]
    fn start_root<F>(&self, scope: ScopeId, composable: F) -> ScopeKey
    where
        F: Fn() + 'static,
    {
        let scope_key = self.scopes.borrow_mut().insert(scope);
        self.scope_keys.borrow_mut().insert(scope, scope_key);
        self.set_current_scope(scope, scope_key);
        self.child_count_stack.borrow_mut().push(0);
        composable();
        self.composables
            .borrow_mut()
            .insert(scope_key, Box::new(composable));
        self.children.borrow_mut().insert(scope_key, Vec::new());
        scope_key
    }

    #[inline(always)]
    fn end_root(&self, scope_id: ScopeId, scope_key: ScopeKey) {
        let mut child_count_stack = self.child_count_stack.borrow_mut();
        let child_count = child_count_stack.pop().unwrap();
        let mut children = self.children.borrow_mut();
        let scope_children = children.get_mut(scope_key).unwrap();
        let old_child_count = scope_children.len();
        if child_count < old_child_count {
            scope_children.truncate(child_count);
        }
    }

    #[inline(always)]
    pub(crate) fn get_current_scope(&self) -> (ScopeId, ScopeKey) {
        *self.current_scope.borrow()
    }

    #[inline(always)]
    fn set_current_scope(&self, scope: ScopeId, scope_key: ScopeKey) {
        let mut current_scope = self.current_scope.borrow_mut();
        *current_scope = (scope, scope_key);
    }

    #[inline(always)]
    fn skip_group(&self) {
        let mut child_count_stack = self.child_count_stack.borrow_mut();
        if let Some(parent_child_count) = child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
    }

    #[inline(always)]
    fn is_registered(&self, scope_key: ScopeKey) -> bool {
        let composables = self.composables.borrow();
        composables.contains_key(scope_key)
    }

    #[inline(always)]
    fn is_visited(&self, scope: ScopeId) -> bool {
        let scope_keys = self.scope_keys.borrow();
        scope_keys.contains_key(&scope)
    }

    #[inline(always)]
    fn is_dirty(&self, scope: ScopeId) -> bool {
        let dirty_scopes = self.dirty_scopes.borrow();
        dirty_scopes.contains(&scope)
    }

    #[inline(always)]
    fn clear_dirty(&self, scope: ScopeId) {
        let mut dirty_scopes = self.dirty_scopes.borrow_mut();
        dirty_scopes.remove(&scope);
    }
}

impl<N> Debug for Composer<N>
where
    N: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("scopes", &self.scopes)
            .field("nodes", &self.nodes)
            .finish()
    }
}

impl<N> Debug for Recomposer<N>
where
    N: 'static + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let c = self.composer.read();
        f.debug_struct("Recomposer")
            .field("scopes", &c.scopes)
            .field("nodes", &c.nodes)
            .finish()
    }
}
