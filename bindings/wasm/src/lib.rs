//! WebAssembly bindings for `tiptap-rusty-parser`.
//!
//! Exposes an opaque [`TiptapDoc`] handle to JavaScript: the document tree
//! stays owned by Rust/WASM and is only serialized to JS objects at the
//! boundary. Queries return cloned nodes (read) or index paths (`number[][]`,
//! for mutation targeting); mutation is path-addressed. Values cross the
//! boundary via `serde-wasm-bindgen`, so `path` arguments are plain `number[]`
//! and node/attr/schema arguments are plain JS objects.

use serde::Serialize;
use serde_json::Value;
use serde_wasm_bindgen::{from_value, Serializer};
use tiptap_rusty_parser::{
    Assoc, BlockRange, Change, DiffOptions, Document, HtmlOptions, Mark, Node, NormalizeOptions,
    PosEdit, PosMap, PosRange, Position, Range, ResolvedPos, Schema,
};
use wasm_bindgen::prelude::*;

/// Map a `Display` error into a JS exception.
fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Serialize to a *plain* JS object/array (maps become objects, not `Map`).
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    value.serialize(&Serializer::json_compatible()).map_err(err)
}

/// Parse a JS `number[]` into an index path.
fn parse_path(path: JsValue) -> Result<Vec<usize>, JsError> {
    from_value(path).map_err(err)
}

/// Parse an optional JS array of mark objects into `Vec<Mark>` (`null`/`undefined`
/// or an empty array -> `None`, i.e. unmarked).
fn parse_marks(marks: JsValue) -> Result<Option<Vec<Mark>>, JsError> {
    if marks.is_null() || marks.is_undefined() {
        return Ok(None);
    }
    let v: Vec<Mark> = from_value(marks).map_err(err)?;
    Ok(if v.is_empty() { None } else { Some(v) })
}

/// Parse an optional JS `attrs` object into `Option<Map>`. Only `null`/`undefined`
/// map to `None`; an explicit `{}` is preserved as `Some(empty)` so callers can
/// set present-but-empty attrs (distinct from omitted, and roundtrip-faithful).
fn parse_attrs(attrs: JsValue) -> Result<Option<serde_json::Map<String, Value>>, JsError> {
    if attrs.is_null() || attrs.is_undefined() {
        return Ok(None);
    }
    Ok(Some(from_value(attrs).map_err(err)?))
}

/// Build a `Mark` from a type string and an optional `attrs` object.
fn build_mark(mark_type: String, attrs: JsValue) -> Result<Mark, JsError> {
    let mut mark = Mark::new(mark_type);
    if !attrs.is_null() && !attrs.is_undefined() {
        let m: serde_json::Map<String, Value> = from_value(attrs).map_err(err)?;
        if !m.is_empty() {
            mark.attrs = Some(Box::new(m));
        }
    }
    Ok(mark)
}

/// Parse an optional `Assoc` (`"left"`/`"right"`); `null`/`undefined` -> `Left`.
fn parse_assoc(assoc: JsValue) -> Result<Assoc, JsError> {
    if assoc.is_null() || assoc.is_undefined() {
        return Ok(Assoc::default());
    }
    from_value(assoc).map_err(err)
}

/// camelCase JS view of [`ResolvedPos`] (the core type keeps snake_case fields
/// for serde back-compat; this gives JS the camelCase shape the rest of the
/// binding uses).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsResolvedPos {
    pos: usize,
    depth: usize,
    path: Vec<usize>,
    parent_offset: usize,
    index: usize,
    text_offset: Option<JsTextPoint>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsTextPoint {
    path: Vec<usize>,
    offset: usize,
}

impl From<ResolvedPos> for JsResolvedPos {
    fn from(r: ResolvedPos) -> Self {
        JsResolvedPos {
            pos: r.pos,
            depth: r.depth,
            path: r.path,
            parent_offset: r.parent_offset,
            index: r.index,
            text_offset: r.text_offset.map(|t| JsTextPoint {
                path: t.path,
                offset: t.offset,
            }),
        }
    }
}

