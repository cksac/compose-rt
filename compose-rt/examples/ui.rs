#![allow(non_snake_case)]

use compose_rt::Composer;
use downcast_rs::{impl_downcast, Downcast};
use std::{cell::RefCell, fmt::Debug, rc::Rc};

////////////////////////////////////////////////////////////////////////////
// Rendering backend
////////////////////////////////////////////////////////////////////////////
pub trait Data: Debug + Downcast + Unpin {}
impl_downcast!(Data);

impl<T: 'static + Debug + Unpin> Data for T {}

impl<T: 'static + Debug + Unpin> Into<Box<dyn Data>> for Rc<RefCell<T>> {
    fn into(self) -> Box<dyn Data> {
        Box::new(self)
    }
}

pub trait RenderObject: Debug {}

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
type Context<'a> = &'a mut Composer<dyn Data>;

fn Column<C>(cx: Context, content: C)
where
    C: Fn(Context),
{
    cx.group(
        |_| Rc::new(RefCell::new(RenderFlex::new())),
        |cx| content(cx),
        |node, children| {
            let mut flex = (**node).borrow_mut();
            flex.children.clear();
            for child in children {
                // TODO: <dyn Data> to other trait object?
                if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderLabel>>>().cloned() {
                    flex.children.push(c);
                } else if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderImage>>>().cloned() {
                    flex.children.push(c);
                } else if let Some(c) = child.downcast_ref::<Rc<RefCell<RenderFlex>>>().cloned() {
                    flex.children.push(c);
                }
            }
        },
        |_| false,
        |_| {},
    );
}

fn Text(cx: Context, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.group(
        |_| Rc::new(RefCell::new(RenderLabel(text.to_string()))),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == text,
        |n| {
            let mut n = (**n).borrow_mut();
            n.0 = text.to_string();
        },
    );
}

fn Image(cx: Context, url: impl AsRef<str>) {
    let url = url.as_ref();
    cx.group(
        |_| Rc::new(RefCell::new(RenderImage(url.to_string()))),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == url,
        |n| {
            let mut n = (**n).borrow_mut();
            n.0 = url.to_string();
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

fn MoviesScreen(cx: Context, movies: Vec<Movie>) {
    Column(cx, |cx| {
        for movie in &movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie))
        }
    })
}

fn MovieOverview(cx: Context, movie: &Movie) {
    Column(cx, |cx| {
        Text(cx, &movie.name);
        Image(cx, &movie.img_url);
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

    let mut cx = cx.finalize();

    let movies = vec![
        Movie::new(1, "AA", "IMG_AA"),
        Movie::new(3, "C", "IMG_C"),
        Movie::new(2, "B", "IMG_B"),
    ];
    MoviesScreen(&mut cx, movies);
    println!("{:#?}", cx);
}
