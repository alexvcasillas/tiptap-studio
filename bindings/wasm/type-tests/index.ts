import {
  type Assoc,
  type BlockInline,
  type Change,
  type JSONContent,
  type Path,
  type PosEdit,
  type PosRange,
  type ResolvedPos,
  type Schema,
  type Violation,
  type ViolationKind,
  compact,
  compose,
  mapPath,
  mapPosition,
  mapPositionRange,
  TiptapDoc,
  validateNode,
} from "../pkg";

const json: JSONContent = {
  type: "doc",
  content: [{ type: "paragraph", content: [{ type: "text", text: "hi" }] }],
};
const schema: Schema = {
  nodes: { doc: { content: "paragraph+" }, paragraph: { content: ["text"], required_attrs: ["id"] }, text: {} },
  marks: { bold: {} },
};
const path: Path = [0];
const range = { start: { child: 0, offset: 0 }, end: { child: 0, offset: 2 } };
const edits: PosEdit[] = [{ type: "insert", pos: 1, content: { type: "text", text: "x" } }];

const doc: TiptapDoc = TiptapDoc.fromJSON(json);
const other = TiptapDoc.fromJSONString(JSON.stringify(json));

const root: JSONContent = doc.toJSON();
const str: string = doc.toJSONString();
const nodes: JSONContent[] = doc.byType("paragraph").concat(doc.byMark("bold"), doc.byAttr("level", 1));
const first: JSONContent | undefined = doc.firstByType("heading");
const paths: Path[] = doc.pathsByType("text").concat(doc.pathsByMark("bold"), doc.pathsByAttr("id", "a"));
const at: JSONContent | undefined = doc.nodeAt(path);
const count: number | undefined = doc.childCount(path);

doc.setAttr(path, "id", { nested: true });
const removedAttr: boolean = doc.removeAttr(path, "id");
doc.setText([0, 0], "hello");
const added: boolean = doc.addMark([0, 0], "link", { href: "https://example.com" });
doc.addMark([0, 0], "bold");
const removedMarks: number = doc.removeMark([0, 0], "bold");
doc.pushChild(path, { type: "text", text: "!" });
doc.insertChild(path, 0, { type: "text", text: ">" });
const removedChild: JSONContent | undefined = doc.removeChild(path, 0);

const text: string = doc.textContent();
const chars: number = doc.charCount() + doc.wordCount() + doc.contentSize();

const violations: Violation[] = doc.validate(schema);
const valid: boolean = doc.isValid(schema);
const kind: ViolationKind | undefined = violations[0]?.kind;
if (kind !== undefined && typeof kind !== "string" && "DisallowedChild" in kind) {
  const parent: string = kind.DisallowedChild.parent;
}

const changes: Change[] = doc.diff(other);
doc.applyChanges(changes);
const inverse: Change[] = doc.invert(changes);
const granular: Change[] = doc.diffWith(other, { text: { smart: { replaceThreshold: 0.5 } } });
doc.diffWith(other);
doc.diffWith(other, null);

const html: string = doc.toHTML() + doc.toHTMLWith({ nodeTags: { callout: "aside" }, unknownNode: "skip", selfClosing: "xhtml" });
doc.normalize();
doc.normalizeWith({ removeEmptyNodes: true });

doc.insertText(path, { child: 0, offset: 1 }, "x");
doc.insertText(path, { child: 0, offset: 1 }, "x", [{ type: "bold" }]);
doc.deleteRange(path, range);
doc.replaceRange(path, range, "y", null);
doc.addMarkRange(path, range, "italic");
doc.removeMarkRange(path, range, "italic");
doc.toggleMarkRange(path, range, "link", { href: "#" });

doc.setBlockType(path, "heading", { level: 2 });
doc.splitBlock(path, 1, 0);
doc.splitBlockAt(path, { child: 0, offset: 1 }, 0);
doc.joinBlocks([], 1);
doc.wrap(path, "blockquote");
doc.wrapRange([], 0, 1, "bulletList", null);
doc.lift([0, 0]);

const resolved: ResolvedPos = doc.resolve(1);
const flat: number = doc.posBefore(path) + doc.posAfter(path) + doc.posInText([0, 0], 1);
const inline: BlockInline = doc.posToInline(1);
const back: number = doc.inlineToPos(inline.block, inline.inline);
const patch: Change[] = doc.applyPosEdits(edits);

const composed: Change[] = compose(changes, inverse);
const compacted: Change[] = compact(composed);
const mapped: Path | null = mapPath(path, changes);
const assoc: Assoc = "right";
const pos: number = mapPosition(1, edits) + mapPosition(1, edits, assoc);
const mappedRange: PosRange = mapPositionRange({ from: 1, to: 2 }, edits, "left");
const nodeViolations: Violation[] = validateNode(schema, json);

// Wrong types must fail; with `any` none of these would.
// @ts-expect-error the root is JSONContent, not a string
const notString: string = doc.toJSON();
// @ts-expect-error paths are number arrays
doc.nodeAt("0");
// @ts-expect-error marks are objects with a type
doc.insertText(path, { child: 0, offset: 0 }, "x", ["bold"]);
// @ts-expect-error unknown diff granularity
doc.diffWith(other, { text: "word" });
// @ts-expect-error assoc is "left" or "right"
mapPosition(1, edits, "middle");
// @ts-expect-error mapPath may return null
const alwaysPath: Path = mapPath(path, changes);
// @ts-expect-error unknown change op
doc.applyChanges([{ op: "delete", path: [0] }]);
// @ts-expect-error nodeAt may return undefined
const alwaysNode: JSONContent = doc.nodeAt(path);
