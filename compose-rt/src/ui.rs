// use taffy::NodeId;

// use crate::{composer::Group, Composer};

// pub struct UiCompoer {
//     pub composer: Composer,
// }

// pub struct ChildIter<'a>(std::slice::Iter<'a, Group>);
// impl Iterator for ChildIter<'_> {
//     type Item = NodeId;
//     fn next(&mut self) -> Option<Self::Item> {
//         self.0
//             .next()
//             .map(|c| NodeId::from(c as *const Group as usize))
//     }
// }

// impl taffy::TraversePartialTree for UiCompoer {
//     type ChildIter<'a> = ChildIter<'a>;

//     fn child_ids(&self, parent_node_id: taffy::NodeId) -> Self::ChildIter<'_> {
//         todo!()
//     }

//     fn child_count(&self, parent_node_id: taffy::NodeId) -> usize {
//         todo!()
//     }

//     fn get_child_id(&self, parent_node_id: taffy::NodeId, child_index: usize) -> taffy::NodeId {
//         todo!()
//     }
// }

// impl taffy::TraverseTree for UiCompoer {}

// impl taffy::LayoutPartialTree for UiCompoer {
//     type CoreContainerStyle<'a>
//         = &'a taffy::Style
//     where
//         Self: 'a;

//     type CacheMut<'b>
//         = &'b mut taffy::Cache
//     where
//         Self: 'b;

//     fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
//         todo!()
//     }

//     fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &taffy::Layout) {
//         todo!()
//     }

//     fn get_cache_mut(&mut self, node_id: NodeId) -> Self::CacheMut<'_> {
//         todo!()
//     }

//     fn compute_child_layout(
//         &mut self,
//         node_id: NodeId,
//         inputs: taffy::LayoutInput,
//     ) -> taffy::LayoutOutput {
//         todo!()
//     }
// }

// impl taffy::LayoutFlexboxContainer for UiCompoer {
//     type FlexboxContainerStyle<'a>
//         = &'a taffy::Style
//     where
//         Self: 'a;

//     type FlexboxItemStyle<'a>
//         = &'a taffy::Style
//     where
//         Self: 'a;

//     fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
//         todo!()
//     }

//     fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
//         todo!()
//     }
// }

// impl taffy::LayoutGridContainer for UiCompoer {
//     type GridContainerStyle<'a>
//         = &'a taffy::Style
//     where
//         Self: 'a;

//     type GridItemStyle<'a>
//         = &'a taffy::Style
//     where
//         Self: 'a;

//     fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
//         todo!()
//     }

//     fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
//         todo!()
//     }
// }

// impl taffy::RoundTree for UiCompoer {
//     fn get_unrounded_layout(&self, node_id: NodeId) -> &taffy::Layout {
//         todo!()
//     }

//     fn set_final_layout(&mut self, node_id: NodeId, layout: &taffy::Layout) {
//         todo!()
//     }
// }

// impl taffy::PrintTree for UiCompoer {
//     fn get_debug_label(&self, node_id: NodeId) -> &'static str {
//         todo!()
//     }

//     fn get_final_layout(&self, node_id: NodeId) -> &taffy::Layout {
//         todo!()
//     }
// }

// impl UiCompoer {
//     pub fn compute_layout(
//         &mut self,
//         root: usize,
//         available_space: taffy::Size<taffy::AvailableSpace>,
//         use_rounding: bool,
//     ) {
//         taffy::compute_root_layout(self, NodeId::from(root), available_space);
//         if use_rounding {
//             taffy::round_layout(self, NodeId::from(root))
//         }
//     }

//     pub fn print_tree(&mut self, root: usize) {
//         taffy::print_tree(self, NodeId::from(root));
//     }
// }

// #[cfg(test)]
// mod tests {
//     use taffy::prelude::*;

//     use super::*;

//     #[test]
//     fn test_ui_compoer() {
//         let composer = Composer::new();
//         let mut ui_compoer = UiCompoer { composer };

//         let root_id = 0;
//         // Compute layout and print result
//         ui_compoer.compute_layout(root_id, Size::MAX_CONTENT, true);
//         ui_compoer.print_tree(root_id);
//     }
// }
