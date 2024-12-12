use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

use compose_rt::ScopeId;
use generational_box::{AnyStorage, GenerationalBox, UnsyncStorage};

pub struct Composer<N> {
    composables: HashMap<ScopeId, Box<dyn Fn()>>,
    current_scope: ScopeId,
    ty: PhantomData<N>,
}

impl<N> Debug for Composer<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Composer {{ composables: {:?} }}",
            self.composables.len(),
        )
    }
}

impl<N> Composer<N>
where
    N: 'static,
{
    pub fn new() -> Self {
        Self {
            composables: HashMap::new(),
            current_scope: ScopeId::new(),
            ty: PhantomData,
        }
    }

    pub fn compose<F>(root: F)
    where
        F: Fn(Context<N>),
    {
        let owner = UnsyncStorage::owner();
        let composer = Self::new();
        let ctx = Context {
            composer: owner.insert(composer),
        };
        root(ctx);
    }

    fn start_group(&mut self, scope: ScopeId) {
        self.current_scope = scope;
    }

    fn end_group(&mut self, scope: ScopeId) {
        println!("End group: {:?}", scope);
    }
}

fn div<C>(c: Context<()>, content: C)
where
    C: Fn(Context<()>) + 'static,
{
    let scope = ScopeId::new();
    c.create_scope(scope, content);
}

struct Context<N> {
    composer: GenerationalBox<Composer<N>>,
}

impl<N> Context<N>
where
    N: 'static,
{
    pub fn create_scope<C>(&self, scope: ScopeId, content: C)
    where
        C: Fn(Context<N>) + 'static,
    {
        let ctx = *self;
        let composable = move || {
            ctx.composer.write().start_group(scope);
            content(ctx);
            ctx.composer.write().end_group(scope);
        };
        composable();
        self.composer
            .write()
            .composables
            .entry(scope)
            .or_insert(Box::new(composable));
    }
}

impl<N> Clone for Context<N> {
    fn clone(&self) -> Self {
        Self {
            composer: self.composer.clone(),
        }
    }
}
impl<N> Copy for Context<N> {}

fn app(c: Context<()>) {
    div(c, |c| {
        println!("div 1");

        div(c, |c| {
            println!("div 2");
        })
    })
}

fn main() {
    Composer::compose(app);
}
