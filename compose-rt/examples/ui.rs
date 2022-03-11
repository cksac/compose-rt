#![allow(non_snake_case)]

use compose_rt::Composer;
use std::fmt::Debug;

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
fn Column<F>(cx: &Composer, children: F)
where
    F: Fn(),
{
    cx.group(|| {
        children();
        RenderColumn {}
    });
}

fn Text(cx: &Composer, text: impl Into<String>) {
    let t = text.into();
    cx.group(|| RenderText(t));
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
        for (key, movie) in movies.iter().enumerate() {
            cx.tag(movie.id, || MovieOverview(cx, &movie))
        }
    })
}

fn MovieOverview(cx: &Composer, movie: &Movie) {
    Text(cx, movie.name.clone());
    // Column(cx, || {
    //     Text(cx, movie);

    //     let count = cx.slot(|| 0usize);
    //     Text(cx, format!("Likes: {}", count));
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
    let movies = vec![Movie::new(1, "A"), Movie::new(3, "C"), Movie::new(2, "B")];
    MoviesScreen(&cx, movies);
    println!("{:#?}", cx);
}
