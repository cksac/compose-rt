use compose_runtime::{
    html::{body, div, text},
    key, use_state, Composer,
};

fn app() {
    body(|c| {
        div(c, |c| {
            let count = use_state(|| 0);
            text(c, "start");
            if count.get() == 0 {
                text(c, "loading...");
                count.set(1);
            } else {
                text(c, "loaded");
                text(c, move || format!("Total Items: {}", count.get()));
                for i in 0..count.get() {
                    key(i, || {
                        text(c, move || format!("Item {}", i));
                    });
                }
                count.set(0);
            }
            text(c, "end");
        })
    });
}

fn main() {
    let recomposer = Composer::compose(app);
    println!("recomposing...");
    recomposer.recompose();
    println!("{:#?}", recomposer);

    println!("recomposing...");
    recomposer.recompose();
    println!("{:#?}", recomposer);
}
