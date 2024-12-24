use std::any::Any;
use std::env;
use std::fmt::Debug;

use compose_rt::node::{Node, NodeData};
use compose_rt::{AnyData, Composer, Root};

pub trait Data: Debug + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> AnyData<T> for Box<dyn Data>
where
    T: Data,
{
    fn new(val: T) -> Self {
        Box::new(val)
    }

    fn value(&self) -> &T {
        self.as_any().downcast_ref::<T>().unwrap()
    }

    fn value_mut(&mut self) -> &mut T {
        self.as_any_mut().downcast_mut::<T>().unwrap()
    }
}

impl NodeData for Box<dyn Data> {
    type Context = ();
}

type Scope<S> = compose_rt::Scope<S, Node<Box<dyn Data>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Div;
impl Data for Div {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Button {
    label: String,
}
impl Data for Button {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Text {
    label: String,
}
impl Data for Text {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub trait Html {
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + Clone + 'static;

    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static;

    fn text<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static;
}

impl<S> Html for Scope<S>
where
    S: 'static,
{
    #[track_caller]
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + Clone + 'static,
    {
        let child_scope = self.child::<Div>();
        self.create_any_node(child_scope, content, || {}, |_, _| Div, |_, _, _| {});
    }

    #[track_caller]
    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let child_scope = self.child::<Button>();
        self.create_any_node(
            child_scope,
            |_| {},
            move || text.clone().into(),
            |text, _| Button { label: text.into() },
            |_, _, _| {},
        );
    }

    #[track_caller]
    fn text<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let child_scope = self.child::<Text>();
        self.create_any_node(
            child_scope,
            |_| {},
            move || text.clone().into(),
            |text, _| Text { label: text.into() },
            |_, _, _| {},
        );
    }
}

fn app(s: Scope<Root>, n: usize) {
    s.div(move |s| {
        let count = s.use_state(|| 0);
        s.text("start");
        s.div(move |s| {
            let c = count.get();
            if c == 0 {
                s.button("Load items");
                count.set(n);
            } else {
                for i in 0..c {
                    s.key(i, move |s| {
                        s.button(format!("Item {}", i));
                    });
                }
                count.set(0);
            }
        });
        s.text("end");
    });
}

fn main() {
    let count = env::args()
        .nth(1)
        .unwrap_or("2".to_string())
        .parse()
        .unwrap();
    let iter = env::args()
        .nth(2)
        .unwrap_or("1".to_string())
        .parse()
        .unwrap();
    let print = env::args()
        .nth(3)
        .unwrap_or("true".to_string())
        .parse()
        .unwrap();
    println!("count: {}, iter: {}, print: {}", count, iter, print);
    let start = std::time::Instant::now();
    let mut recomposer = Composer::compose(move |s| app(s, count), ());
    if print {
        recomposer.print_tree();
    }
    for _ in 0..iter {
        recomposer.recompose();
    }
    if print {
        recomposer.print_tree();
    }
    //println!("{:#?}", recomposer);
    println!("Time: {:?}", start.elapsed());
}
