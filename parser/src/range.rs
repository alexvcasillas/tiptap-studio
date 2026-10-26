//! Editor-style range editing over a block's inline content.
//!
//! These methods treat `self` as a **single block** (e.g. a `paragraph`) and
//! edit its inline children — text nodes and inline leaves — addressed by a
//! [`Position`] (`child` index + Unicode-scalar `offset` into that child's
//! text). A [`Range`] spans two positions in the *same* block. To edit a nested
//! block, resolve it first: `doc.node_at_mut(&path)?.delete_range(range)`.
//!
//! Text nodes are split at range boundaries as needed, and adjacent text nodes
//! with equal marks are merged again afterwards (sharing the [`normalize`]
//! primitive), so edits leave the inline content canonical.
//!
//! Offsets count **Unicode scalar values** (`char`s), consistent with
//! [`Node::char_count`]. Splits always land on a scalar boundary (never mid-code
//! point); they may split a grapheme cluster. Out-of-range positions return a
//! [`RangeError`] rather than clamping.
//!
//! ```
//! use tiptap_rusty_parser::{Mark, Node, Position, Range};
//!
//! // "Hello world" as one text node inside a paragraph.
//! let mut p = Node::element("paragraph").with_child(Node::text("Hello world"));
//! // Bold "world".
//! p.add_mark_range(Range::new(Position::new(0, 6), Position::new(0, 11)), Mark::new("bold")).unwrap();
//! assert_eq!(p.child_count(), 2); // "Hello " | "world"(bold)
//! assert!(p.child(1).unwrap().has_mark("bold"));
//! ```
//!
//! [`normalize`]: Node::normalize

use crate::node::{Mark, Node};
use crate::normalize::{normalize_children, NormalizeOptions};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A position in a block's inline content: a `child` index plus a Unicode-scalar
/// `offset` into that child's text. For a non-text child, `offset` must be `0`
/// (the boundary before it); the boundary after it is the next child's offset 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Index of the child within the block's content.
    pub child: usize,
    /// Unicode-scalar offset into that child's text.
    pub offset: usize,
}

impl Position {
    /// Construct a position.
    #[inline]
    pub fn new(child: usize, offset: usize) -> Self {
        Self { child, offset }
    }
}

/// A range between two [`Position`]s in the same block (`start <= end` in
/// document order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    /// Start position (inclusive).
    pub start: Position,
    /// End position (exclusive).
    pub end: Position,
}

impl Range {
    /// Construct a range.
    #[inline]
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// A collapsed (empty) range at `pos`.
    #[inline]
    pub fn collapsed(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }
}

/// Why a range operation could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// `child` index is past the end of the content.
    ChildOutOfRange {
        /// The offending child index.
        child: usize,
    },
    /// `offset` is past the end of that child's text.
    OffsetOutOfRange {
        /// The child index.
        child: usize,
        /// The offending offset.
        offset: usize,
    },
    /// A non-zero `offset` was given for a non-text child.
    NotTextNode {
        /// The child index.
        child: usize,
    },
    /// `end` precedes `start` in document order.
    InvertedRange,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeError::ChildOutOfRange { child } => write!(f, "range: child {child} out of range"),
            RangeError::OffsetOutOfRange { child, offset } => {
                write!(f, "range: offset {offset} out of range in child {child}")
            }
            RangeError::NotTextNode { child } => {
                write!(f, "range: child {child} is not a text node")
            }
            RangeError::InvertedRange => write!(f, "range: end precedes start"),
        }
    }
}

impl std::error::Error for RangeError {}

impl Node {
    /// Insert `text` (with optional `marks`) at `pos` in this block's inline
    /// content, splitting the target text node if needed and merging with
    /// adjacent equal-mark text afterwards.
    pub fn insert_text(
        &mut self,
        pos: Position,
        text: &str,
        marks: Option<&[Mark]>,
    ) -> Result<(), RangeError> {
        check_boundary(self.children(), pos)?;
        let children = self.children_mut();
        let i = split_at_boundary(children, pos);
        children.insert(i, make_text(text, marks));
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }

    /// Delete everything in `range`, splitting text nodes at the boundaries and
    /// removing fully-covered inline nodes. A collapsed range is a no-op.
    pub fn delete_range(&mut self, range: Range) -> Result<(), RangeError> {
        check_range(self.children(), range)?;
        if range.start == range.end {
            return Ok(());
        }
        let children = self.children_mut();
        let (s, e) = split_range(children, range);
        children.drain(s..e);
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }

    /// Replace `range` with `text` (carrying optional `marks`).
    pub fn replace_range(
        &mut self,
        range: Range,
        text: &str,
        marks: Option<&[Mark]>,
    ) -> Result<(), RangeError> {
        check_range(self.children(), range)?;
        if range.start == range.end && text.is_empty() {
            return Ok(());
        }
        let children = self.children_mut();
        let (s, e) = split_range(children, range);
        children.drain(s..e);
        if !text.is_empty() {
            children.insert(s, make_text(text, marks));
        }
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }

    /// Add `mark` to every text node covered by `range` (splitting at the
    /// boundaries first), then re-merge equal-mark neighbours.
    pub fn add_mark_range(&mut self, range: Range, mark: Mark) -> Result<(), RangeError> {
        check_range(self.children(), range)?;
        if range.start == range.end {
            return Ok(());
        }
        let children = self.children_mut();
        let (s, e) = split_range(children, range);
        for node in &mut children[s..e] {
            if is_text(node) {
                node.add_mark(mark.clone());
            }
        }
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }

