use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_rt::{ComposeNode, Composer, Root, Scope, SlotId, State, SubcomposeScope};

#[derive(Default)]
struct LayoutRuntime;

#[derive(Debug, Clone)]
struct ElementNode {
    name: String,
    width: usize,
}

impl ComposeNode for ElementNode {
    type Context = LayoutRuntime;
}

type DemoScope<S> = Scope<S, ElementNode>;

struct ColumnNode;
struct BoxNode;
struct TextNode;
struct SpacerNode;
struct MeasureSlot;
struct RenderSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutPhase {
    Measure,
    Place,
}

#[derive(Clone)]
struct ColumnContext {
    phase: LayoutPhase,
    resize: bool,
    metrics: Rc<RefCell<Vec<usize>>>,
    max_width: Rc<Cell<usize>>,
}

trait ColumnDsl {
    fn red_box(&mut self, text: &str, padding_top: usize);
}

struct ColumnRunner<S> {
    scope: SubcomposeScope<S, ElementNode, ColumnContext>,
}

impl<S> ColumnRunner<S> {
    fn new(scope: SubcomposeScope<S, ElementNode, ColumnContext>) -> Self {
        Self { scope }
    }
}

impl<S: 'static> ColumnDsl for ColumnRunner<S> {
    fn red_box(&mut self, text: &str, padding_top: usize) {
        let text_owned = text.to_string();
        let measured_width = max_line_width(&text_owned);
        let ctx = self.scope.context();
        let phase = ctx.phase;
        let resize = ctx.resize;
        let metrics = ctx.metrics.clone();
        let max_width = ctx.max_width.clone();

        if phase == LayoutPhase::Measure {
            metrics.borrow_mut().push(measured_width);
            return;
        }

        let should_resize = phase == LayoutPhase::Place && resize;
        let target_width = if should_resize {
            max_width.get()
        } else {
            measured_width
        };

        let display_text = if should_resize {
            pad_lines(&text_owned, target_width)
        } else {
            text_owned.clone()
        };

        let width = max_line_width(&display_text);
        let scope = self.scope.scope();
        let box_scope = scope.child::<BoxNode>();
        let display_shared = Rc::new(display_text.clone());
        scope.create_node(
            box_scope,
            {
                let display_shared = display_shared.clone();
                move |box_scope| {
                    if padding_top > 0 {
                        let spacer_scope = box_scope.child::<SpacerNode>();
                        box_scope.create_node(
                            spacer_scope,
                            |_| {},
                            move || padding_top,
                            |padding, _| ElementNode {
                                name: format!("Spacer(top={padding})"),
                                width: 0,
                            },
                            |node, padding, _| {
                                node.name = format!("Spacer(top={padding})");
                                node.width = 0;
                            },
                        );
                    }

                    let display_for_text = display_shared.clone();
                    compose_text_node(box_scope, width, display_for_text);
                }
            },
            move || (padding_top, width),
            move |(padding_top, width), _| ElementNode {
                name: format!("Box(padding_top={padding_top})"),
                width,
            },
            move |node, (padding_top, width), _| {
                node.name = format!("Box(padding_top={padding_top})");
                node.width = width;
            },
        );
    }
}

fn compose_text_node<S: 'static>(
    box_scope: Scope<S, ElementNode>,
    width: usize,
    display_text: Rc<String>,
) {
    let text_scope = box_scope.child::<TextNode>();
    let input_value = display_text.clone();
    box_scope.create_node(
        text_scope,
        |_| {},
        move || input_value.clone(),
        move |value: Rc<String>, _| ElementNode {
            name: format!("Text[`{}`]", value.as_ref()),
            width,
        },
        move |node, value: Rc<String>, _| {
            node.name = format!("Text[`{}`]", value.as_ref());
            node.width = width;
        },
    );
}

fn subcompose_layout_demo(scope: DemoScope<Root>, resize_state: State<bool, ElementNode>) {
    resize_width_column(
        scope,
        resize_state,
        Rc::new(move |column: &mut dyn ColumnDsl| {
            column.red_box("Hello", 0);
            column.red_box("This is a long message \nand it's longer", 1);
        }),
    );
}

fn resize_width_column(
    scope: DemoScope<Root>,
    resize_state: State<bool, ElementNode>,
    content: Rc<dyn Fn(&mut dyn ColumnDsl)>,
) {
    let column_scope = scope.child::<ColumnNode>();
    scope.create_node(
        column_scope,
        {
            let content = content.clone();
            let resize_state = resize_state;
            move |scope| {
                let resize = resize_state.get();
                let metrics = Rc::new(RefCell::new(Vec::new()));
                let max_width = Rc::new(Cell::new(0usize));
                let measure_content = content.clone();
                let render_content = content.clone();
                scope.subcompose(move |mut registry| {
                    metrics.borrow_mut().clear();
                    let measure_ctx = ColumnContext {
                        phase: LayoutPhase::Measure,
                        resize,
                        metrics: metrics.clone(),
                        max_width: max_width.clone(),
                    };
                    let measure_fn = measure_content.clone();
                    registry.subcompose::<MeasureSlot, _, _>(
                        SlotId::from("measure"),
                        measure_ctx,
                        move |slot| {
                            let mut column_scope = ColumnRunner::new(slot);
                            let callback = measure_fn.clone();
                            callback.as_ref()(&mut column_scope);
                        },
                    );

                    let max = metrics.borrow().iter().copied().max().unwrap_or(0);
                    max_width.set(max);
                    metrics.borrow_mut().clear();

                    let render_ctx = ColumnContext {
                        phase: LayoutPhase::Place,
                        resize,
                        metrics: metrics.clone(),
                        max_width: max_width.clone(),
                    };
                    let render_fn = render_content.clone();
                    registry.subcompose::<RenderSlot, _, _>(
                        SlotId::from("render"),
                        render_ctx,
                        move |slot| {
                            let mut column_scope = ColumnRunner::new(slot);
                            let callback = render_fn.clone();
                            callback.as_ref()(&mut column_scope);
                        },
                    );
                });
            }
        },
        move || resize_state.get(),
        |resize, _| ElementNode {
            name: format!("ResizeWidthColumn(resize={resize})"),
            width: 0,
        },
        |node, resize, _| {
            node.name = format!("ResizeWidthColumn(resize={resize})");
            node.width = 0;
        },
    );
}

fn max_line_width(text: &str) -> usize {
    let mut lines = text.lines().peekable();
    if lines.peek().is_none() {
        return text.chars().count();
    }
    lines.map(|line| line.chars().count()).max().unwrap_or(0)
}

fn pad_lines(text: &str, width: usize) -> String {
    if width == 0 {
        return text.to_string();
    }
    let mut lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("");
    }
    lines
        .into_iter()
        .map(|line| {
            let len = line.chars().count();
            if len >= width {
                line.to_string()
            } else {
                let padding = " ".repeat(width - len);
                format!("{line}{padding}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn main() {
    let mut recomposer =
        Composer::compose_with(subcompose_layout_demo, LayoutRuntime::default(), || true);

    println!("== resize = true ==");
    recomposer.print_tree();

    println!("\n== resize = false ==");
    recomposer.recompose_with(false);
    recomposer.print_tree();
}
