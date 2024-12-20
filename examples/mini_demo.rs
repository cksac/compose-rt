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
    type Context = usize;
}

type Scope<S> = compose_rt::Scope<S, Node>;

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
        self.create_node(
            child_scope,
            content,
            || {},
            move |_, c| {
                *c += 1;
                node
            },
            |_, _, _| {},
        );
    }

    #[track_caller]
    fn leaf(&self) {
        let child_scope = self.child::<Left>();
        let node = Node::new(Loc::new());
        self.create_node(
            child_scope,
            |_| {},
            || {},
            move |_, c| {
                *c += 1;
                node
            },
            |_, _, _| {},
        );
    }
}

#[track_caller]
fn component(s: Scope<Container>) {
    s.container(move |s| {
        for i in 0..2 {
            s.key(i, |s| {
                s.leaf();
            });
        }
    });
}

fn app(s: Scope<Root>) {
    s.container(move |s| {
        component(s);
        component(s);
    });
}

fn main() {
    let recomposer = Composer::compose(app, 0);
    recomposer.print_tree();
    recomposer.with_context(|c| {
        println!("Node count: {}", c);
    })
}