    /// Remove every mark of `mark_type` from text nodes covered by `range`.
    pub fn remove_mark_range(&mut self, range: Range, mark_type: &str) -> Result<(), RangeError> {
        check_range(self.children(), range)?;
        if range.start == range.end {
            return Ok(());
        }
        let children = self.children_mut();
        let (s, e) = split_range(children, range);
        for node in &mut children[s..e] {
            if is_text(node) {
                node.remove_mark(mark_type);
            }
        }
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }

    /// Toggle `mark` over `range`: if **every** covered text node already has a
    /// mark of that type, remove it from all; otherwise add it to all. Mirrors
    /// the ProseMirror toggle semantics, range-scoped.
    pub fn toggle_mark_range(&mut self, range: Range, mark: Mark) -> Result<(), RangeError> {
        check_range(self.children(), range)?;
        if range.start == range.end {
            return Ok(());
        }
        let children = self.children_mut();
        let (s, e) = split_range(children, range);
        let all_have = children[s..e]
            .iter()
            .filter(|n| is_text(n))
            .all(|n| n.has_mark(&mark.mark_type));
        for node in &mut children[s..e] {
            if is_text(node) {
                if all_have {
                    node.remove_mark(&mark.mark_type);
                } else {
                    node.add_mark(mark.clone());
                }
            }
        }
        normalize_children(children, &NormalizeOptions::default());
        Ok(())
    }
}

#[inline]
fn is_text(n: &Node) -> bool {
    n.node_type.as_deref() == Some("text")
}

#[inline]
fn char_len(n: &Node) -> usize {
    n.text.as_deref().unwrap_or("").chars().count()
}

/// Build a text node with optional marks (empty/absent marks -> a plain text node).
fn make_text(text: &str, marks: Option<&[Mark]>) -> Node {
    match marks {
        Some(m) if !m.is_empty() => Node::text_with_marks(text, m.iter().cloned()),
        _ => Node::text(text),
    }
}

/// Split a text node at scalar offset `k`, preserving marks/attrs/extra on both
/// halves. `k` is assumed `<= char_len(node)`.
pub(crate) fn split_text_at(node: &Node, k: usize) -> (Node, Node) {
    let s = node.text.as_deref().unwrap_or("");
    let byte = s.char_indices().nth(k).map_or(s.len(), |(b, _)| b);
    let (l, r) = s.split_at(byte);
    let mut left = node.clone();
    left.text = Some(l.to_owned());
    let mut right = node.clone();
    right.text = Some(r.to_owned());
    (left, right)
}

/// Check that `pos` is a valid boundary in `children` without changing them.
pub(crate) fn check_boundary(children: &[Node], pos: Position) -> Result<(), RangeError> {
    let Some(child) = children.get(pos.child) else {
        // End-of-content position; only offset 0 is meaningful.
        return if pos.child > children.len() {
            Err(RangeError::ChildOutOfRange { child: pos.child })
        } else if pos.offset == 0 {
            Ok(())
        } else {
            Err(RangeError::OffsetOutOfRange {
                child: pos.child,
                offset: pos.offset,
            })
        };
    };
    if is_text(child) {
        if pos.offset > char_len(child) {
            return Err(RangeError::OffsetOutOfRange {
                child: pos.child,
                offset: pos.offset,
            });
        }
        Ok(())
    } else if pos.offset == 0 {
        Ok(())
    } else {
        Err(RangeError::NotTextNode { child: pos.child })
    }
}

/// Check both ends of `range` and their order without changing `children`.
pub(crate) fn check_range(children: &[Node], range: Range) -> Result<(), RangeError> {
    let (sp, ep) = (range.start, range.end);
    if (ep.child, ep.offset) < (sp.child, sp.offset) {
        return Err(RangeError::InvertedRange);
    }
    check_boundary(children, ep)?;
    check_boundary(children, sp)
}

/// Ensure a child boundary exists exactly at `pos`, splitting a text node if it
/// falls mid-text. Returns the index of the first child at or after `pos`.
/// Validates first, so an error leaves `children` unchanged.
pub(crate) fn ensure_boundary(
    children: &mut Vec<Node>,
    pos: Position,
) -> Result<usize, RangeError> {
    check_boundary(children, pos)?;
    Ok(split_at_boundary(children, pos))
}

/// [`ensure_boundary`] for a `pos` already accepted by [`check_boundary`].
fn split_at_boundary(children: &mut Vec<Node>, pos: Position) -> usize {
    let Some(child) = children.get(pos.child) else {
        return children.len();
    };
    if !is_text(child) || pos.offset == 0 {
        return pos.child;
    }
    if pos.offset == char_len(child) {
        return pos.child + 1;
    }
    let (l, r) = split_text_at(child, pos.offset);
    children[pos.child] = l;
    children.insert(pos.child + 1, r);
    pos.child + 1
}

/// Resolve a range to a `[s, e)` child-index span, splitting text nodes at both
/// boundaries. Validates both ends first, so an error leaves `children` unchanged.
pub(crate) fn resolve_range(
    children: &mut Vec<Node>,
    range: Range,
) -> Result<(usize, usize), RangeError> {
    check_range(children, range)?;
    Ok(split_range(children, range))
}

/// [`resolve_range`] for a range already accepted by [`check_range`]. Splits the
/// end first, then the start, shifting the end if the start split inserted a node.
fn split_range(children: &mut Vec<Node>, range: Range) -> (usize, usize) {
    let e = split_at_boundary(children, range.end);
    let len_after_end = children.len();
    let s = split_at_boundary(children, range.start);
    let start_split_inserted = children.len() > len_after_end;
    let e = if start_split_inserted && s <= e {
        e + 1
    } else {
        e
    };
    (s, e)
}