/// JS shape returned by `posToInline`: the block path plus the inline position.
#[derive(Serialize)]
struct BlockInline {
    block: Vec<usize>,
    inline: Position,
}

/// A Tiptap/ProseMirror document handle. Construct with [`TiptapDoc::from_json`]
/// or [`TiptapDoc::from_json_string`].
#[wasm_bindgen]
pub struct TiptapDoc {
    inner: Document,
}

#[wasm_bindgen]
impl TiptapDoc {
    // ---- lifecycle / serialize ----

    /// Build from a JS `JSONContent` object.
    #[wasm_bindgen(js_name = fromJSON)]
    pub fn from_json(
        #[wasm_bindgen(unchecked_param_type = "JSONContent")] value: JsValue,
    ) -> Result<TiptapDoc, JsError> {
        let node: Node = from_value(value).map_err(err)?;
        Ok(Self {
            inner: Document::new(node),
        })
    }

    /// Build from a JSON string.
    #[wasm_bindgen(js_name = fromJSONString)]
    pub fn from_json_string(s: &str) -> Result<TiptapDoc, JsError> {
        Ok(Self {
            inner: Document::from_json_str(s).map_err(err)?,
        })
    }

    /// Serialize the document to a JS `JSONContent` object.
    #[wasm_bindgen(js_name = toJSON, unchecked_return_type = "JSONContent")]
    pub fn to_json(&self) -> Result<JsValue, JsError> {
        to_js(self.inner.root())
    }

    /// Serialize the document to a JSON string.
    #[wasm_bindgen(js_name = toJSONString)]
    pub fn to_json_string(&self) -> Result<String, JsError> {
        self.inner.to_json_str().map_err(err)
    }

    // ---- read queries (return cloned node objects) ----

