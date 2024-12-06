use compose_rt::{Composer, Root};

type Scope<S> = compose_rt::Scope<S, String>;
pub struct Div;
pub struct Button;

pub struct Text;

pub trait Html {
    fn div<C>(&self, content: C)
    where
        C: Fn(Scope<Div>) + 'static;

    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static;

    fn text<T>(&self, text: T)
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
        self.build_child(scope, content, || {}, |_| String::from("div"), |_, _| {});
    }

    #[track_caller]
    fn button<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let scope = self.child_scope::<Button>();
        self.build_child(
            scope,
            |_| {},
            move || text.clone().into(),
            |text| format!("button({})", text),
            |_, _| {},
        );
    }

    #[track_caller]
    fn text<T>(&self, text: T)
    where
        T: Into<String> + Clone + 'static,
    {
        let scope = self.child_scope::<Text>();
        self.build_child(
            scope,
            |_| {},
            move || text.clone().into(),
            |text| format!("text({})", text),
            |_, _| {},
        );
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
        let count = s.use_state(|| 0);
        s.text("start");
        s.div(move |s| {
            let c = count.get();
            if c == 0 {
                s.button("Load items");
                count.set(c + 1);
            } else {
                for i in 0..c {
                    s.key(i, move |s| {
                        s.button(format!("Item {}", i));
                    });
                }
                count.set(c + 1);
            }
        });

        s.text("end");
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
    println!("{:#?}", recomposer);

    println!("recompose");
    recomposer.recompose();
    println!("{:#?}", recomposer);
}
