# compose-rt
![Rust](https://github.com/cksac/compose-rt/workflows/Rust/badge.svg)
[![Docs Status](https://docs.rs/compose-rt/badge.svg)](https://docs.rs/compose-rt)
[![Latest Version](https://img.shields.io/crates/v/compose-rt.svg)](https://crates.io/crates/compose-rt)

A positional memoization runtime similar to Jetpack Compose Runtime.

>What this means is that Compose is, at its core, a general-purpose tool for managing a tree of nodes of any type. Well a “tree of nodes” describes just about anything, and as a result Compose can target just about anything. – [jakewharton](https://jakewharton.com/a-jetpack-compose-by-any-other-name/)

# use cases
- Declarative GUI
    - https://github.com/cksac/oxui, an experiment GUI framework similar to Flutter
- Automatic differentiation
- Others...

# examples
- Below example show how to build a declarative GUI similar to Jetpack Compose UI

```toml
[dependencies]
compose-rt = "0.12"
downcast-rs = "1.2"
log = "0.4"
env_logger = "0.6"
fltk = { version = "^1.2", features = ["fltk-bundled"] }
```

```rust
#![allow(non_snake_case)]

use compose_rt::{compose, Composer, Recomposer};
use fltk::{
    app, button,
    group::{self, Flex},
    prelude::*,
    text,
    window::Window,
};
use std::{cell::RefCell, rc::Rc};

////////////////////////////////////////////////////////////////////////////
// User application
////////////////////////////////////////////////////////////////////////////
pub struct Movie {
    pub id: usize,
    pub name: String,
    pub img_url: String,
}
impl Movie {
    pub fn new(id: usize, name: impl Into<String>, img_url: impl Into<String>) -> Self {
        Movie {
            id,
            name: name.into(),
            img_url: img_url.into(),
        }
    }
}

#[compose]
pub fn MoviesScreen(movies: &Vec<Movie>) {
    Column(cx, |cx| {
        for movie in movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie));
        }
    });
}

#[compose]
pub fn MovieOverview(movie: &Movie) {
    Column(cx, |cx| {
        Text(cx, &movie.name);

        let count = cx.remember(|| Rc::new(RefCell::new(0usize)));
        let c = count.clone();
        Button(
            cx,
            &format!("{} get {} likes", movie.name, count.borrow()),
            move || *c.borrow_mut() += 1,
        );
        Text(cx, format!("Count {}", count.borrow()));
    });
}

fn main() {
    // Setup logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Trace)
        .init();

    let app = app::App::default();
    // define root compose
    let root_fn = |cx: &mut Composer, movies| Window(cx, |cx| MoviesScreen(cx, movies));

    let mut recomposer = Recomposer::new(20);

    let movies = vec![Movie::new(1, "A", "IMG_A"), Movie::new(2, "B", "IMG_B")];
    recomposer.compose(|cx| {
        root_fn(cx, &movies);
    });

    while app.wait() {
        recomposer.compose(|cx| {
            root_fn(cx, &movies);
        });
    }
}

////////////////////////////////////////////////////////////////////////////
// Components - Usage of compose-rt
////////////////////////////////////////////////////////////////////////////
#[compose(skip_inject_cx = true)]
pub fn Window<C>(cx: &mut Composer, content: C)
where
    C: Fn(&mut Composer),
{
    cx.group(
        |_| Window::default().with_size(400, 300),
        |_| false,
        content,
        |_, _| {},
        |win| {
            win.end();
            win.show();
        },
    )
}

#[compose(skip_inject_cx = true)]
pub fn Column<C>(cx: &mut Composer, content: C)
where
    C: Fn(&mut Composer),
{
    cx.group(
        |_| {
            let mut flex = Flex::new(0, 0, 400, 300, None);
            flex.set_type(group::FlexType::Column);
            flex
        },
        |_| false,
        content,
        |_, _| {},
        |flex| {
            flex.end();
        },
    );
}

#[compose(skip_inject_cx = true)]
pub fn Text(cx: &mut Composer, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.memo(
        |_| {
            let mut editor = text::TextEditor::default()
                .with_size(390, 290)
                .center_of_parent();

            let mut buf = text::TextBuffer::default();
            buf.set_text(text);
            editor.set_buffer(buf);
            editor
        },
        |n| n.buffer().unwrap().text().eq(text),
        |n| {
            n.buffer().as_mut().unwrap().set_text(text);
        },
        |_| {},
    );
}

#[compose(skip_inject_cx = true)]
pub fn Button<F>(cx: &mut Composer, text: &str, mut cb: F)
where
    F: 'static + FnMut(),
{
    cx.memo(
        |_| {
            let mut btn = button::Button::new(160, 210, 80, 40, None);
            btn.set_callback(move |_| cb());
            btn
        },
        |n| n.label().eq(text),
        |n| {
            n.set_label(text);
        },
        |_| {},
    );
}
```