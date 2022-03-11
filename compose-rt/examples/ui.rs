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
                if let Ok(t) = child.downcast_rc::<RefCell<RenderLabel>>() {
                    flex.children.push(t);
                }
            }
        },
        |_| false,
        |_| {},
    );
}

fn Text(cx: &mut Composer, text: impl AsRef<str>) {
    let t = text.as_ref();
    cx.group(
        |_| RefCell::new(RenderLabel(t.to_string())),
        |_| {},
        |_, _| {},
        |n| n.borrow().0 == t,
        |n| {
            n.borrow_mut().0 = t.to_string();
        },
    );
}

////////////////////////////////////////////////////////////////////////////
// User application
////////////////////////////////////////////////////////////////////////////
pub struct Movie {
    id: usize,
    name: String,
}
impl Movie {
    pub fn new(id: usize, name: impl Into<String>) -> Self {
        Movie {
            id,
            name: name.into(),
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
    Text(cx, &movie.name);
    // Column(cx, |cx| {
    //     Text(cx, "name");

    //     //let count = cx.slot(|| 0usize);
    //     Text(cx, movie.name.clone());
    // })
}

fn main() {
    // Setup logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Trace)
        .init();

    let ref mut cx = Composer::new(10);
    let movies = vec![Movie::new(1, "A"), Movie::new(2, "B")];
    MoviesScreen(cx, movies);
    println!("{:#?}", cx);
    cx.finalize();

    let movies = vec![Movie::new(1, "AAA"), Movie::new(3, "C"), Movie::new(2, "B")];
    MoviesScreen(cx, movies);
    println!("{:#?}", cx);
}
