# compose-rt
![Rust](https://github.com/cksac/compose-rt/workflows/Rust/badge.svg)
[![Docs Status](https://docs.rs/compose-rt/badge.svg)](https://docs.rs/compose-rt)
[![Latest Version](https://img.shields.io/crates/v/compose-rt.svg)](https://crates.io/crates/compose-rt)

A positional memoization runtime similar to Jetpack Compose Runtime. 

# example
Below example show how to build a declarative GUI using compose-rt

```toml
[dependencies]
compose-rt = "0.3"
downcast-rs = "1.2.0"
log = "0.4"
env_logger = "0.6"
fake = "2.4"
```

```rust
#![allow(non_snake_case)]

use compose_rt::{ComposeNode, Composer};
use downcast_rs::{impl_downcast, Downcast};
use fake::{Fake, Faker};
use std::{
    any::TypeId,
    cell::{RefCell, RefMut},
    fmt::Debug,
    rc::Rc,
};

////////////////////////////////////////////////////////////////////////////
// User application
////////////////////////////////////////////////////////////////////////////
pub struct Movie {
    id: usize,
    name: String,
    img_url: String,
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

#[track_caller]
pub fn MoviesScreen(cx: Context, movies: &Vec<Movie>) {
    Column(cx, |cx| {
        for movie in movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie))
        }
    })
}

#[track_caller]
pub fn MovieOverview(cx: Context, movie: &Movie) {
    Column(cx, |cx| {
        Text(cx, &movie.name);
        Image(cx, &movie.img_url);
        RandomRenderObject(cx, &movie.name)
    })
}

fn main() {
    // Setup logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Trace)
        .init();

    // define root compose
    let root_fn = |cx: Context, movies| MoviesScreen(cx, movies);

    let mut cx = Composer::new(10);

    // first run
    let movies = vec![Movie::new(1, "A", "IMG_A"), Movie::new(2, "B", "IMG_B")];
    root_fn(&mut cx, &movies);

    // end compose, Recomposer allow you to access root
    let mut recomposer = cx.finalize();
    if let Some(mut root) = recomposer.root_mut() {
        if let Some(render_obj) = root.cast_mut::<Rc<RefCell<RenderFlex>>>() {
            // call paint of render tree
            let mut context = PaintContext::new();
            render_obj.borrow().paint(&mut context);
        }
    }

    cx = recomposer.compose();
    // rerun with new input
    let movies = vec![
        Movie::new(1, "AA", "IMG_AA"),
        Movie::new(3, "C", "IMG_C"),
        Movie::new(2, "B", "IMG_B"),
    ];
    root_fn(&mut cx, &movies);

    // end compose, Recomposer allow you to access root
    let mut recomposer = cx.finalize();
    if let Some(mut root) = recomposer.root_mut() {
        if let Some(render_obj) = root.cast_mut::<Rc<RefCell<RenderFlex>>>() {
            // call paint of render tree
            let mut context = PaintContext::new();
            render_obj.borrow().paint(&mut context);
        }
    }
}

////////////////////////////////////////////////////////////////////////////
// Components - Usage of compose-rt
////////////////////////////////////////////////////////////////////////////
type Context<'a> = &'a mut Composer<dyn Node>;

#[track_caller]
pub fn Column<C>(cx: Context, content: C)
where
    C: Fn(Context),
{
    cx.group_use_children(
        |_| Rc::new(RefCell::new(RenderFlex::new())),
        |cx| content(cx),
        |node, children| {
            let mut flex = node.borrow_mut();
            flex.children.clear();
            for child in children {
                if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderLabel>>>().cloned() {
                    flex.children.push(c);
                } else if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderImage>>>().cloned() {
                    flex.children.push(c);
                } else if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderFlex>>>().cloned() {
                    flex.children.push(c);
                } else if let Some(c) = child
                    .downcast_ref::<Rc<RefCell<dyn RenderObject>>>()
                    .cloned()
                {
                    flex.children.push(c);
                }
            }
        },
        |_| false,
        |_| {},
    );
}

#[track_caller]
pub fn Text(cx: Context, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.memo(
        |_| Rc::new(RefCell::new(RenderLabel(text.to_string()))),
        |n| n.borrow().0 == text,
        |n| {
            let mut n = n.borrow_mut();
            n.0 = text.to_string();
        },
    );
}

#[track_caller]
pub fn Image(cx: Context, url: impl AsRef<str>) {
    let url = url.as_ref();
    cx.memo(
        |_| Rc::new(RefCell::new(RenderImage(url.to_string()))),
        |n| n.borrow().0 == url,
        |n| {
            let mut n = n.borrow_mut();
            n.0 = url.to_string();
        },
    );
}

#[track_caller]
pub fn RandomRenderObject(cx: Context, text: impl AsRef<str>) {
    let t = text.as_ref();
    cx.memo(
        |_| {
            let obj: Rc<RefCell<dyn RenderObject>> = if Faker.fake::<bool>() {
                let url = format!("http://image.com/{}.png", t);
                Rc::new(RefCell::new(RenderImage(url)))
            } else {
                Rc::new(RefCell::new(RenderLabel(t.to_string())))
            };
            obj
        },
        |_| false,
        |n| {
            let n = n.borrow_mut();
            let ty_id = (*n).type_id();

            if ty_id == TypeId::of::<RenderLabel>() {
                let mut label = RefMut::map(n, |x| x.downcast_mut::<RenderLabel>().unwrap());
                label.0 = t.to_string();
            } else if ty_id == TypeId::of::<RenderImage>() {
                let mut img = RefMut::map(n, |x| x.downcast_mut::<RenderImage>().unwrap());
                let url = format!("http://image.com/{}.png", t);
                img.0 = url;
            };
        },
    );
}

////////////////////////////////////////////////////////////////////////////
// Rendering backend - Not scope of compose-rt
////////////////////////////////////////////////////////////////////////////
pub trait Node: Debug + Downcast + Unpin {
    fn debug_print(&self) {}
}
impl_downcast!(Node);

impl<T: 'static + Unpin + Debug> Node for T {}

impl<T: 'static + Unpin + Debug> Into<Box<dyn Node>> for Rc<RefCell<T>> {
    fn into(self) -> Box<dyn Node> {
        Box::new(self)
    }
}

impl<'a> ComposeNode for &'a mut dyn Node {
    fn cast_mut<T: 'static + Debug + Unpin>(&mut self) -> Option<&mut T> {
        self.downcast_mut::<T>()
    }
}

impl Into<Box<dyn Node>> for Rc<RefCell<dyn RenderObject>> {
    fn into(self) -> Box<dyn Node> {
        Box::new(self)
    }
}

pub struct PaintContext {
    depth: usize,
}
impl PaintContext {
    pub fn new() -> Self {
        Self { depth: 0 }
    }
}

pub trait RenderObject: Debug + Downcast {
    fn paint(&self, context: &mut PaintContext);
}
impl_downcast!(RenderObject);

#[derive(Debug)]
pub struct RenderFlex {
    children: Vec<Rc<RefCell<dyn RenderObject>>>,
}

impl RenderFlex {
    pub fn new() -> Self {
        RenderFlex {
            children: Vec::new(),
        }
    }
}

impl RenderObject for RenderFlex {
    fn paint(&self, context: &mut PaintContext) {
        println!(
            "{}<flex size={}>",
            "\t".repeat(context.depth),
            self.children.len()
        );
        context.depth += 1;
        for child in &self.children {
            child.borrow().paint(context);
        }
        context.depth -= 1;
        println!("{}<flex>", "\t".repeat(context.depth));
    }
}

#[derive(Debug)]
pub struct RenderLabel(String);
impl RenderObject for RenderLabel {
    fn paint(&self, context: &mut PaintContext) {
        println!("{}<label>{}</label>", "\t".repeat(context.depth), self.0);
    }
}

#[derive(Debug)]
pub struct RenderImage(String);
impl RenderObject for RenderImage {
    fn paint(&self, context: &mut PaintContext) {
        println!("{}<img src={}/>", "\t".repeat(context.depth), self.0);
    }
}
```