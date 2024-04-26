use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::iter::Peekable;

use pep440_rs::Version;

use crate::version_map::VersionMapDistHandle;

/// An iterator that returns the maximum version from a set of iterators.
pub(crate) struct MaxIterator<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> {
    iterators: Vec<Peekable<T>>,
    heap: BinaryHeap<(&'a Version, Reverse<usize>)>,
}

impl<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> MaxIterator<'a, T> {
    pub(crate) fn new(iterators: Vec<T>) -> Self {
        // Convert each iterator into a peekable.
        let mut iterators = iterators
            .into_iter()
            .map(Iterator::peekable)
            .collect::<Vec<_>>();

        // Create a binary heap to track the maximum version from each iterator, respecting the
        // order of the indexes.
        let heap = iterators
            .iter_mut()
            .enumerate()
            .filter_map(|(index, iter)| iter.peek().map(|(version, _)| (*version, Reverse(index))))
            .collect();

        Self { iterators, heap }
    }
}

impl<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> Iterator
    for MaxIterator<'a, T>
{
    type Item = (&'a Version, VersionMapDistHandle<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        // Take the maximum version from the heap.
        let (_, index) = self.heap.pop()?;

        // Advance the iterator and push the next version onto the heap.
        let next = self.iterators[index.0].next()?;

        // Push the next version onto the heap.
        if let Some((version, _)) = self.iterators[index.0].peek() {
            self.heap.push((version, index));
        }

        Some(next)
    }
}

/// An iterator that returns the minimum version from a set of iterators.
pub(crate) struct MinIterator<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> {
    iterators: Vec<Peekable<T>>,
    heap: BinaryHeap<(Reverse<&'a Version>, Reverse<usize>)>,
}

impl<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> MinIterator<'a, T> {
    pub(crate) fn new(iterators: Vec<T>) -> Self {
        // Convert each iterator into a peekable.
        let mut iterators = iterators
            .into_iter()
            .map(Iterator::peekable)
            .collect::<Vec<_>>();

        // Create a binary heap to track the minimum version from each iterator, respecting the
        // order of the indexes.
        let heap = iterators
            .iter_mut()
            .enumerate()
            .filter_map(|(index, iter)| {
                iter.peek()
                    .map(|(version, _)| (Reverse(*version), Reverse(index)))
            })
            .collect();

        Self { iterators, heap }
    }
}

impl<'a, T: Iterator<Item = (&'a Version, VersionMapDistHandle<'a>)>> Iterator
    for MinIterator<'a, T>
{
    type Item = (&'a Version, VersionMapDistHandle<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        // Take the minimum version from the heap, skipping duplicates.
        let (_, index) = self.heap.pop()?;

        // Advance the iterator and push the next version onto the heap.
        let next = self.iterators[index.0].next()?;

        // Push the next version onto the heap.
        if let Some((version, _)) = self.iterators[index.0].peek() {
            self.heap.push((Reverse(version), index));
        }

        Some(next)
    }
}
