#![allow(non_snake_case)]

use compose_rt::{compose, ComposeNode, Composer, Recomposer};
use downcast_rs::impl_downcast;
use fake::{Fake, Faker};
use std::{cell::RefCell, fmt::Debug, rc::Rc};

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
        Image(cx, &movie.img_url);
        RandomRenderObject(cx, &movie.name);

        let count = cx.remember(Rc::new(RefCell::new(0usize)));
        Text(cx, format!("compose count {}", count.borrow()));
        *count.borrow_mut() += 1;
    });
}

fn main() {
    // Setup logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Trace)
        .init();

    // define root compose
    let root_fn = |cx: &mut Composer, movies| MoviesScreen(cx, movies);

    let mut recomposer = Recomposer::new();

    // first run
    let movies = vec![Movie::new(1, "A", "IMG_A"), Movie::new(2, "B", "IMG_B")];
    root_fn(recomposer.composer(), &movies);

    // end compose
    recomposer.finalize();
    if let Some(root) = recomposer.root::<Rc<RefCell<RenderFlex>>>() {
        // call paint of render tree
        let mut context = PaintContext::new();
        root.borrow().paint(&mut context);
    }

    // rerun with new input
    let movies = vec![
        Movie::new(1, "AA", "IMG_AA"),
        Movie::new(3, "C", "IMG_C"),
        Movie::new(2, "B", "IMG_B"),
    ];
    root_fn(recomposer.composer(), &movies);

    recomposer.finalize();
    // end compose, Recomposer allow you to access root
    if let Some(root) = recomposer.root::<Rc<RefCell<RenderFlex>>>() {
        // call paint of render tree
        let mut context = PaintContext::new();
        root.borrow().paint(&mut context);
    }
}

////////////////////////////////////////////////////////////////////////////
// Components - Usage of compose-rt
////////////////////////////////////////////////////////////////////////////
#[compose(skip_inject_cx = true)]
pub fn Column<C>(cx: &mut Composer, content: C)
where
    C: Fn(&mut Composer),
{
    cx.group_use_children(
        |_| Rc::new(RefCell::new(RenderFlex::new())),
        content,
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
        |_| {},
    );
}

#[compose(skip_inject_cx = true)]
pub fn Text(cx: &mut Composer, text: impl AsRef<str>) {
    let text = text.as_ref();
    cx.memo(
        |_| Rc::new(RefCell::new(RenderLabel(text.to_string()))),
        |n| n.borrow().0 == text,
        |n| {
            let mut n = n.borrow_mut();
            n.0 = text.to_string();
        },
        |_| {},
    );
}

#[compose(skip_inject_cx = true)]
pub fn Image(cx: &mut Composer, url: impl AsRef<str>) {
    let url = url.as_ref();
    cx.memo(
        |_| Rc::new(RefCell::new(RenderImage(url.to_string()))),
        |n| n.borrow().0 == url,
        |n| {
            let mut n = n.borrow_mut();
            n.0 = url.to_string();
        },
        |_| {},
    );
}

#[compose(skip_inject_cx = true)]
pub fn RandomRenderObject(cx: &mut Composer, text: impl AsRef<str>) {
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
            if let Some(label) = n.borrow_mut().downcast_mut::<RenderLabel>() {
                label.0 = t.to_string();
            }
            if let Some(img) = n.borrow_mut().downcast_mut::<RenderImage>() {
                let url = format!("http://image.com/{}.png", t);
                img.0 = url;
            }
        },
        |_| {},
    );
}

////////////////////////////////////////////////////////////////////////////
// Rendering backend - Not scope of compose-rt
////////////////////////////////////////////////////////////////////////////
pub struct PaintContext {
    depth: usize,
}
impl PaintContext {
    pub fn new() -> Self {
        Self { depth: 0 }
    }
}

pub trait RenderObject: Debug + ComposeNode {
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
