use std::ops::{Index, IndexMut};

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ExceptionRegion, Instruction, Item, Label, Operand, block_has_fallthrough, block_jump_target,
    ends_scope,
};

/// A stable handle to a basic block.
///
/// Blocks live in an append-only arena. Reordering or removing a block from the
/// emitted order therefore never changes the identity of any other block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct BasicBlockId(usize);

impl BasicBlockId {
    pub(super) const fn index(self) -> usize {
        self.0
    }
}

/// A temporary cursor into a block's item list.
///
/// Cursors are intentionally pass-local: inserting or removing an item can
/// shift later offsets in the same block, while the block identity stays
/// stable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ItemPosition {
    block: BasicBlockId,
    offset: usize,
}

#[derive(Clone, Debug)]
pub(super) struct BasicBlock {
    items: Vec<Item>,
}

impl BasicBlock {
    fn new(items: Vec<Item>) -> Self {
        debug_assert!(!items.is_empty());
        Self { items }
    }

    pub(super) fn items(&self) -> &[Item] {
        &self.items
    }

    pub(super) fn items_mut(&mut self) -> &mut Vec<Item> {
        &mut self.items
    }

    pub(super) fn first_label(&self) -> Option<Label> {
        self.items.iter().find_map(|item| match item {
            Item::Label(label) => Some(*label),
            Item::Instruction(_) => None,
        })
    }

    pub(super) fn last_instruction(&self) -> Option<&Instruction> {
        self.items.iter().rev().find_map(|item| match item {
            Item::Instruction(instruction) => Some(instruction),
            Item::Label(_) => None,
        })
    }
}

/// The assembler's block-aware intermediate representation.
///
/// `blocks` is an append-only arena and `order` is the current emission order.
/// Keeping those concepts separate lets CFG passes reorder or duplicate blocks
/// without invalidating identities captured by data-flow state.
#[derive(Debug)]
pub(super) struct BlockGraph {
    blocks: Vec<BasicBlock>,
    order: Vec<BasicBlockId>,
    label_blocks: FxHashMap<Label, BasicBlockId>,
}

