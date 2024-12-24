use crate::composer::NodeKey;
use crate::{ComposeNode, Composer};

pub fn print_tree<N, D>(composer: &Composer<N>, node_key: NodeKey, display_fn: D)
where
    N: ComposeNode,
    D: Fn(Option<&N>) -> String,
{
    println!("Root");
    print_node(composer, node_key, &display_fn, false, String::new());
}

fn print_node<N, D>(
    composer: &Composer<N>,
    node_key: NodeKey,
    display_fn: &D,
    has_sibling: bool,
    lines_string: String,
) where
    N: ComposeNode,
    D: Fn(Option<&N>) -> String,
{
    let node = &composer.nodes[node_key];
    let num_children = node.children.len();
    let fork_string = if has_sibling {
        "├── "
    } else {
        "└── "
    };
    let scope = node.scope_id;
    println!(
        "{lines}{fork} {display} [{scope:?} @ {node_key:?}]",
        lines = lines_string,
        fork = fork_string,
        display = display_fn(node.data.as_ref()),
        scope = scope,
        node_key = node_key,
    );
    let bar = if has_sibling { "│   " } else { "    " };
    let new_string = lines_string + bar;
    // Recurse into children
    for (index, child) in node.children.iter().cloned().enumerate() {
        let has_sibling = index < num_children - 1;
        print_node(composer, child, display_fn, has_sibling, new_string.clone());
    }
}
