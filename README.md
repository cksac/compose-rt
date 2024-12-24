# compose-rt
![Rust](https://github.com/cksac/compose-rt/workflows/Rust/badge.svg)
[![Docs Status](https://docs.rs/compose-rt/badge.svg)](https://docs.rs/compose-rt)
[![Latest Version](https://img.shields.io/crates/v/compose-rt.svg)](https://crates.io/crates/compose-rt)

A positional memoization runtime similar to Jetpack Compose Runtime.

>What this means is that Compose is, at its core, a general-purpose tool for managing a tree of nodes of any type. Well a “tree of nodes” describes just about anything, and as a result Compose can target just about anything. – [jakewharton](https://jakewharton.com/a-jetpack-compose-by-any-other-name/)

## Example
```rust
use std::env;

use compose_rt::{ComposeNode, Composer, Root};

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

impl ComposeNode for Data {
    type Context = ();
}

type Scope<S> = compose_rt::Scope<S, Data>;

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
            |n, text, _| {
                n.0 = text;
            },
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
            |n, text, _| {
                n.0 = text;
            },
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
    println!("Time: {:?}", start.elapsed());
}
```

## LICENSE
This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.
