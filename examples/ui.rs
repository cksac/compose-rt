use std::cell::RefCell;
use std::rc::Rc;

use compose_rt::{ComposeNode, Composer, Root, Scope, SlotId, SubcomposeScope};

#[derive(Debug)]
struct UiNode {
    name: String,
    width: usize,
    min_width: usize,
}

impl ComposeNode for UiNode {
    type Context = ();
}

type UiScope<S> = Scope<S, UiNode>;

#[derive(Clone, Copy, Debug, Default)]
struct LayoutConstraint {
    min_width: usize,
}

impl LayoutConstraint {
    fn new(min_width: usize) -> Self {
        Self { min_width }
    }
}

struct ColumnNode;
struct TextNode;
struct ButtonNode;
struct ColumnSlot;

#[derive(Clone)]
enum ColumnRunContext {
    Measure { widths: Rc<RefCell<Vec<usize>>> },
    Render,
}

trait ColumnDsl {
    fn text(&mut self, constraint: LayoutConstraint, text: String);

    fn button(&mut self, constraint: LayoutConstraint, label: String);
}

struct ColumnRunner<S> {
    scope: SubcomposeScope<S, UiNode, ColumnRunContext>,
}

impl<S: 'static> ColumnRunner<S> {
    fn new(scope: SubcomposeScope<S, UiNode, ColumnRunContext>) -> Self {
        Self { scope }
    }

    fn render_text(&mut self, value: String, width: usize, constraint: LayoutConstraint) {
        let scope = self.scope.scope();
        let text_scope = scope.child::<TextNode>();
        let display = Rc::new(value);
        let display_for_input = display.clone();
        scope.create_node(
            text_scope,
            |_| {},
            move || (display_for_input.clone(), width, constraint),
            move |(value, width, constraint), _| UiNode {
                name: format!("Text(\"{}\")", value.as_ref()),
                width,
                min_width: constraint.min_width,
            },
            move |node, (value, width, constraint), _| {
                node.name = format!("Text(\"{}\")", value.as_ref());
                node.width = width;
                node.min_width = constraint.min_width;
            },
        );
    }

    fn render_button(&mut self, value: String, width: usize, constraint: LayoutConstraint) {
        let scope = self.scope.scope();
        let button_scope = scope.child::<ButtonNode>();
        let display = Rc::new(value);
        let display_for_input = display.clone();
        scope.create_node(
            button_scope,
            |_| {},
            move || (display_for_input.clone(), width, constraint),
            move |(value, width, constraint), _| UiNode {
                name: format!("Button(\"{}\")", value.as_ref()),
                width,
                min_width: constraint.min_width,
            },
            move |node, (value, width, constraint), _| {
                node.name = format!("Button(\"{}\")", value.as_ref());
                node.width = width;
                node.min_width = constraint.min_width;
            },
        );
    }
}

impl<S: 'static> ColumnDsl for ColumnRunner<S> {
    fn text(&mut self, constraint: LayoutConstraint, text: String) {
        let width = text_width(&text);
        if let ColumnRunContext::Measure { widths } = self.scope.context() {
            widths.borrow_mut().push(width);
        } else {
            let resolved_width = width.max(constraint.min_width);
            self.render_text(text, resolved_width, constraint);
        }
    }

    fn button(&mut self, constraint: LayoutConstraint, label: String) {
        let width = button_width(&label);
        if let ColumnRunContext::Measure { widths } = self.scope.context() {
            widths.borrow_mut().push(width);
        } else {
            let resolved_width = width.max(constraint.min_width);
            self.render_button(label, resolved_width, constraint);
        }
    }
}

fn measured_column<S, C>(scope: UiScope<S>, constraint: LayoutConstraint, content: C)
where
    S: 'static,
    C: Fn(LayoutConstraint, &mut dyn ColumnDsl) + Clone + 'static,
{
    let min_width_state = scope.use_state(|| 0usize);
    let column_scope = scope.child::<ColumnNode>();
    scope.create_node(
        column_scope,
        {
            let content = content.clone();
            move |scope| {
                let mut c = constraint.clone();
                let metrics = Rc::new(RefCell::new(Vec::new()));
                let measure_content = content.clone();
                let render_content = content.clone();
                let min_width_state = min_width_state;
                scope.subcompose(move |mut registry| {
                    let mut constraint = c;
                    metrics.borrow_mut().clear();
                    let metrics_for_measure = metrics.clone();
                    let measure_content = measure_content.clone();
                    registry.subcompose::<ColumnSlot, _, _>(
                        SlotId::from("column"),
                        ColumnRunContext::Measure {
                            widths: metrics_for_measure.clone(),
                        },
                        move |slot| {
                            let mut runner = ColumnRunner::new(slot);
                            let callback = measure_content.clone();
                            callback(constraint.clone(), &mut runner as &mut dyn ColumnDsl);
                        },
                    );

                    let max_width = metrics.borrow().iter().copied().max().unwrap_or(0);
                    if max_width != min_width_state.get() {
                        min_width_state.set(max_width);
                    }

                    let min_width = min_width_state.get();
                    constraint.min_width = min_width;
                    let render_content = render_content.clone();
                    registry.subcompose::<ColumnSlot, _, _>(
                        SlotId::from("column"),
                        ColumnRunContext::Render,
                        move |slot| {
                            let mut runner = ColumnRunner::new(slot);
                            let callback = render_content.clone();
                            callback(constraint.clone(), &mut runner as &mut dyn ColumnDsl);
                        },
                    );
                });
            }
        },
        move || constraint,
        |constraint, _| UiNode {
            name: "Column".to_string(),
            width: constraint.min_width,
            min_width: constraint.min_width,
        },
        move |node, constraint, _| {
            node.name = "Column".to_string();
            node.width = constraint.min_width;
            node.min_width = constraint.min_width;
        },
    );
}

fn text_width(text: &str) -> usize {
    text.chars().count()
}

fn button_width(label: &str) -> usize {
    text_width(label) + 4
}

fn app(scope: UiScope<Root>) {
    let constraint = LayoutConstraint::new(0);
    measured_column(scope, constraint, |constraint, column| {
        column.text(constraint, "Title".to_string());
        column.button(constraint, "Tap me".to_string());
        column.text(
            constraint,
            "This is a much longer line that drives the minimum width".to_string(),
        );
        column.text(constraint, "Footer".to_string());
    });
}

fn main() {
    let recomposer = Composer::compose(app, ());
    recomposer.print_tree();
}