    /// All nodes of `node_type`, as an array of JS objects.
    #[wasm_bindgen(js_name = byType, unchecked_return_type = "JSONContent[]")]
    pub fn by_type(&self, node_type: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.by_type(node_type))
    }

    /// The first node of `node_type`, or `undefined`.
    #[wasm_bindgen(js_name = firstByType, unchecked_return_type = "JSONContent | undefined")]
    pub fn first_by_type(&self, node_type: &str) -> Result<JsValue, JsError> {
        match self.inner.first_by_type(node_type) {
            Some(n) => to_js(n),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// All nodes carrying a mark of `mark_type`.
    #[wasm_bindgen(js_name = byMark, unchecked_return_type = "JSONContent[]")]
    pub fn by_mark(&self, mark_type: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.by_mark(mark_type))
    }

    /// All nodes whose attribute `key` equals `value`.
    #[wasm_bindgen(js_name = byAttr, unchecked_return_type = "JSONContent[]")]
    pub fn by_attr(
        &self,
        key: &str,
        #[wasm_bindgen(unchecked_param_type = "unknown")] value: JsValue,
    ) -> Result<JsValue, JsError> {
        let v: Value = from_value(value).map_err(err)?;
        to_js(&self.inner.by_attr(key, v))
    }

    // ---- path-returning selectors (for mutation targeting) ----

    /// Index paths (`number[][]`) of every node of `node_type`.
    #[wasm_bindgen(js_name = pathsByType, unchecked_return_type = "Path[]")]
    pub fn paths_by_type(&self, node_type: &str) -> Result<JsValue, JsError> {
        to_js(
            &self
                .inner
                .paths_to(|n| n.node_type.as_deref() == Some(node_type)),
        )
    }

    /// Index paths of every node carrying a mark of `mark_type`.
    #[wasm_bindgen(js_name = pathsByMark, unchecked_return_type = "Path[]")]
    pub fn paths_by_mark(&self, mark_type: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.paths_to(|n| n.has_mark(mark_type)))
    }

    /// Index paths of every node whose attribute `key` equals `value`.
    #[wasm_bindgen(js_name = pathsByAttr, unchecked_return_type = "Path[]")]
    pub fn paths_by_attr(
        &self,
        key: &str,
        #[wasm_bindgen(unchecked_param_type = "unknown")] value: JsValue,
    ) -> Result<JsValue, JsError> {
        let v: Value = from_value(value).map_err(err)?;
        to_js(&self.inner.paths_to(|n| n.attr(key) == Some(&v)))
    }

    // ---- read a single node ----

    /// The node at `path`, or `undefined`.
    #[wasm_bindgen(js_name = nodeAt, unchecked_return_type = "JSONContent | undefined")]
    pub fn node_at(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    ) -> Result<JsValue, JsError> {
        let path = parse_path(path)?;
        match self.inner.node_at(&path) {
            Some(n) => to_js(n),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// Child count of the node at `path`, or `undefined` if no such node.
    #[wasm_bindgen(js_name = childCount)]
    pub fn child_count(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    ) -> Result<Option<usize>, JsError> {
        let path = parse_path(path)?;
        Ok(self.inner.node_at(&path).map(Node::child_count))
    }

    // ---- path-addressed mutation ----

    /// Set attribute `key` to `value` on the node at `path`.
    #[wasm_bindgen(js_name = setAttr)]
    pub fn set_attr(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        key: String,
        #[wasm_bindgen(unchecked_param_type = "unknown")] value: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let v: Value = from_value(value).map_err(err)?;
        self.at_mut(&path)?.set_attr(key, v);
        Ok(())
    }

    /// Remove attribute `key` from the node at `path`; returns whether it existed.
    #[wasm_bindgen(js_name = removeAttr)]
    pub fn remove_attr(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        key: &str,
    ) -> Result<bool, JsError> {
        let path = parse_path(path)?;
        Ok(self.at_mut(&path)?.remove_attr(key).is_some())
    }

    /// Set the `text` of the node at `path`.
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        text: String,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        self.at_mut(&path)?.set_text(text);
        Ok(())
    }

    /// Add a mark of `mark_type` (optional `attrs` object) to the node at `path`;
    /// returns whether it was newly added.
    #[wasm_bindgen(js_name = addMark)]
    pub fn add_mark(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        mark_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<bool, JsError> {
        let path = parse_path(path)?;
        let mut mark = Mark::new(mark_type);
        if !attrs.is_null() && !attrs.is_undefined() {
            let m: serde_json::Map<String, Value> = from_value(attrs).map_err(err)?;
            if !m.is_empty() {
                mark.attrs = Some(Box::new(m));
            }
        }
        Ok(self.at_mut(&path)?.add_mark(mark))
    }

    /// Remove all marks of `mark_type` from the node at `path`; returns the count removed.
    #[wasm_bindgen(js_name = removeMark)]
    pub fn remove_mark(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        mark_type: &str,
    ) -> Result<usize, JsError> {
        let path = parse_path(path)?;
        Ok(self.at_mut(&path)?.remove_mark(mark_type))
    }

    /// Append `child` to the node at `path`.
    #[wasm_bindgen(js_name = pushChild)]
    pub fn push_child(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "JSONContent")] child: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let child: Node = from_value(child).map_err(err)?;
        self.at_mut(&path)?.push_child(child);
        Ok(())
    }

    /// Insert `child` at `index` under the node at `path` (index clamped).
    #[wasm_bindgen(js_name = insertChild)]
    pub fn insert_child(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        index: usize,
        #[wasm_bindgen(unchecked_param_type = "JSONContent")] child: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let child: Node = from_value(child).map_err(err)?;
        self.at_mut(&path)?.insert_child(index, child);
        Ok(())
    }

    /// Remove the child at `index` under the node at `path`; returns it, or `undefined`.
    #[wasm_bindgen(js_name = removeChild, unchecked_return_type = "JSONContent | undefined")]
    pub fn remove_child(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        index: usize,
    ) -> Result<JsValue, JsError> {
        let path = parse_path(path)?;
        match self.at_mut(&path)?.remove_child(index) {
            Some(removed) => to_js(&removed),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    // ---- text ----

    /// Concatenated text of all descendant text nodes.
    #[wasm_bindgen(js_name = textContent)]
    pub fn text_content(&self) -> String {
        self.inner.text_content()
    }

    /// Unicode scalar count of the extracted text.
    #[wasm_bindgen(js_name = charCount)]
    pub fn char_count(&self) -> usize {
        self.inner.char_count()
    }

    /// Word count of the extracted text.
    #[wasm_bindgen(js_name = wordCount)]
    pub fn word_count(&self) -> usize {
        self.inner.word_count()
    }

    // ---- validation ----

    /// Validate against a `schema` object; returns an array of violations
    /// (empty = valid).
    #[wasm_bindgen(unchecked_return_type = "Violation[]")]
    pub fn validate(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Schema")] schema: JsValue,
    ) -> Result<JsValue, JsError> {
        let schema: Schema = from_value(schema).map_err(err)?;
        to_js(&self.inner.validate(&schema))
    }

    /// True if the document has no schema violations.
    #[wasm_bindgen(js_name = isValid)]
    pub fn is_valid(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Schema")] schema: JsValue,
    ) -> Result<bool, JsError> {
        let schema: Schema = from_value(schema).map_err(err)?;
        Ok(self.inner.is_valid(&schema))
    }

    // ---- diff / apply ----

    /// Structural diff from this document to `other`; returns an array of
    /// change objects (each tagged with an `op`).
    #[wasm_bindgen(unchecked_return_type = "Change[]")]
    pub fn diff(&self, other: &TiptapDoc) -> Result<JsValue, JsError> {
        to_js(&self.inner.root().diff(other.inner.root()))
    }

    /// Apply a change array (as produced by [`diff`](Self::diff)) in place.
    #[wasm_bindgen(js_name = applyChanges)]
    pub fn apply_changes(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Change[]")] changes: JsValue,
    ) -> Result<(), JsError> {
        let changes: Vec<Change> = from_value(changes).map_err(err)?;
        tiptap_rusty_parser::apply(self.inner.root_mut(), &changes).map_err(err)
    }

    /// Invert a change array relative to this document (the pre-image); returns
    /// the reverse change array for undo. See `applyChanges`.
    #[wasm_bindgen(unchecked_return_type = "Change[]")]
    pub fn invert(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Change[]")] changes: JsValue,
    ) -> Result<JsValue, JsError> {
        let changes: Vec<Change> = from_value(changes).map_err(err)?;
        let inverse = tiptap_rusty_parser::invert(self.inner.root(), &changes).map_err(err)?;
        to_js(&inverse)
    }

    // ---- HTML rendering ----

    /// Render the document to an HTML string (Tiptap-default mapping).
    #[wasm_bindgen(js_name = toHTML)]
    pub fn to_html(&self) -> String {
        self.inner.to_html()
    }

    /// Render to HTML with an options object (see `HtmlOptions`).
    #[wasm_bindgen(js_name = toHTMLWith)]
    pub fn to_html_with(
        &self,
        #[wasm_bindgen(unchecked_param_type = "HtmlOptions")] options: JsValue,
    ) -> Result<String, JsError> {
        let opts: HtmlOptions = from_value(options).map_err(err)?;
        Ok(self.inner.to_html_with(&opts))
    }

    // ---- normalization ----

    /// Normalize the document tree in place (merge adjacent text, drop empties).
    #[wasm_bindgen(js_name = normalize)]
    pub fn normalize(&mut self) {
        self.inner.normalize();
    }

    /// Normalize with an options object (see `NormalizeOptions`).
    #[wasm_bindgen(js_name = normalizeWith)]
    pub fn normalize_with(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "NormalizeOptions")] options: JsValue,
    ) -> Result<(), JsError> {
        let opts: NormalizeOptions = from_value(options).map_err(err)?;
        self.inner.normalize_with(&opts);
        Ok(())
    }

    // ---- range editing (inline content of the block at `path`) ----

    /// Insert `text` (with optional `marks` array) at a `Position` within the
    /// inline content of the block at `path`.
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Position")] position: JsValue,
        text: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Mark[] | null")] marks: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let pos: Position = from_value(position).map_err(err)?;
        let marks = parse_marks(marks)?;
        self.at_mut(&path)?
            .insert_text(pos, &text, marks.as_deref())
            .map_err(err)
    }

    /// Delete a `Range` from the inline content of the block at `path`.
    #[wasm_bindgen(js_name = deleteRange)]
    pub fn delete_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Range")] range: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let range: Range = from_value(range).map_err(err)?;
        self.at_mut(&path)?.delete_range(range).map_err(err)
    }

    /// Replace a `Range` with `text` (and optional `marks` array).
    #[wasm_bindgen(js_name = replaceRange)]
    pub fn replace_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Range")] range: JsValue,
        text: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Mark[] | null")] marks: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let range: Range = from_value(range).map_err(err)?;
        let marks = parse_marks(marks)?;
        self.at_mut(&path)?
            .replace_range(range, &text, marks.as_deref())
            .map_err(err)
    }

    /// Add a mark (`mark_type` + optional `attrs`) over a `Range`.
    #[wasm_bindgen(js_name = addMarkRange)]
    pub fn add_mark_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Range")] range: JsValue,
        mark_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let range: Range = from_value(range).map_err(err)?;
        self.at_mut(&path)?
            .add_mark_range(range, build_mark(mark_type, attrs)?)
            .map_err(err)
    }

    /// Remove all marks of `mark_type` over a `Range`.
    #[wasm_bindgen(js_name = removeMarkRange)]
    pub fn remove_mark_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Range")] range: JsValue,
        mark_type: &str,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let range: Range = from_value(range).map_err(err)?;
        self.at_mut(&path)?
            .remove_mark_range(range, mark_type)
            .map_err(err)
    }

    /// Toggle a mark (`mark_type` + optional `attrs`) over a `Range`.
    #[wasm_bindgen(js_name = toggleMarkRange)]
    pub fn toggle_mark_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Range")] range: JsValue,
        mark_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let range: Range = from_value(range).map_err(err)?;
        self.at_mut(&path)?
            .toggle_mark_range(range, build_mark(mark_type, attrs)?)
            .map_err(err)
    }

    // ---- block-level structural editing (absolute index-path) ----

    /// Change the type (and optional `attrs`) of the block at `path`, keeping
    /// its content.
    #[wasm_bindgen(js_name = setBlockType)]
    pub fn set_block_type(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        node_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        self.inner
            .root_mut()
            .set_block_type(&path, node_type, parse_attrs(attrs)?)
            .map_err(err)
    }

    /// Split the block at `path` at child-boundary `at` (also splitting `depth`
    /// ancestors).
    #[wasm_bindgen(js_name = splitBlock)]
    pub fn split_block(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        at: usize,
        depth: usize,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        self.inner
            .root_mut()
            .split_block(&path, at, depth)
            .map_err(err)
    }

    /// Split the block at `path` at an inline `Position` (mid-text), also
    /// splitting `depth` ancestors.
    #[wasm_bindgen(js_name = splitBlockAt)]
    pub fn split_block_at(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Position")] position: JsValue,
        depth: usize,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        let pos: Position = from_value(position).map_err(err)?;
        self.inner
            .root_mut()
            .split_block_at(&path, pos, depth)
            .map_err(err)
    }

    /// Merge `parent[index]` into its previous sibling `parent[index-1]`.
    #[wasm_bindgen(js_name = joinBlocks)]
    pub fn join_blocks(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] parent: JsValue,
        index: usize,
    ) -> Result<(), JsError> {
        let parent = parse_path(parent)?;
        self.inner
            .root_mut()
            .join_blocks(&parent, index)
            .map_err(err)
    }

    /// Wrap the single block at `path` in a new parent of `wrapperType`.
    #[wasm_bindgen(js_name = wrap)]
    pub fn wrap(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        wrapper_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        self.inner
            .root_mut()
            .wrap(&path, wrapper_type, parse_attrs(attrs)?)
            .map_err(err)
    }

    /// Wrap the run of sibling blocks `[start, end)` under `parentPath` in a new
    /// parent of `wrapperType`.
    #[wasm_bindgen(js_name = wrapRange)]
    pub fn wrap_range(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] parent_path: JsValue,
        start: usize,
        end: usize,
        wrapper_type: String,
        #[wasm_bindgen(unchecked_optional_param_type = "Record<string, unknown> | null")]
        attrs: JsValue,
    ) -> Result<(), JsError> {
        let parent = parse_path(parent_path)?;
        let range = BlockRange::new(parent, start, end);
        self.inner
            .root_mut()
            .wrap_range(&range, wrapper_type, parse_attrs(attrs)?)
            .map_err(err)
    }

    /// Lift the block at `path` out of its parent into its grandparent.
    #[wasm_bindgen(js_name = lift)]
    pub fn lift(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    ) -> Result<(), JsError> {
        let path = parse_path(path)?;
        self.inner.root_mut().lift(&path).map_err(err)
    }

    // ---- flat ProseMirror positions ----

    /// Total flat content size (the maximum valid position).
    #[wasm_bindgen(js_name = contentSize)]
    pub fn content_size(&self) -> usize {
        self.inner.root().content_size()
    }

    /// Resolve a flat position into a `ResolvedPos`.
    #[wasm_bindgen(unchecked_return_type = "ResolvedPos")]
    pub fn resolve(&self, pos: usize) -> Result<JsValue, JsError> {
        let r = self.inner.root().resolve(pos).map_err(err)?;
        to_js(&JsResolvedPos::from(r))
    }

    /// Flat position just before the node at `path`.
    #[wasm_bindgen(js_name = posBefore)]
    pub fn pos_before(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    ) -> Result<usize, JsError> {
        let path = parse_path(path)?;
        self.inner.root().pos_before(&path).map_err(err)
    }

    /// Flat position just after the node at `path`.
    #[wasm_bindgen(js_name = posAfter)]
    pub fn pos_after(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    ) -> Result<usize, JsError> {
        let path = parse_path(path)?;
        self.inner.root().pos_after(&path).map_err(err)
    }

    /// Flat position at scalar `offset` inside the text node at `path`.
    #[wasm_bindgen(js_name = posInText)]
    pub fn pos_in_text(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
        offset: usize,
    ) -> Result<usize, JsError> {
        let path = parse_path(path)?;
        self.inner.root().pos_in_text(&path, offset).map_err(err)
    }

    /// Map a flat position to `{ block: number[], inline: Position }`.
    #[wasm_bindgen(js_name = posToInline, unchecked_return_type = "BlockInline")]
    pub fn pos_to_inline(&self, pos: usize) -> Result<JsValue, JsError> {
        let (block, inline) = self.inner.root().pos_to_inline(pos).map_err(err)?;
        to_js(&BlockInline { block, inline })
    }

    /// Inverse of `posToInline`: flat position for a block path + inline `Position`.
    #[wasm_bindgen(js_name = inlineToPos)]
    pub fn inline_to_pos(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Path")] block_path: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Position")] position: JsValue,
    ) -> Result<usize, JsError> {
        let block = parse_path(block_path)?;
        let pos: Position = from_value(position).map_err(err)?;
        self.inner.root().inline_to_pos(&block, pos).map_err(err)
    }

    // ---- granular diff ----

    /// Structural diff with options (e.g. `{ text: "inline" }` or
    /// `{ text: { smart: { replaceThreshold: 0.5 } } }`); returns `Change[]`.
    #[wasm_bindgen(js_name = diffWith, unchecked_return_type = "Change[]")]
    pub fn diff_with(
        &self,
        other: &TiptapDoc,
        #[wasm_bindgen(unchecked_optional_param_type = "DiffOptions | null")] options: JsValue,
    ) -> Result<JsValue, JsError> {
        let opts: DiffOptions = if options.is_null() || options.is_undefined() {
            DiffOptions::default()
        } else {
            from_value(options).map_err(err)?
        };
        to_js(&self.inner.root().diff_with(other.inner.root(), &opts))
    }

    // ---- position-addressed editing ----

    /// Apply a batch of flat-position `PosEdit`s in place; returns the recovered,
    /// invertible `Change[]`. On error the document is left unchanged.
    #[wasm_bindgen(js_name = applyPosEdits, unchecked_return_type = "Change[]")]
    pub fn apply_pos_edits(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "PosEdit[]")] edits: JsValue,
    ) -> Result<JsValue, JsError> {
        let edits: Vec<PosEdit> = from_value(edits).map_err(err)?;
        let patch = self.inner.root_mut().apply_pos_edits(&edits).map_err(err)?;
        to_js(&patch)
    }
}

