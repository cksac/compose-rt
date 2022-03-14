# compose-derive
transform function into compose function

```rust
#[compose]
pub fn MoviesScreen(movies: &Vec<Movie>) {
    Column(cx, |cx| {
        for movie in movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie));
        }
    });
}

// will become after expand
#[track_caller]
pub fn MoviesScreen(cx: &mut compose_rt::Composer, movies: &Vec<Movie>) {
    Column(cx, |cx| {
        for movie in movies {
            cx.tag(movie.id, |cx| MovieOverview(cx, &movie));
        }
    });
}
```