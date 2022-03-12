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
// Rendering backend
////////////////////////////////////////////////////////////////////////////
#[derive(Debug)]
#[non_exhaustive]
pub enum Node {
    RenderFlex(Rc<RefCell<RenderFlex>>),
    RenderLabel(Rc<RefCell<RenderLabel>>),
    RenderImage(Rc<RefCell<RenderImage>>),
    RenderObject(Rc<RefCell<dyn RenderObject>>),
}

macro_rules! into_boxed_node {
    ($ty:ident) => {
        impl Into<Box<Node>> for Rc<RefCell<$ty>> {
            fn into(self) -> Box<Node> {
                Box::new(Node::$ty(self))
            }
        }
    };
    (dyn $ty:ident) => {
        impl Into<Box<Node>> for Rc<RefCell<dyn $ty>> {
            fn into(self) -> Box<Node> {
                Box::new(Node::$ty(self))
            }
        }
    };
}

into_boxed_node!(RenderFlex);
into_boxed_node!(RenderLabel);
into_boxed_node!(RenderImage);
into_boxed_node!(dyn RenderObject);

impl<'a> ComposeNode for &'a mut Node {
    fn cast_mut<T: 'static + Unpin + Debug>(&mut self) -> Option<&mut T> {
        match self {
            Node::RenderFlex(r) => r.as_any_mut().downcast_mut::<T>(),
            Node::RenderLabel(r) => r.as_any_mut().downcast_mut::<T>(),
            Node::RenderImage(r) => r.as_any_mut().downcast_mut::<T>(),
            Node::RenderObject(r) => r.as_any_mut().downcast_mut::<T>(),
        }
    }
}

pub trait RenderObject: Debug + Downcast + Unpin {}
impl_downcast!(RenderObject);

pub struct RenderFlex {
    children: Vec<Rc<RefCell<dyn RenderObject>>>,
}

impl Debug for RenderFlex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // trim debug print
        f.debug_struct("RenderFlex")
            .field("children_count", &self.children.len())
            .finish()
    }
}

impl RenderFlex {
    pub fn new() -> Self {
        RenderFlex {
            children: Vec::new(),
        }
    }
}

impl RenderObject for RenderFlex {}

#[derive(Debug)]
pub struct RenderLabel(String);
impl RenderObject for RenderLabel {}

#[derive(Debug)]
pub struct RenderImage(String);
impl RenderObject for RenderImage {}

////////////////////////////////////////////////////////////////////////////
// Components
////////////////////////////////////////////////////////////////////////////
type Context<'a> = &'a mut Composer<Node>;

#[track_caller]
pub fn Column<C>(cx: Context, content: C)
where
    C: Fn(Context),
{
    cx.group(
        |_| Rc::new(RefCell::new(RenderFlex::new())),
        |cx| content(cx),
        |node, children| {
            let mut flex = node.borrow_mut();
            flex.children.clear();
            for child in children {
                match child {
                    Node::RenderFlex(c) => flex.children.push(c.clone()),
                    Node::RenderLabel(c) => flex.children.push(c.clone()),
                    Node::RenderImage(c) => flex.children.push(c.clone()),
                    Node::RenderObject(c) => flex.children.push(c.clone()),
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
    cx.group(
        |_| Rc::new(RefCell::new(RenderLabel(text.to_string()))),
        |_| {},
        |_, _| {},
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
    cx.group(
        |_| Rc::new(RefCell::new(RenderImage(url.to_string()))),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == url,
        |n| {
            let mut n = n.borrow_mut();
            n.0 = url.to_string();
        },
    );
}

#[track_caller]
pub fn RandomRenderObject(cx: Context, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.group(
        |_| {
            let obj: Rc<RefCell<dyn RenderObject>> = if Faker.fake::<bool>() {
                let url = format!("http://image.com/{}.png", text);
                Rc::new(RefCell::new(RenderImage(url)))
            } else {
                Rc::new(RefCell::new(RenderLabel(text.to_string())))
            };
            obj
        },
        |_| {},
        |_, _| {},
        |_| false,
        |n| {
            let n = n.borrow_mut();
            let ty_id = (*n).type_id();

            // TODO: why n.as_any_mut().downcast_mut::<Rc<RefCell<RenderLabel>>>() not work?
            if ty_id == TypeId::of::<RenderLabel>() {
                let mut label = RefMut::map(n, |x| x.downcast_mut::<RenderLabel>().unwrap());
                label.0 = text.to_string();
            } else if ty_id == TypeId::of::<RenderImage>() {
                let mut img = RefMut::map(n, |x| x.downcast_mut::<RenderImage>().unwrap());
                let url = format!("http://image.com/{}.png", text);
                img.0 = url;
            };
        },
    );
}

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

// TODO: ROOT should not track caller, require start_root in composer
// #[track_caller]
fn MoviesScreen(cx: Context, movies: Vec<Movie>) {
    Column(cx, |cx| {
        for movie in &movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie))
        }
    })
}

#[track_caller]
fn MovieOverview(cx: Context, movie: &Movie) {
    Column(cx, |cx| {
        Text(cx, &movie.name);
        Image(cx, &movie.img_url);
        RandomRenderObject(cx, Faker.fake::<String>());
    })
}
fn main() {
    // Setup logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Trace)
        .init();

    let mut cx = Composer::new(10);

    let movies = vec![Movie::new(1, "A", "IMG_A"), Movie::new(2, "B", "IMG_B")];
    MoviesScreen(&mut cx, movies);
    println!("{:#?}", cx);

    cx = cx.finalize();

    let movies = vec![
        Movie::new(1, "AA", "IMG_AA"),
        Movie::new(3, "C", "IMG_C"),
        Movie::new(2, "D", "IMG_B"),
    ];
    MoviesScreen(&mut cx, movies);
    println!("{:#?}", cx);
}
