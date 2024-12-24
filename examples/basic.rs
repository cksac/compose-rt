use std::env;

use compose_rt::node::NodeData;
use compose_rt::{ComposeNode, Composer, NodeKey, Root, ScopeId};

#[derive(Debug)]
pub struct Data(String);

impl From<String> for Data {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Data {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl NodeData for Data {
    type Context = ();
}

type Node = compose_rt::node::Node<Data>;

type Scope<S> = compose_rt::Scope<S, Node>;

pub struct Div;
pub struct Button;
pub struct Text;

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
        self.create_node(
            child_scope,
            content,
            || {},
            |_, _| "div".into(),
            |_, _, _| {},
        );
    }

    #[track_caller]
    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let child_scope = self.child::<Button>();
        self.create_node(
            child_scope,
            |_| {},
            move || text.clone().into(),
            |text, _| format!("button({})", text).into(),
            |_, _, _| {},
        );
    }

    #[track_caller]
    fn text<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let child_scope = self.child::<Text>();
        self.create_node(
            child_scope,
            |_| {},
            move || text.clone().into(),
            |text, _| format!("text({})", text).into(),
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
