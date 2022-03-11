#![allow(non_snake_case)]

use compose_rt::{Composer, Data};
use std::{cell::RefCell, fmt::Debug, rc::Rc};

////////////////////////////////////////////////////////////////////////////
// Rendering backend
////////////////////////////////////////////////////////////////////////////
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

fn Column<C>(cx: &mut Composer, content: C)
where
    C: Fn(&mut Composer),
{
    cx.group(
        |_| RefCell::new(RenderFlex::new()),
        |cx| content(cx),
        |node: Rc<RefCell<RenderFlex>>, children: Vec<Rc<dyn Data>>| {
            let mut flex = node.borrow_mut();
            flex.children.clear();
            for child in children {
                // TODO: <dyn Data> to other trait object
                if let Ok(t) = child.clone().downcast_rc::<RefCell<RenderLabel>>() {
                    flex.children.push(t);
                }
                if let Ok(t) = child.clone().downcast_rc::<RefCell<RenderImage>>() {
                    flex.children.push(t);
                }
                if let Ok(t) = child.downcast_rc::<RefCell<RenderFlex>>() {
                    flex.children.push(t);
                }                
            }
        },
        |_| false,
        |_| {},
    );
}

fn Text(cx: &mut Composer, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.group(
        |_| RefCell::new(RenderLabel(text.to_string())),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == text,
        |n| {
            n.borrow_mut().0 = text.to_string();
        },
    );
}

fn Image(cx: &mut Composer, url: impl AsRef<str>) {
    let url = url.as_ref();
    cx.group(
        |_| RefCell::new(RenderImage(url.to_string())),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == url,
        |n| {
            n.borrow_mut().0 = url.to_string();
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

fn MoviesScreen(cx: &mut Composer, movies: Vec<Movie>) {
    Column(cx, |cx| {
        for movie in &movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie))
        }
    })
}

fn MovieOverview(cx: &mut Composer, movie: &Movie) {
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

    let ref mut cx = Composer::new(10);
    let movies = vec![Movie::new(1, "A", "IMG_A"), Movie::new(2, "B", "IMG_B")];
    MoviesScreen(cx, movies);
    println!("{:#?}", cx);
    cx.finalize();

    let movies = vec![
        Movie::new(1, "AA", "IMG_AA"),
        Movie::new(3, "C", "IMG_C"),
        Movie::new(2, "B", "IMG_B"),
    ];
    MoviesScreen(cx, movies);
    println!("{:#?}", cx);
}