impl TiptapDoc {
    /// Resolve a mutable node at `path` or raise a JS error.
    fn at_mut(&mut self, path: &[usize]) -> Result<&mut Node, JsError> {
        self.inner
            .node_at_mut(path)
            .ok_or_else(|| JsError::new("no node at path"))
    }
}

// ---- change-list algebra (free functions over Change[]) ----

/// Compose two change arrays into one apply-equivalent (compacted) array.
#[wasm_bindgen(js_name = compose, unchecked_return_type = "Change[]")]
pub fn compose(
    #[wasm_bindgen(unchecked_param_type = "Change[]")] a: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Change[]")] b: JsValue,
) -> Result<JsValue, JsError> {
    let a: Vec<Change> = from_value(a).map_err(err)?;
    let b: Vec<Change> = from_value(b).map_err(err)?;
    to_js(&tiptap_rusty_parser::compose(&a, &b))
}

/// Coalesce redundant node-local writes in a change array (safely).
#[wasm_bindgen(js_name = compact, unchecked_return_type = "Change[]")]
pub fn compact(
    #[wasm_bindgen(unchecked_param_type = "Change[]")] changes: JsValue,
) -> Result<JsValue, JsError> {
    let changes: Vec<Change> = from_value(changes).map_err(err)?;
    to_js(&tiptap_rusty_parser::compact(&changes))
}