impl BlockGraph {
    fn partition_items(
        items: Vec<Item>,
        preserved_block_boundaries: &FxHashSet<Label>,
    ) -> Vec<Vec<Item>> {
        let mut blocks = Vec::new();
        let mut block = Vec::new();
        for item in items {
            if let Item::Label(label) = item
                && (block
                    .iter()
                    .any(|item| matches!(item, Item::Instruction(_)))
                    || (!block.is_empty() && preserved_block_boundaries.contains(&label)))
            {
                blocks.push(std::mem::take(&mut block));
            }
            let ends_block = matches!(item, Item::Instruction(instruction) if
                !matches!(instruction.operand, Operand::Value(_))
                    || ends_scope(instruction.opcode));
            block.push(item);
            if ends_block {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }
        blocks
    }

    pub(super) fn from_items(
        items: Vec<Item>,
        preserved_block_boundaries: &FxHashSet<Label>,
    ) -> Self {
        let mut graph = Self {
            blocks: Vec::new(),
            order: Vec::new(),
            label_blocks: FxHashMap::default(),
        };
        for block in Self::partition_items(items, preserved_block_boundaries) {
            graph.push_block(block);
        }
        graph.rebuild_label_index();
        graph
    }

    pub(super) fn len(&self) -> usize {
        self.order.len()
    }

    pub(super) fn order(&self) -> &[BasicBlockId] {
        &self.order
    }

    pub(super) fn order_mut(&mut self) -> &mut Vec<BasicBlockId> {
        &mut self.order
    }

    pub(super) fn block_at(&self, position: usize) -> BasicBlockId {
        self.order[position]
    }

    pub(super) fn label_block(&self, label: Label) -> Option<BasicBlockId> {
        self.label_blocks.get(&label).copied()
    }

    pub(super) fn push_block(&mut self, items: Vec<Item>) -> BasicBlockId {
        self.insert_block(self.order.len(), items)
    }

    pub(super) fn insert_block(&mut self, position: usize, items: Vec<Item>) -> BasicBlockId {
        let block = self.allocate_block(items);
        self.order.insert(position, block);
        block
    }

    pub(super) fn allocate_block(&mut self, items: Vec<Item>) -> BasicBlockId {
        let block = BasicBlockId(self.blocks.len());
        self.blocks.push(BasicBlock::new(items));
        let labels = self[block]
            .items()
            .iter()
            .filter_map(|item| match item {
                Item::Label(label) => Some(*label),
                Item::Instruction(_) => None,
            })
            .collect::<Vec<_>>();
        for label in labels {
            let previous = self.label_blocks.insert(label, block);
            debug_assert!(previous.is_none(), "label is bound more than once");
        }
        block
    }

    pub(super) fn allocate_blocks_from_items(
        &mut self,
        items: Vec<Item>,
        preserved_block_boundaries: &FxHashSet<Label>,
    ) -> Vec<BasicBlockId> {
        Self::partition_items(items, preserved_block_boundaries)
            .into_iter()
            .map(|items| self.allocate_block(items))
            .collect()
    }

    /// Repartitions existing blocks without changing the identity of each
    /// block's first resulting segment.
    pub(super) fn normalize_blocks(&mut self, preserved_block_boundaries: &FxHashSet<Label>) {
        // Labels may move from an existing block into a newly allocated suffix.
        // Rebuild the index after repartitioning instead of treating that move
        // as a duplicate binding.
        self.label_blocks.clear();
        let original_order = std::mem::take(&mut self.order);
        let mut segment = Vec::new();
        let mut segments = Vec::new();
        for block in original_order.iter().copied() {
            for item in std::mem::take(&mut self[block].items) {
                if let Item::Label(label) = item
                    && (segment
                        .iter()
                        .any(|item| matches!(item, Item::Instruction(_)))
                        || (!segment.is_empty() && preserved_block_boundaries.contains(&label)))
                {
                    segments.push(std::mem::take(&mut segment));
                }
                let ends_block = matches!(item, Item::Instruction(instruction) if
                    !matches!(instruction.operand, Operand::Value(_))
                        || ends_scope(instruction.opcode));
                segment.push(item);
                if ends_block {
                    segments.push(std::mem::take(&mut segment));
                }
            }
        }
        if !segment.is_empty() {
            segments.push(segment);
        }

        let mut reusable = original_order.into_iter();
        let mut order = Vec::with_capacity(segments.len());
        for items in segments {
            if let Some(block) = reusable.next() {
                self[block].items = items;
                order.push(block);
            } else {
                order.push(self.allocate_block(items));
            }
        }
        self.order = order;
        self.rebuild_label_index();
    }

    /// Repartitions the ordered instruction stream for a pass whose logical
    /// block boundaries are narrower than the assembler's structural ones.
    /// Existing block IDs are reused in order; newly split suffixes receive
    /// fresh IDs from the append-only arena.
    pub(super) fn repartition(
        &mut self,
        boundary_labels: &FxHashSet<Label>,
        starts_block: impl Fn(&Instruction) -> bool,
        ends_block: impl Fn(&Instruction) -> bool,
    ) {
        let original_order = std::mem::take(&mut self.order);
        self.label_blocks.clear();
        let mut segments = Vec::<Vec<Item>>::new();
        let mut segment = Vec::new();
        let mut has_instruction = false;
        for block in original_order.iter().copied() {
            for item in std::mem::take(&mut self[block].items) {
                let starts_here = match item {
                    Item::Label(label) => boundary_labels.contains(&label),
                    Item::Instruction(instruction) => starts_block(&instruction),
                };
                if starts_here && has_instruction {
                    segments.push(std::mem::take(&mut segment));
                    has_instruction = false;
                }
                let ends_here = match item {
                    Item::Instruction(instruction) => ends_block(&instruction),
                    Item::Label(_) => false,
                };
                has_instruction |= matches!(item, Item::Instruction(_));
                segment.push(item);
                if ends_here {
                    segments.push(std::mem::take(&mut segment));
                    has_instruction = false;
                }
            }
        }
        if !segment.is_empty() {
            segments.push(segment);
        }

        let mut reusable = original_order.into_iter();
        let mut order = Vec::with_capacity(segments.len());
        for items in segments {
            if let Some(block) = reusable.next() {
                self[block].items = items;
                order.push(block);
            } else {
                order.push(self.allocate_block(items));
            }
        }
        self.order = order;
        self.rebuild_label_index();
    }

    pub(super) fn append_to_block(
        &mut self,
        block: BasicBlockId,
        items: impl IntoIterator<Item = Item>,
    ) {
        let start = self[block].items.len();
        self[block].items.extend(items);
        let labels = self[block].items[start..]
            .iter()
            .filter_map(|item| match item {
                Item::Label(label) => Some(*label),
                Item::Instruction(_) => None,
            })
            .collect::<Vec<_>>();
        for label in labels {
            let previous = self.label_blocks.insert(label, block);
            debug_assert!(previous.is_none(), "label is bound more than once");
        }
    }

    pub(super) fn insert_label(&mut self, block: BasicBlockId, position: usize, label: Label) {
        self[block].items.insert(position, Item::Label(label));
        let previous = self.label_blocks.insert(label, block);
        debug_assert!(previous.is_none(), "label is bound more than once");
    }

    pub(super) fn rebuild_label_index(&mut self) {
        let mut label_blocks = FxHashMap::default();
        for block in self.order.iter().copied() {
            for item in self[block].items() {
                if let Item::Label(label) = item {
                    let previous = label_blocks.insert(*label, block);
                    debug_assert!(previous.is_none(), "label is bound more than once");
                }
            }
        }
        self.label_blocks = label_blocks;
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (BasicBlockId, &BasicBlock)> {
        self.order
            .iter()
            .copied()
            .map(|block| (block, &self[block]))
    }

    pub(super) fn iter_items(&self) -> impl Iterator<Item = &Item> {
        self.iter().flat_map(|(_, block)| block.items())
    }

    pub(super) fn item_positions(&self) -> Vec<ItemPosition> {
        self.iter()
            .flat_map(|(block, contents)| {
                (0..contents.items().len()).map(move |offset| ItemPosition { block, offset })
            })
            .collect()
    }

    pub(super) fn block_item_positions(&self, block: BasicBlockId) -> Vec<ItemPosition> {
        (0..self[block].items().len())
            .map(|offset| ItemPosition { block, offset })
            .collect()
    }

    /// Builds a pass-local ordered view of block contents without moving the
    /// graph's owned items or changing stable block identities.
    pub(super) fn partition_positions(
        &self,
        boundary_labels: &FxHashSet<Label>,
        ends_block: impl Fn(&Instruction) -> bool,
    ) -> Vec<Vec<ItemPosition>> {
        let mut blocks = Vec::new();
        let mut block = Vec::new();
        for position in self.item_positions() {
            let item = self.item(position);
            if matches!(item, Item::Label(label) if boundary_labels.contains(label))
                && !block.is_empty()
            {
                blocks.push(std::mem::take(&mut block));
            }
            let ends_here =
                matches!(item, Item::Instruction(instruction) if ends_block(instruction));
            block.push(position);
            if ends_here {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }
        blocks
    }

    pub(super) fn item(&self, position: ItemPosition) -> &Item {
        &self[position.block].items[position.offset]
    }

    pub(super) fn item_mut(&mut self, position: ItemPosition) -> &mut Item {
        &mut self[position.block].items[position.offset]
    }

    pub(super) fn insert_item(&mut self, position: ItemPosition, item: Item) {
        if let Item::Label(label) = item {
            let previous = self.label_blocks.insert(label, position.block);
            debug_assert!(previous.is_none(), "label is bound more than once");
        }
        self[position.block].items.insert(position.offset, item);
    }

    pub(super) fn remove_item(&mut self, position: ItemPosition) -> Item {
        let item = self[position.block].items.remove(position.offset);
        if let Item::Label(label) = item {
            self.label_blocks.remove(&label);
        }
        item
    }

    pub(super) fn reachable_blocks(
        &self,
        exception_regions: &[ExceptionRegion],
    ) -> FxHashSet<BasicBlockId> {
        let Some(entry) = self.order.first().copied() else {
            return FxHashSet::default();
        };
        let order_positions = self
            .order
            .iter()
            .copied()
            .enumerate()
            .map(|(position, block)| (block, position))
            .collect::<FxHashMap<_, _>>();
        let region_blocks = exception_regions
            .iter()
            .filter_map(|region| {
                Some((
                    self.label_block(region.start)?,
                    self.label_block(region.end)?,
                    self.label_block(region.target)?,
                ))
            })
            .collect::<Vec<_>>();
        let mut reachable = FxHashSet::default();
        let mut pending = vec![entry];
        pending.extend(
            region_blocks
                .iter()
                .filter_map(|(start, end, handler)| (start == end).then_some(*handler)),
        );
        while let Some(block) = pending.pop() {
            if !reachable.insert(block) {
                continue;
            }
            let position = order_positions[&block];
            if block_has_fallthrough(self[block].items())
                && let Some(next) = self.order.get(position + 1)
            {
                pending.push(*next);
            }
            if let Some(target) = block_jump_target(self[block].items())
                && let Some(target) = self.label_block(target)
            {
                pending.push(target);
            }
            for (start, end, handler) in &region_blocks {
                if order_positions[start] <= position && position < order_positions[end] {
                    pending.push(*handler);
                }
            }
        }
        reachable
    }

    pub(super) fn item_reachability(&self, exception_regions: &[ExceptionRegion]) -> Vec<bool> {
        let reachable = self.reachable_blocks(exception_regions);
        self.iter()
            .flat_map(|(block, contents)| {
                let reachable = reachable.contains(&block);
                contents
                    .items()
                    .iter()
                    .map(move |item| matches!(item, Item::Label(_)) || reachable)
            })
            .collect()
    }

    pub(super) fn into_items(self) -> Vec<Item> {
        let mut blocks = self.blocks.into_iter().map(Some).collect::<Vec<_>>();
        self.order
            .into_iter()
            .flat_map(|block| {
                blocks[block.index()]
                    .take()
                    .expect("block appears more than once in emission order")
                    .items
            })
            .collect()
    }

    #[cfg(any(test, debug_assertions))]
    pub(super) fn validate(&self) -> Result<(), String> {
        let mut seen_blocks = FxHashSet::default();
        let mut labels = FxHashMap::default();
        for block in self.order.iter().copied() {
            if !seen_blocks.insert(block) {
                return Err(format!("block {} appears more than once", block.index()));
            }
            let Some(contents) = self.blocks.get(block.index()) else {
                return Err(format!("block {} is outside the arena", block.index()));
            };
            if contents.items.is_empty() {
                return Err(format!("block {} is empty", block.index()));
            }
            for item in contents.items() {
                if let Item::Label(label) = item
                    && labels.insert(*label, block).is_some()
                {
                    return Err(format!("label {} is bound more than once", label.0));
                }
            }
        }
        if labels != self.label_blocks {
            return Err("label-to-block index is stale".to_string());
        }
        Ok(())
    }
}

impl Index<BasicBlockId> for BlockGraph {
    type Output = BasicBlock;

    fn index(&self, index: BasicBlockId) -> &Self::Output {
        &self.blocks[index.index()]
    }
}

impl IndexMut<BasicBlockId> for BlockGraph {
    fn index_mut(&mut self, index: BasicBlockId) -> &mut Self::Output {
        &mut self.blocks[index.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::SourceLocation;
    use crate::target::opcodes::{JUMP_FORWARD, NOP, RETURN_VALUE};

    fn instruction(opcode: crate::target::Opcode, operand: Operand) -> Item {
        Item::Instruction(Instruction::new(
            opcode,
            operand,
            SourceLocation::NONE,
            None,
        ))
    }

    #[test]
    fn block_ids_survive_reordering_and_insertion() {
        let first_label = Label(0);
        let second_label = Label(1);
        let inserted_label = Label(2);
        let mut graph = BlockGraph::from_items(
            vec![
                Item::Label(first_label),
                instruction(JUMP_FORWARD, Operand::Forward(second_label)),
                Item::Label(second_label),
                instruction(RETURN_VALUE, Operand::Value(0)),
            ],
            &FxHashSet::default(),
        );
        let first = graph.block_at(0);
        let second = graph.block_at(1);

        graph.order_mut().swap(0, 1);
        let inserted = graph.insert_block(
            1,
            vec![
                Item::Label(inserted_label),
                instruction(NOP, Operand::Value(0)),
            ],
        );

        assert_eq!(graph.order(), &[second, inserted, first]);
        assert_eq!(graph.label_block(first_label), Some(first));
        assert_eq!(graph.label_block(second_label), Some(second));
        assert_eq!(graph.label_block(inserted_label), Some(inserted));
        graph.validate().unwrap();
    }

    #[test]
    fn preserved_consecutive_label_starts_a_distinct_block() {
        let first = Label(0);
        let boundary = Label(1);
        let graph = BlockGraph::from_items(
            vec![
                Item::Label(first),
                Item::Label(boundary),
                instruction(NOP, Operand::Value(0)),
            ],
            &FxHashSet::from_iter([boundary]),
        );

        assert_eq!(graph.len(), 2);
        assert_ne!(graph.label_block(first), graph.label_block(boundary));
        graph.validate().unwrap();
    }

    #[test]
    fn repartition_reuses_the_leading_block_id_and_moves_labels() {
        let first_label = Label(0);
        let second_label = Label(1);
        let mut graph = BlockGraph::from_items(
            vec![
                Item::Label(first_label),
                instruction(NOP, Operand::Value(0)),
                Item::Label(second_label),
                instruction(RETURN_VALUE, Operand::Value(0)),
            ],
            &FxHashSet::default(),
        );
        let first = graph.block_at(0);
        let second = graph.block_at(1);

        graph.repartition(&FxHashSet::default(), |_| false, |_| false);

        assert_eq!(graph.order(), &[first]);
        assert_ne!(first, second);
        assert_eq!(graph.label_block(first_label), Some(first));
        assert_eq!(graph.label_block(second_label), Some(first));
        graph.validate().unwrap();
    }

    #[test]
    fn normalizing_allocates_stable_ids_for_split_suffixes() {
        let first_label = Label(0);
        let second_label = Label(1);
        let mut graph = BlockGraph {
            blocks: Vec::new(),
            order: Vec::new(),
            label_blocks: FxHashMap::default(),
        };
        let first = graph.push_block(vec![
            Item::Label(first_label),
            instruction(NOP, Operand::Value(0)),
            Item::Label(second_label),
            instruction(RETURN_VALUE, Operand::Value(0)),
        ]);

        graph.normalize_blocks(&FxHashSet::default());

        let suffix = graph.block_at(1);
        assert_eq!(graph.block_at(0), first);
        assert_ne!(suffix, first);
        assert_eq!(graph.label_block(first_label), Some(first));
        assert_eq!(graph.label_block(second_label), Some(suffix));
        graph.validate().unwrap();
    }
}
