use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use compose_rt::{html::Cx, key, Composer};

fn app(cx: Cx, n: usize) {
    cx.body(move |cx, s| {
        cx.div(s, move |cx, s| {
            let count = cx.use_state(|| 0);
            cx.text(s, "start");
            if count.get() == 0 {
                cx.text(s, "loading...");
                count.set(n);
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

fn run_app(n: usize) {
    let recomposer = Composer::compose(move |cx| app(cx, n));
    recomposer.recompose();
    recomposer.recompose();
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("count 100", |b| b.iter(|| run_app(black_box(100))));
    c.bench_function("count 10000", |b| b.iter(|| run_app(black_box(10000))));
    c.bench_function("count 100000", |b| b.iter(|| run_app(black_box(100000))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