/// Map an index-path through a change array; returns the new `number[]`, or
/// `null` if the node was removed/replaced.
#[wasm_bindgen(js_name = mapPath, unchecked_return_type = "Path | null")]
pub fn map_path(
    #[wasm_bindgen(unchecked_param_type = "Path")] path: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Change[]")] changes: JsValue,
) -> Result<JsValue, JsError> {
    let path = parse_path(path)?;
    let changes: Vec<Change> = from_value(changes).map_err(err)?;
    match tiptap_rusty_parser::map_path(&path, &changes) {
        Some(p) => to_js(&p),
        None => Ok(JsValue::NULL),
    }
}

// ---- flat-position mapping (over a PosEdit[] batch) ----

/// Map a flat position through a disjoint `PosEdit[]` batch (pre-edit -> post-edit
/// coordinates). `assoc` (`"left"`/`"right"`, default `"left"`) picks which edge a
/// position inside a replaced span lands on.
#[wasm_bindgen(js_name = mapPosition)]
pub fn map_position(
    pos: usize,
    #[wasm_bindgen(unchecked_param_type = "PosEdit[]")] edits: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "Assoc | null")] assoc: JsValue,
) -> Result<usize, JsError> {
    let edits: Vec<PosEdit> = from_value(edits).map_err(err)?;
    let assoc = parse_assoc(assoc)?;
    Ok(PosMap::from_pos_edits(&edits).map(pos, assoc))
}

