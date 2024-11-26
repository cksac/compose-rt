use compose_rt::{html::Cx, key, Composer};

fn app(cx: Cx) {
    cx.body(|cx, s| {
        cx.div(s, |cx, s| {
            let count = cx.use_state(|| 0);
            cx.text(s, "start");
            if count.get() == 0 {
                cx.text(s, "loading...");
                count.set(1)
            } else {
                cx.text(s, "loaded");
                cx.text(s, move || count.with(|c| format!("count: {}", c)));
                for i in 0..count.get() {
                    key(cx, i, || {
                        cx.text(s, move || format!("Item {}", i));
                    });
                }
                count.set(0);
            }
            cx.text(s, "end");
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
