#![allow(non_snake_case)]

use compose_rt::Composer;
use std::{cell::RefCell, fmt::Debug};

////////////////////////////////////////////////////////////////////////////
// Rendering backend
////////////////////////////////////////////////////////////////////////////
pub trait RenderObject: Debug {}

#[derive(Debug)]
pub struct RenderColumn {}

impl RenderObject for RenderColumn {}

#[derive(Debug)]
pub struct RenderText(String);
impl RenderObject for RenderText {}

////////////////////////////////////////////////////////////////////////////
// Components
////////////////////////////////////////////////////////////////////////////
fn Column<C>(cx: &Composer, content: C)
where
    C: Fn(),
{
    cx.group(
        || RefCell::new(RenderColumn {}),
        || content(),
        |_| false,
        |_| {},
    );
}

fn Text(cx: &Composer, text: impl AsRef<str>) {
    let t = text.as_ref();
    cx.group(
        || RefCell::new(RenderText(t.to_string())),
        || {},
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

fn MoviesScreen(cx: &Composer, movies: Vec<Movie>) {
    Column(cx, || {
        for movie in &movies {
            cx.tag(movie.id, || MovieOverview(cx, &movie))
        }
    })
}

fn MovieOverview(cx: &Composer, movie: &Movie) {
    Text(cx, &movie.name);
    // Column(cx, || {
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

    let cx = Composer::new(10);
    let movies = vec![Movie::new(1, "A"), Movie::new(2, "B")];
    MoviesScreen(&cx, movies);
    println!("{:#?}", cx);

    // TODO: recompose
    cx.reset_cursor();
    let movies = vec![Movie::new(1, "AAA"), Movie::new(3, "C"), Movie::new(2, "B")];
    MoviesScreen(&cx, movies);
    println!("{:#?}", cx);
}