/// Map a `{ from, to }` range through a `PosEdit[]` batch; returns the mapped
/// `PosRange`.
#[wasm_bindgen(js_name = mapPositionRange, unchecked_return_type = "PosRange")]
pub fn map_position_range(
    #[wasm_bindgen(unchecked_param_type = "PosRange")] range: JsValue,
    #[wasm_bindgen(unchecked_param_type = "PosEdit[]")] edits: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "Assoc | null")] assoc: JsValue,
) -> Result<JsValue, JsError> {
    let range: PosRange = from_value(range).map_err(err)?;
    let edits: Vec<PosEdit> = from_value(edits).map_err(err)?;
    let assoc = parse_assoc(assoc)?;
    to_js(&PosMap::from_pos_edits(&edits).map_range(range, assoc))
}

/// Schema-guard: validate a proposed node subtree against `schema` *before*
/// inserting it (e.g. AI-proposed content). Returns the `Violation[]` (empty =
/// valid), so callers can reject invalid content without mutating the document.
#[wasm_bindgen(js_name = validateNode, unchecked_return_type = "Violation[]")]
pub fn validate_node(
    #[wasm_bindgen(unchecked_param_type = "Schema")] schema: JsValue,
    #[wasm_bindgen(unchecked_param_type = "JSONContent")] node: JsValue,
) -> Result<JsValue, JsError> {
    let schema: Schema = from_value(schema).map_err(err)?;
    let node: Node = from_value(node).map_err(err)?;
    to_js(&node.validate(&schema))
}

