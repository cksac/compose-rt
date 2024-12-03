use std::any::Any;

use compose_rt::{Composer, Loc, Root};

type Scope<S> = compose_rt::Scope<S, ()>;
pub struct Div;
pub struct Button;

pub trait Html {
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + 'static;

    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static;
}

impl<S> Html for Scope<S>
where
    S: 'static,
{
    #[track_caller]
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + 'static,
    {
        let scope = self.child_scope::<Div>();
        scope.build_container(content);
    }

    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let scope = self.child_scope::<Button>();
        scope.build();
    }
}

// pub trait Compose<P>: Sized {
//     fn emit_to(self, parent: Scope<P>);
// }

// pub struct h_div<F>
// where
//     F: Fn(Scope<Div>) + 'static,
// {
//     pub padding: f32,
//     pub content: F,
// }

// impl<P, F> Compose<P> for h_div<F>
// where
//     F: Fn(Scope<Div>) + 'static,
// {
//     fn emit_to(self, parent: Scope<P>) {
//         let (padding, content) = (self.padding, self.content);
//         let scope = parent.child_scope::<Div>();
//         let composable = move || {
//             content(scope);
//         };
//         composable();
//         parent.build(composable);
//     }
// }

// pub struct h_button<T>
// where
//     T: Into<String>,
// {
//     pub padding: f32,
//     pub text: T,
// }

// impl<P, T> Compose<P> for h_button<T>
// where
//     T: Into<String>,
// {
//     fn emit_to(self, parent: Scope<P>) {
//         let (padding, text) = (self.padding, self.text.into());
//         let composable = move || {
//             println!("<button>{}</button>", text);
//         };
//         composable();
//         parent.build(composable);
//     }
// }

fn app(s: Scope<Root>) {
    s.div(|s| {
        for i in 0..3 {
            s.button(format!("Button {}", i));
        }
    });

    // s.div().padding(10).content(|s| {
    //     s.button().padding(5).text("Click me");
    // });

    // s.div(Modifier::new().padding(32.0), |s| {
    //     s.button(Modifier::new().padding(32.0), "Click me");
    // });

    // h_div {
    //     padding: 32.0,
    //     content: |s: Scope<Div>| {
    //         h_button {
    //             padding: 16.0,
    //             text: "Click me",
    //         }
    //         .emit_to(s);
    //     },
    // }
    // .emit_to(s);
}

fn main() {
    let recomposer = Composer::compose(app);
}
