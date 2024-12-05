use std::fmt::Debug;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::sync::RwLock;

use compose_rt::ScopeId;
use generational_box::AnyStorage;
use generational_box::GenerationalBox;
use generational_box::Owner;

use generational_box::UnsyncStorage;
use slotmap::{SlotMap, new_key_type};

new_key_type! {
    struct GroupKey;
}

#[derive(Debug)]
pub struct Group<N> {
    scope_id: ScopeId,
    node: Option<N>,
    parent: Option<GroupKey>,
    children: Vec<GroupKey>,
}

#[derive(Debug)]
pub struct Composer<N> {
    groups: RwLock<SlotMap<GroupKey, Group<N>>>,
    start_stack: RwLock<Vec<GroupKey>>,
    child_count_stack: RwLock<Vec<usize>>,
    current_group: RwLock<Option<GroupKey>>,
}

impl<N> Composer<N>
where
    N: Debug + 'static,
{
    pub fn new() -> Self {
        Composer {
            groups: RwLock::new(SlotMap::with_key()),
            start_stack: RwLock::new(Vec::new()),
            child_count_stack: RwLock::new(vec![0]),
            current_group: RwLock::new(None),
        }
    }

    pub fn compose<F>(root: F) -> Recomposer<N>
    where
        F: FnOnce(Scope<Root, N>),
    {
        let id = ScopeId::new(0);
        let owner = UnsyncStorage::owner();
        let composer = owner.insert(Composer::new());
        let scope = Scope::new(id, composer);
        root(scope);
        Recomposer { owner, composer }
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
            let c = scope.composer.read();
            c.start_group(scope.id);
            content(scope);
            c.end_group(parent.id, scope.id);
        };
        composable();
    }

    pub fn start_group(&self, scope: ScopeId) {
        let parent = self.start_stack.read().unwrap().last().cloned();
        let curr_child_idx = self
            .child_count_stack
            .read()
            .unwrap()
            .last()
            .cloned()
            .unwrap();
        println!("start_group: {:?} {:?} {:?}", scope, parent, curr_child_idx);

        let mut groups = self.groups.write().unwrap();
        let group = Group {
            scope_id: scope,
            node: None,
            parent,
            children: Vec::new(),
        };
        let key = groups.insert(group);
        self.start_stack.write().unwrap().push(key);
        self.child_count_stack.write().unwrap().push(0);
    }

    pub fn end_group(&self, parent: ScopeId, scope: ScopeId) {
        let current = self.start_stack.write().unwrap().pop().unwrap();
        let mut child_count_stack = self.child_count_stack.write().unwrap();
        let child_count = child_count_stack.pop().unwrap();
        if let Some(parent_child_count) = child_count_stack.last_mut() {
            *parent_child_count += 1;
        }
        let mut groups = self.groups.write().unwrap();
        let old_child_count = groups[current].children.len();
        if child_count < old_child_count {
            groups[current].children.truncate(child_count);
        }
        let parent = self.start_stack.read().unwrap().last().cloned();
        if let Some(parent) = parent {
            groups[parent].children.push(current);
        }
    }
}

pub struct Recomposer<N> {
    owner: Owner,
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Debug for Recomposer<N>
where
    N: 'static + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let c = self.composer.read();
        f.debug_struct("Recomposer")
            .field("groups", &c.groups)
            .finish()
    }
}

pub struct Scope<S, N> {
    ty: std::marker::PhantomData<S>,
    id: ScopeId,
    composer: GenerationalBox<Composer<N>>,
}

impl<S, N> Clone for Scope<S, N> {
    fn clone(&self) -> Self {
        Self {
            ty: PhantomData,
            id: self.id,
            composer: self.composer.clone(),
        }
    }
}

impl<S, N> Copy for Scope<S, N> {}

impl<S, N> Scope<S, N>
where
    S: 'static,
    N: Debug + 'static,
{
    pub fn new(id: ScopeId, composer: GenerationalBox<Composer<N>>) -> Self {
        Self {
            ty: PhantomData,
            id,
            composer,
        }
    }

    #[track_caller]
    pub fn child_scope<T>(&self) -> Scope<T, N>
    where
        T: 'static,
    {
        let id = ScopeId::with_key(self.id.key, self.id.depth + 1);
        Scope::new(id, self.composer)
    }

    #[track_caller]
    pub fn build_child<C, T>(&self, scope: Scope<T, N>, content: C)
    where
        T: 'static,
        C: Fn(Scope<T, N>) + 'static,
    {
        let c = self.composer.read();
        c.create_scope(*self, scope, content);
    }
}

pub struct Root;

//
type Htlm<S> = Scope<S, String>;
pub struct Body;
pub struct Div;
pub struct Text;

#[track_caller]
fn body<C>(s: Htlm<Root>, content: C)
where
    C: Fn(Htlm<Body>) + 'static,
{
    let scope = s.child_scope::<Body>();
    s.build_child(scope, content);
}

#[track_caller]
fn div<P, C>(s: Htlm<P>, content: C)
where
    P: 'static,
    C: Fn(Htlm<Div>) + 'static,
{
    let scope = s.child_scope::<Div>();
    s.build_child(scope, content);
}

#[track_caller]
fn text<P, T>(s: Htlm<P>, text: T)
where
    P: 'static,
    T: Into<String> + Clone + 'static,
{
    let scope = s.child_scope::<Text>();
    s.build_child(scope, move |s| println!("text: {}", text.clone().into()));
}

fn app(s: Htlm<Root>) {
    body(s, |s| {
        div(s, |s| {
            text(s, "Hello, world!");
            text(s, "Hello, world!");
        });
    });
}

fn main() {
    let recomposer = Composer::compose(app);
    println!("{:?}", recomposer);
}
