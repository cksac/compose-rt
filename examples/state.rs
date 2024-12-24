use std::fmt::Debug;

use compose_rt::node::{Node, NodeData};
use compose_rt::{Composer, Loc, Root};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Data(Loc);

impl Data {
    #[track_caller]
    pub fn new() -> Self {
        Self(Loc::new())
    }
}

impl Default for Data {
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, " {:?} ", self.0)
    }
}

impl NodeData for Data {
    type Context = ();
}

type Scope<S> = compose_rt::Scope<S, Node<Data>>;
type State<T> = compose_rt::State<T, Node<Data>>;

pub struct Container;
pub struct Left;

pub trait ComposerTest {
    fn container<C>(&self, content: C)
    where
        C: Fn(Scope<Container>) + Clone + 'static;

    fn leaf(&self);
}

impl<S> ComposerTest for Scope<S>
where
    S: 'static,
{
    #[track_caller]
    fn container<C>(&self, content: C)
    where
        C: Fn(Scope<Container>) + Clone + 'static,
    {
        let child_scope = self.child::<Container>();
        let data = Data::new();
        self.create_node(child_scope, content, || {}, move |_, _| data, |_, _, _| {});
    }

    #[track_caller]
    fn leaf(&self) {
        let child_scope = self.child::<Left>();
        let data = Data::new();
        self.create_node(child_scope, |_| {}, || {}, move |_, _| data, |_, _, _| {});
    }
}

fn app(s: Scope<Root>, count: State<usize>) {
    s.container(move |s| {
        for i in 0..count.get() {
            s.key(i, |s| {
                s.container(|s| {
                    s.leaf();
                });
            });
        }
    });
}

fn main() {
    let mut recomposer = Composer::compose_with(app, (), || 3);
    recomposer.print_tree();

    recomposer.recompose_with(1);

    recomposer.print_tree();
}
