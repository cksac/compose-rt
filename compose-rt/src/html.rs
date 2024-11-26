use std::{any::Any, fmt::Debug};

use crate::{composer, Arg, Scope};

pub trait Node: Debug {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub type Cx = composer::Cx<Box<dyn Node>>;

#[derive(Debug)]
pub struct Body;
impl Node for Body {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug)]
pub struct Div;
impl Node for Div {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug)]
pub struct Text(String);
impl Node for Text {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Cx {
    #[track_caller]
    pub fn body<C>(&self, content: C)
    where
        C: Fn(Cx, Scope<Body>) + 'static,
    {
        let scope = Scope::<Body>::new();
        self.create_scope(scope, scope, content, || {}, |_| Box::new(Body), |_, _| {});
    }

    #[track_caller]
    pub fn div<P, C>(&self, parent: Scope<P>, content: C)
    where
        P: 'static,
        C: Fn(Cx, Scope<Div>) + 'static,
    {
        let scope = parent.child_scope::<Div>();
        self.create_scope(parent, scope, content, || {}, |_| Box::new(Div), |_, _| {});
    }

    #[track_caller]
    pub fn text<P, T>(&self, parent: Scope<P>, text: T)
    where
        P: 'static,
        T: Arg<String>,
    {
        let scope = parent.child_scope::<Text>();
        self.create_scope(
            parent,
            scope,
            |_, _| {},
            move || text.arg(),
            |text| Box::new(Text(text)),
            |node, text| {
                let node = node.as_any_mut().downcast_mut::<Text>().unwrap();
                node.0 = text;
            },
        );
    }
}
