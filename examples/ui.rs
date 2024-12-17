use std::fmt::Debug;

use compose_rt::{ComposeNode, Composer, Loc, Root};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Node(Loc);

impl Node {
    fn new(loc: Loc) -> Self {
        Self(loc)
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, " {:?} ", self.0)
    }
}

impl ComposeNode for Node {
    type Context = ();
}

type Scope<S> = compose_rt::Scope<S, Node>;
type State<T> = compose_rt::State<T, Node>;

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
        let node = Node::new(Loc::new());
        self.create_node(child_scope, content, |_| {}, move |_, _| node, |_, _, _| {});
    }

    #[track_caller]
    fn leaf(&self) {
        let child_scope = self.child::<Left>();
        let node = Node::new(Loc::new());
        self.create_node(child_scope, |_| {}, |_| {}, move |_, _| node, |_, _, _| {});
    }
}

fn app(s: Scope<Root>, count: State<usize>) {
    s.container(move |s| {
        for i in 0..count.get() {
            s.key(i as u32, |s| {
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
