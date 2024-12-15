use std::fmt::Debug;

use crate::composer::Node;
use crate::{ComposeNode, Composer, ScopeId};

pub fn print_tree<N, D>(composer: &Composer<N>, root: ScopeId, display_fn: D)
where
    N: ComposeNode,
    D: Fn(Option<&N>) -> String,
{
    println!("Root");
    print_node(composer, &root, &display_fn, false, String::new());
}

/// Recursive function that prints each node in the tree
fn print_node<N, D>(
    composer: &Composer<N>,
    scope: &ScopeId,
    display_fn: &D,
    has_sibling: bool,
    lines_string: String,
) where
    N: ComposeNode,
    D: Fn(Option<&N>) -> String,
{
    let node = &composer.nodes[scope];
    let num_children = node.children.len();
    let fork_string = if has_sibling {
        "├── "
    } else {
        "└── "
    };
    let key: u64 = (*scope).into();
    println!(
        "{lines}{fork} {key:<20}: {display}",
        lines = lines_string,
        fork = fork_string,
        display = display_fn(node.data.as_ref()),
        key = key,
    );
    let bar = if has_sibling { "│   " } else { "    " };
    let new_string = lines_string + bar;
    // Recurse into children
    for (index, child) in node.children.iter().enumerate() {
        let has_sibling = index < num_children - 1;
        print_node(composer, child, display_fn, has_sibling, new_string.clone());
    }
}
