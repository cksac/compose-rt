use std::cell::RefCell;
use std::rc::Rc;

use compose_rt::{ComposeNode, Composer, Root, Scope, SlotId, SubcomposeScope};

#[derive(Debug)]
struct LayoutNode {
    widget: String,
    width: usize,
    min_width: usize,
}

impl ComposeNode for LayoutNode {
    type Context = ();
}

type UiScope<S> = Scope<S, LayoutNode>;

#[derive(Clone, Copy, Debug, Default)]
struct Modifier {
    min_width: usize,
}

impl Modifier {
    fn new(min_width: usize) -> Self {
        Self { min_width }
    }
}

struct ColumnNode;
struct TextNode;
struct ButtonNode;
struct ColumnSlot;

#[derive(Clone)]
enum LayoutContext {
    Measure { widths: Rc<RefCell<Vec<usize>>> },
    Render,
}

trait UiDsl {
    fn text(&mut self, modifier: Modifier, text: String);

    fn button(&mut self, modifier: Modifier, label: String);
}

struct Ui<S> {
    scope: SubcomposeScope<S, LayoutNode, LayoutContext>,
}

impl<S: 'static> Ui<S> {
    fn new(scope: SubcomposeScope<S, LayoutNode, LayoutContext>) -> Self {
        Self { scope }
    }

    fn render_text(&mut self, value: String, modifier: Modifier) {
        let scope = self.scope.scope();
        let text_scope = scope.child::<TextNode>();
        let display = Rc::new(value);
        let display_for_input = display.clone();
        scope.create_node(
            text_scope,
            |_| {},
            move || (display_for_input.clone(), modifier),
            move |(value, modifier), _| LayoutNode {
                widget: format!("Text(\"{}\")", &value),
                width: text_width(&value),
                min_width: modifier.min_width,
            },
            move |node, (value, modifier), _| {
                node.widget = format!("Text(\"{}\")", &value);
                node.width = text_width(&value);
                node.min_width = modifier.min_width;
            },
        );
    }

    fn render_button(&mut self, value: String, modifier: Modifier) {
        let scope = self.scope.scope();
        let button_scope = scope.child::<ButtonNode>();
        let display = Rc::new(value);
        let display_for_input = display.clone();
        scope.create_node(
            button_scope,
            |_| {},
            move || (display_for_input.clone(), modifier),
            move |(value, constraint), _| LayoutNode {
                widget: format!("Button(\"{}\")", &value),
                width: text_width(&value),
                min_width: constraint.min_width,
            },
            move |node, (value, constraint), _| {
                node.widget = format!("Button(\"{}\")", &value);
                node.width = text_width(&value);
                node.min_width = constraint.min_width;
            },
        );
    }
}

impl<S: 'static> UiDsl for Ui<S> {
    fn text(&mut self, constraint: Modifier, text: String) {
        if let LayoutContext::Measure { widths } = self.scope.context() {
            widths.borrow_mut().push(text_width(&text));
        } else {
            self.render_text(text, constraint);
        }
    }

    fn button(&mut self, constraint: Modifier, label: String) {
        if let LayoutContext::Measure { widths } = self.scope.context() {
            widths.borrow_mut().push(text_width(&label));
        } else {
            self.render_button(label, constraint);
        }
    }
}

fn measured_column<S, C>(scope: UiScope<S>, constraint: Modifier, content: C)
where
    S: 'static,
    C: Fn(Modifier, &mut dyn UiDsl) + Clone + 'static,
{
    let column_scope = scope.child::<ColumnNode>();
    scope.create_node(
        column_scope,
        {
            let content = content.clone();
            move |scope| {
                let c = constraint.clone();
                let metrics = Rc::new(RefCell::new(Vec::new()));
                let measure_content = content.clone();
                let render_content = content.clone();
                scope.subcompose(move |mut registry| {
                    let mut constraint = c;
                    metrics.borrow_mut().clear();
                    let metrics_for_measure = metrics.clone();
                    let measure_content = measure_content.clone();
                    registry.subcompose::<ColumnSlot, _, _>(
                        SlotId::from("measure"),
                        LayoutContext::Measure {
                            widths: metrics_for_measure.clone(),
                        },
                        move |slot| {
                            let mut runner = Ui::new(slot);
                            let callback = measure_content.clone();
                            callback(constraint.clone(), &mut runner as &mut dyn UiDsl);
                        },
                    );
                    constraint.min_width = metrics.borrow().iter().copied().max().unwrap_or(0);
                    let render_content = render_content.clone();
                    registry.subcompose::<ColumnSlot, _, _>(
                        SlotId::from("render"),
                        LayoutContext::Render,
                        move |slot| {
                            let mut runner = Ui::new(slot);
                            let callback = render_content.clone();
                            callback(constraint.clone(), &mut runner as &mut dyn UiDsl);
                        },
                    );
                });
            }
        },
        move || constraint,
        |modifier, _| LayoutNode {
            widget: "Column".to_string(),
            width: 5,
            min_width: modifier.min_width,
        },
        move |node, constraint, _| {
            node.min_width = constraint.min_width;
        },
    );
}

fn text_width(text: &str) -> usize {
    text.chars().count()
}

fn app(scope: UiScope<Root>) {
    let constraint = Modifier::new(0);
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
