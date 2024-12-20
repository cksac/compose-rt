use std::hint::black_box;

use compose_rt::{ComposeNode, Composer, Root};
use criterion::{criterion_group, criterion_main, Criterion};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node;

impl ComposeNode for Node {
    type Context = ();
}

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
        self.create_node(child_scope, content, |_| {}, |_, _| Node, |_, _, _| {});
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
            move |_| text.clone().into(),
            |_, _| Node,
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
            move |_| text.clone().into(),
            |_, _| Node,
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

fn run_app(count: usize) {
    let mut recomposer = Composer::compose(move |s: Scope<Root>| app(s, count), ());
    recomposer.recompose();
    recomposer.recompose();
}

fn criterion_benchmark(c: &mut Criterion) {
    for count in [100, 1000, 5000, 10000, 50000] {
        c.bench_function(&format!("bench {}", count), |b| {
            b.iter(|| run_app(black_box(count)))
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