// ---- TypeScript types ----
// Injected verbatim into the generated `.d.ts`; signatures reference them via `unchecked_*_type`.
#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &'static str = r#"
export type Path = number[];
export interface Mark { type: string; attrs?: Record<string, unknown>; [k: string]: unknown; }
export interface JSONContent {
  type?: string;
  attrs?: Record<string, unknown>;
  content?: JSONContent[];
  marks?: Mark[];
  text?: string;
  [k: string]: unknown;
}
export interface Position { child: number; offset: number; }
export interface Range { start: Position; end: Position; }
export type Change =
  | { op: "setAttr"; path: Path; key: string; value: unknown }
  | { op: "removeAttr"; path: Path; key: string }
  | { op: "setText"; path: Path; text: string | null }
  | { op: "spliceText"; path: Path; from: number; lenDel: number; insert: string }
  | { op: "setMarks"; path: Path; marks: Mark[] | null }
  | { op: "setExtra"; path: Path; key: string; value: unknown }
  | { op: "removeExtra"; path: Path; key: string }
  | { op: "insert"; path: Path; index: number; node: JSONContent }
  | { op: "remove"; path: Path; index: number }
  | { op: "replace"; path: Path; node: JSONContent }
  | { op: "move"; path: Path; from: number; to: number };

// ---- schema validation ----
export interface NodeSpec {
  content?: string[] | string;
  group?: string;
  marks?: string[];
  attrs?: string[];
  required_attrs?: string[];
}
export interface MarkSpec { attrs?: string[]; required_attrs?: string[]; }
export interface Schema {
  nodes?: Record<string, NodeSpec>;
  marks?: Record<string, MarkSpec>;
}
export type ViolationKind =
  | "MissingNodeType"
  | { UnknownNodeType: string }
  | { DisallowedChild: { parent: string; child: string } }
  | { InvalidContent: { parent: string; expr: string } }
  | { UnknownMark: string }
  | { DisallowedMark: { node: string; mark: string } }
  | { MissingAttr: { key: string } }
  | { UnknownAttr: { key: string } };
