use compose_rt::{ComposeNode, Composer, Root, Scope, SlotId, State};

#[derive(Default)]
struct TestContext;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestNode(&'static str);

impl ComposeNode for TestNode {
    type Context = TestContext;
}

type TestScope<S> = Scope<S, TestNode>;

struct Host;
struct SlotItem;

fn app(scope: TestScope<Root>, count_state: State<usize, TestNode>) {
    scope.create_node(
        scope.child::<Host>(),
        move |scope| {
            let count = count_state.get();
            let mut subcomposition = scope.subcompose(move |mut registry| {
                for i in 0..count {
                    registry.subcompose::<SlotItem, _, _>(
                        SlotId::from(i as u64),
                        i,
                        move |slot_scope| {
                            let state = slot_scope.use_state(|| 0usize);
                            if state.get() != i {
                                state.set(i);
                            }
                        },
                    );
                }
            });

            if count == 0 {
                subcomposition.subcompose::<SlotItem, _, _>(
                    SlotId::from(u64::MAX),
                    0usize,
                    |slot_scope| {
                        let flag = slot_scope.use_state(|| 0usize);
                        if flag.get() != 1 {
                            flag.set(1);
                        }
                    },
                );
            }
        },
        || (),
        |_, _| TestNode("host"),
        |node, _, _| *node = TestNode("host"),
    );
}

fn slot_keys(recomposer: &mut compose_rt::Recomposer<usize, TestNode>) -> Vec<compose_rt::NodeKey> {
    recomposer.with_composer(|composer| {
        let root = composer.root_node_key();
        composer.nodes[root].children.clone()
    })
}

#[test]
fn subcompose_reuses_and_replaces_slots() {
    let mut recomposer = Composer::compose_with(app, TestContext::default(), || 2usize);

    let initial = slot_keys(&mut recomposer);
    assert_eq!(initial.len(), 2);

    recomposer.recompose_with(3);
    let expanded = slot_keys(&mut recomposer);
    assert_eq!(expanded.len(), 3);
    assert_eq!(expanded[0], initial[0]);
    assert_eq!(expanded[1], initial[1]);

    recomposer.recompose_with(1);
    let reduced = slot_keys(&mut recomposer);
    assert_eq!(reduced.len(), 1);
    assert_eq!(reduced[0], initial[0]);

    recomposer.recompose_with(0);
    let replaced = slot_keys(&mut recomposer);
    assert_eq!(replaced.len(), 1);
    assert_ne!(replaced[0], reduced[0]);

    recomposer.recompose_with(2);
    let reexpanded = slot_keys(&mut recomposer);
    assert_eq!(reexpanded.len(), 2);
}
