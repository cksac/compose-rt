use std::env;

use compose_rt::{Composer, Root};

type Scope<S> = compose_rt::Scope<S, String>;
pub struct Div;
pub struct Button;

pub struct Text;

pub trait Html {
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + 'static;

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
        C: Fn(Scope<Div>) + 'static,
    {
        let scope = self.child_scope::<Div>();
        self.build_child(scope, content, || {}, |_| String::from("div"), |_, _| {});
    }

    #[track_caller]
    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let scope = self.child_scope::<Button>();
        self.build_child(
            scope,
            |_| {},
            move || text.clone().into(),
            |text| format!("button({})", text),
            |_, _| {},
        );
    }

    #[track_caller]
    fn text<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let scope = self.child_scope::<Text>();
        self.build_child(
            scope,
            |_| {},
            move || text.clone().into(),
            |text| format!("text({})", text),
            |_, _| {},
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
        .unwrap_or("100".to_string())
        .parse()
        .unwrap();
    let start = std::time::Instant::now();
    let recomposer = Composer::compose(move |s| app(s, count));
    recomposer.recompose();
    recomposer.recompose();
    println!("Time: {:?}", start.elapsed());
}