export interface Violation { path: Path; kind: ViolationKind; child?: number; mark?: number; }

// ---- rendering and normalization ----
export interface HtmlOptions {
  nodeTags?: Record<string, string>;
  markTags?: Record<string, string>;
  unknownNode?: "transparent" | "dataTypeDiv" | "skip";
  unknownMark?: "transparent" | "dataMarkSpan" | "skip";
  selfClosing?: "html5" | "xhtml";
  spreadAttrs?: boolean;
  textAlign?: boolean;
}
export interface NormalizeOptions {
  mergeAdjacentText?: boolean;
  removeEmptyText?: boolean;
  removeEmptyNodes?: boolean;
}

// ---- flat positions ----
export interface TextPoint { path: Path; offset: number; }
export interface ResolvedPos {
  pos: number;
  depth: number;
  path: Path;
  parentOffset: number;
  index: number;
  textOffset: TextPoint | null;
}
export interface PosRange { from: number; to: number; }
export interface BlockInline { block: Path; inline: Position; }
export type Assoc = "left" | "right";

// ---- granular diff ----
export type DiffGranularity =
  | "block"
  | "inline"
  | { smart: { replaceThreshold: number } };
export interface DiffOptions { text?: DiffGranularity; }

// ---- position-addressed editing ----
export type PosContent =
  | { type: "text"; text: string; marks?: Mark[] }
  | { type: "nodes"; nodes: JSONContent[] };
export type PosEdit =
  | { type: "insert"; pos: number; content: PosContent }
  | { type: "delete"; from: number; to: number }
  | { type: "replace"; from: number; to: number; content: PosContent }
  | { type: "addMark"; from: number; to: number; mark: Mark }
  | { type: "removeMark"; from: number; to: number; markType: string }
  | { type: "setBlockAttrs"; pos: number; attrs: Record<string, unknown> };
"#;
