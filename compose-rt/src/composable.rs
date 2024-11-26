use crate::composer::Cx;

pub trait Composable<N>: 'static {
    fn compose(&self, cx: Cx<N>);
}

impl<N, F> Composable<N> for F
where
    N: 'static,
    F: Fn(Cx<N>) + 'static,
{
    fn compose(&self, cx: Cx<N>) {
        (self)(cx);
    }
}
