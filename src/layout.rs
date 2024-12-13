use std::fmt::Debug;

use taffy::{
    compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
    compute_hidden_layout, compute_leaf_layout, style, AvailableSpace, Cache, CacheTree, Display,
    FlexDirection, Layout, LayoutBlockContainer, LayoutFlexboxContainer, LayoutGridContainer,
    LayoutPartialTree, NodeId, PrintTree, RoundTree, RunMode, Size, Style, TraversePartialTree,
    TraverseTree,
};

use crate::{Composer, Recomposer, ScopeId};

pub struct LayoutNode<T> {
    style: Style,
    unrounded_layout: Layout,
    final_layout: Layout,
    cache: Cache,
    context: Option<T>,
}

impl<T> LayoutNode<T> {
    pub fn new(context: Option<T>, style: Style) -> Self {
        Self {
            style,
            unrounded_layout: Layout::new(),
            final_layout: Layout::new(),
            cache: Cache::new(),
            context,
        }
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.cache.clear()
    }
}

impl From<ScopeId> for NodeId {
    fn from(id: ScopeId) -> Self {
        NodeId::new(id.0)
    }
}

impl From<NodeId> for ScopeId {
    fn from(id: NodeId) -> Self {
        ScopeId(id.into())
    }
}

pub struct ChildIter<'a>(core::slice::Iter<'a, ScopeId>);
impl Iterator for ChildIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().copied().map(NodeId::from)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TaffyConfig {
    /// Whether to round layout values
    pub(crate) use_rounding: bool,
}

impl Default for TaffyConfig {
    fn default() -> Self {
        Self { use_rounding: true }
    }
}

pub struct TaffyTree<'a, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    composer: &'a mut Composer<LayoutNode<T>>,
    config: TaffyConfig,
    measure_function: M,
}

impl<'a, T, M> TaffyTree<'a, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    pub fn new(composer: &'a mut Composer<LayoutNode<T>>, measure_function: M) -> Self {
        Self {
            composer,
            config: TaffyConfig::default(),
            measure_function,
        }
    }

    pub fn enable_rounding(&mut self) {
        self.config.use_rounding = true;
    }

    pub fn disable_rounding(&mut self) {
        self.config.use_rounding = false;
    }
}

impl<T, M> TraversePartialTree for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    #[inline(always)]

    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        ChildIter(self.composer.nodes[&parent_node_id.into()].children.iter())
    }

    #[inline(always)]

    fn child_count(&self, parent_node_id: NodeId) -> usize {
        self.composer.nodes[&parent_node_id.into()].children.len()
    }

    #[inline(always)]
    fn get_child_id(&self, parent_node_id: NodeId, child_index: usize) -> NodeId {
        self.composer.nodes[&parent_node_id.into()].children[child_index].into()
    }
}

impl<T, M> TraverseTree for TaffyTree<'_, T, M> where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>
{
}

impl<T, M> CacheTree for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: taffy::Size<Option<f32>>,
        available_space: taffy::Size<taffy::AvailableSpace>,
        run_mode: taffy::RunMode,
    ) -> Option<taffy::LayoutOutput> {
        self.composer.nodes[&node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: taffy::Size<Option<f32>>,
        available_space: taffy::Size<taffy::AvailableSpace>,
        run_mode: taffy::RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.composer
            .nodes
            .get_mut(&node_id.into())
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .cache
            .store(known_dimensions, available_space, run_mode, layout_output)
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.composer
            .nodes
            .get_mut(&node_id.into())
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .cache
            .clear();
    }
}

impl<T, M> PrintTree for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    #[inline(always)]
    fn get_debug_label(&self, node_id: NodeId) -> &'static str {
        let node = self.composer.nodes[&node_id.into()].data.as_ref().unwrap();
        let display = node.style.display;
        let num_children = self.child_count(node_id);

        match (num_children, display) {
            (_, Display::None) => "NONE",
            (0, _) => "LEAF",
            (_, Display::Block) => "BLOCK",
            (_, Display::Flex) => match node.style.flex_direction {
                FlexDirection::Row | FlexDirection::RowReverse => "FLEX ROW",
                FlexDirection::Column | FlexDirection::ColumnReverse => "FLEX COL",
            },
            (_, Display::Grid) => "GRID",
        }
    }

    fn get_final_layout(&self, node_id: NodeId) -> &Layout {
        if self.config.use_rounding {
            &self.composer.nodes[&node_id.into()]
                .data
                .as_ref()
                .unwrap()
                .final_layout
        } else {
            &self.composer.nodes[&node_id.into()]
                .data
                .as_ref()
                .unwrap()
                .unrounded_layout
        }
    }
}

impl<T, M> LayoutPartialTree for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.composer.nodes[&node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .style
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.composer
            .nodes
            .get_mut(&node_id.into())
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .unrounded_layout = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node: NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        // If RunMode is PerformHiddenLayout then this indicates that an ancestor node is `Display::None`
        // and thus that we should lay out this node using hidden layout regardless of it's own display style.
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, node);
        }

        // We run the following wrapped in "compute_cached_layout", which will check the cache for an entry matching the node and inputs and:
        //   - Return that entry if exists
        //   - Else call the passed closure (below) to compute the result
        //
        // If there was no cache match and a new result needs to be computed then that result will be added to the cache
        compute_cached_layout(self, node, inputs, |tree, node, inputs| {
            let display_mode = tree.composer.nodes[&node.into()]
                .data
                .as_ref()
                .unwrap()
                .style
                .display;
            let has_children = tree.child_count(node) > 0;

            // Dispatch to a layout algorithm based on the node's display style and whether the node has children or not.
            match (display_mode, has_children) {
                (Display::None, _) => compute_hidden_layout(tree, node),
                (Display::Block, true) => compute_block_layout(tree, node, inputs),
                (Display::Flex, true) => compute_flexbox_layout(tree, node, inputs),
                (Display::Grid, true) => compute_grid_layout(tree, node, inputs),
                (_, false) => {
                    let node_key = node.into();
                    let data = tree
                        .composer
                        .nodes
                        .get_mut(&node_key)
                        .unwrap()
                        .data
                        .as_mut()
                        .unwrap();
                    let style = &data.style;
                    let node_context = data.context.as_mut();
                    let measure_function = |known_dimensions, available_space| {
                        (tree.measure_function)(
                            known_dimensions,
                            available_space,
                            node,
                            node_context,
                            style,
                        )
                    };
                    compute_leaf_layout(inputs, style, measure_function)
                }
            }
        })
    }
}

impl<T, M> LayoutBlockContainer for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    #[inline(always)]
    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<T, M> LayoutFlexboxContainer for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.composer.nodes[&node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .style
    }

    #[inline(always)]
    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.composer.nodes[&child_node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .style
    }
}

impl<T, M> LayoutGridContainer for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    #[inline(always)]
    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.composer.nodes[&node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .style
    }

    #[inline(always)]
    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        &self.composer.nodes[&child_node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .style
    }
}

impl<T, M> RoundTree for TaffyTree<'_, T, M>
where
    M: FnMut(Size<Option<f32>>, Size<AvailableSpace>, NodeId, Option<&mut T>, &Style) -> Size<f32>,
{
    #[inline(always)]
    fn get_unrounded_layout(&self, node_id: NodeId) -> &Layout {
        &self.composer.nodes[&node_id.into()]
            .data
            .as_ref()
            .unwrap()
            .unrounded_layout
    }

    #[inline(always)]
    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.composer
            .nodes
            .get_mut(&node_id.into())
            .unwrap()
            .data
            .as_mut()
            .unwrap()
            .final_layout = *layout;
    }
}
