/// Convert Markdown to ProseMirror JSON matching Productive's Docs schema.
///
/// Uses comrak to parse markdown into an AST, then walks the tree to produce
/// ProseMirror JSON nodes. Custom node names match Productive's schema:
/// - `codeblock` (not `code_block`)
/// - `divider` (not `horizontal_rule`)
/// - `ul`/`ol`/`li` (not `bullet_list`/`ordered_list`/`list_item`)
use comrak::arena_tree::Node;
use comrak::nodes::{Ast, ListType, NodeValue};
use comrak::{parse_document, Arena, Options};
use serde_json::{json, Value};

/// Convert markdown to a ProseMirror JSON string.
pub fn markdown_to_prosemirror_json(markdown: &str) -> String {
    if markdown.is_empty() {
        return json!({"type": "doc", "content": [{"type": "paragraph"}]}).to_string();
    }

    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;

    let root = parse_document(&arena, markdown, &options);
    let content = convert_children(root);

    if content.is_empty() {
        return json!({"type": "doc", "content": [{"type": "paragraph"}]}).to_string();
    }

    json!({"type": "doc", "content": content}).to_string()
}

fn convert_children<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> Vec<Value> {
    node.children()
        .filter_map(|child| convert_node(child))
        .collect()
}

fn convert_node<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> Option<Value> {
    let ast = node.data.borrow();
    match &ast.value {
        NodeValue::Document => None,

        NodeValue::Paragraph => {
            let content = convert_inline_children(node);
            if content.is_empty() {
                Some(json!({"type": "paragraph"}))
            } else {
                Some(json!({"type": "paragraph", "content": content}))
            }
        }

        NodeValue::Heading(h) => {
            let content = convert_inline_children(node);
            let mut n = json!({"type": "heading", "attrs": {"level": h.level}});
            if !content.is_empty() {
                n["content"] = json!(content);
            }
            Some(n)
        }

        NodeValue::BlockQuote => {
            let content = convert_children(node);
            if content.is_empty() {
                None
            } else {
                Some(json!({"type": "blockquote", "content": content}))
            }
        }

        NodeValue::CodeBlock(cb) => {
            let lang = if cb.info.is_empty() {
                Value::Null
            } else {
                json!(cb.info.split_whitespace().next().unwrap_or(""))
            };
            let text = cb.literal.clone();
            // Strip trailing newline (comrak adds one)
            let text = text.strip_suffix('\n').unwrap_or(&text);
            if text.is_empty() {
                Some(json!({"type": "codeblock", "attrs": {"language": lang}}))
            } else {
                Some(json!({
                    "type": "codeblock",
                    "attrs": {"language": lang},
                    "content": [{"type": "text", "text": text}]
                }))
            }
        }

        NodeValue::List(list) => {
            let list_type = match list.list_type {
                ListType::Ordered => "ol",
                ListType::Bullet => "ul",
            };
            let items: Vec<Value> = node
                .children()
                .filter_map(|child| convert_list_item(child))
                .collect();

            let mut n = json!({"type": list_type, "content": items});
            if list.list_type == ListType::Ordered && list.start != 1 {
                n["attrs"] = json!({"order": list.start});
            }
            Some(n)
        }

        NodeValue::Item(_) => {
            // Handled by convert_list_item
            None
        }

        NodeValue::ThematicBreak => Some(json!({"type": "divider"})),

        NodeValue::Table(_) => {
            let rows: Vec<Value> = node.children().filter_map(|row| convert_table_row(row)).collect();
            if rows.is_empty() {
                None
            } else {
                Some(json!({"type": "table", "content": rows}))
            }
        }

        NodeValue::TableRow(is_header) => {
            let cell_type = if *is_header { "table_header" } else { "table_cell" };
            let cells: Vec<Value> = node
                .children()
                .map(|cell| {
                    let content = convert_inline_children(cell);
                    let para = if content.is_empty() {
                        json!({"type": "paragraph"})
                    } else {
                        json!({"type": "paragraph", "content": content})
                    };
                    json!({"type": cell_type, "content": [para]})
                })
                .collect();
            Some(json!({"type": "table_row", "content": cells}))
        }

        NodeValue::TableCell => None, // Handled inline by TableRow

        // Block-level text (tight list items) — wrap in paragraph
        NodeValue::Text(text) => {
            let t = text.clone();
            if t.is_empty() {
                None
            } else {
                Some(json!({"type": "paragraph", "content": [{"type": "text", "text": t}]}))
            }
        }

        NodeValue::SoftBreak | NodeValue::LineBreak => None,

        // Inline nodes shouldn't appear at block level, but handle gracefully
        _ => None,
    }
}

fn convert_table_row<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> Option<Value> {
    let ast = node.data.borrow();
    if let NodeValue::TableRow(is_header) = &ast.value {
        let cell_type = if *is_header { "table_header" } else { "table_cell" };
        let cells: Vec<Value> = node
            .children()
            .map(|cell| {
                let content = convert_inline_children(cell);
                let para = if content.is_empty() {
                    json!({"type": "paragraph"})
                } else {
                    json!({"type": "paragraph", "content": content})
                };
                json!({"type": cell_type, "content": [para]})
            })
            .collect();
        Some(json!({"type": "table_row", "content": cells}))
    } else {
        None
    }
}

fn convert_list_item<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> Option<Value> {
    let content = convert_children(node);
    if content.is_empty() {
        Some(json!({"type": "li", "content": [{"type": "paragraph"}]}))
    } else {
        Some(json!({"type": "li", "content": content}))
    }
}

/// Convert inline children of a node to ProseMirror text nodes with marks.
fn convert_inline_children<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> Vec<Value> {
    let mut result = Vec::new();
    collect_inline(node, &[], &mut result);
    result
}

fn collect_inline<'a>(
    node: &'a Node<'a, std::cell::RefCell<Ast>>,
    marks: &[Value],
    result: &mut Vec<Value>,
) {
    for child in node.children() {
        let ast = child.data.borrow();
        match &ast.value {
            NodeValue::Text(text) => {
                let t = text.clone();
                if !t.is_empty() {
                    let mut n = json!({"type": "text", "text": t});
                    if !marks.is_empty() {
                        n["marks"] = json!(marks);
                    }
                    result.push(n);
                }
            }

            NodeValue::Code(code) => {
                let t = code.literal.clone();
                let mut all_marks: Vec<Value> = marks.to_vec();
                all_marks.push(json!({"type": "code"}));
                result.push(json!({"type": "text", "text": t, "marks": all_marks}));
            }

            NodeValue::Strong => {
                let mut new_marks: Vec<Value> = vec![json!({"type": "strong"})];
                new_marks.extend(marks.iter().cloned());
                collect_inline(child, &new_marks, result);
            }

            NodeValue::Emph => {
                let mut new_marks: Vec<Value> = vec![json!({"type": "em"})];
                new_marks.extend(marks.iter().cloned());
                collect_inline(child, &new_marks, result);
            }

            NodeValue::Strikethrough => {
                let mut new_marks: Vec<Value> = vec![json!({"type": "strike"})];
                new_marks.extend(marks.iter().cloned());
                collect_inline(child, &new_marks, result);
            }

            NodeValue::Link(link) => {
                let href = link.url.clone();
                let title = link.title.clone();
                let link_mark = json!({
                    "type": "link",
                    "attrs": {
                        "href": href,
                        "title": if title.is_empty() { Value::Null } else { json!(title) }
                    }
                });
                let mut new_marks: Vec<Value> = vec![link_mark];
                new_marks.extend(marks.iter().cloned());
                collect_inline(child, &new_marks, result);
            }

            NodeValue::Image(img) => {
                let src = img.url.clone();
                let alt = img.title.clone();
                // For images inside inline context, get alt from child text nodes
                let alt_text = collect_text(child);
                result.push(json!({
                    "type": "image",
                    "attrs": {
                        "src": src,
                        "alt": if alt_text.is_empty() { Value::Null } else { json!(alt_text) },
                        "title": if alt.is_empty() { Value::Null } else { json!(alt) }
                    }
                }));
            }

            NodeValue::SoftBreak => {
                // Treat as space in inline context
                result.push(json!({"type": "text", "text": " "}));
            }

            NodeValue::LineBreak => {
                result.push(json!({"type": "br"}));
            }

            // Recurse into other inline containers
            _ => {
                collect_inline(child, marks, result);
            }
        }
    }
}

/// Collect all text content from a node tree (for alt text extraction).
fn collect_text<'a>(node: &'a Node<'a, std::cell::RefCell<Ast>>) -> String {
    let mut text = String::new();
    for child in node.children() {
        let ast = child.data.borrow();
        if let NodeValue::Text(t) = &ast.value {
            text.push_str(t);
        } else {
            text.push_str(&collect_text(child));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(md: &str) -> Value {
        let json_str = markdown_to_prosemirror_json(md);
        serde_json::from_str(&json_str).unwrap()
    }

    #[test]
    fn empty_input() {
        let doc = parse("");
        assert_eq!(doc["type"], "doc");
        assert_eq!(doc["content"][0]["type"], "paragraph");
    }

    #[test]
    fn simple_paragraph() {
        let doc = parse("Hello world");
        assert_eq!(doc["content"][0]["type"], "paragraph");
        assert_eq!(doc["content"][0]["content"][0]["text"], "Hello world");
    }

    #[test]
    fn multiple_paragraphs() {
        let doc = parse("First\n\nSecond");
        assert_eq!(doc["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn heading_h1() {
        let doc = parse("# Title");
        assert_eq!(doc["content"][0]["type"], "heading");
        assert_eq!(doc["content"][0]["attrs"]["level"], 1);
    }

    #[test]
    fn heading_h2() {
        let doc = parse("## Subtitle");
        assert_eq!(doc["content"][0]["attrs"]["level"], 2);
    }

    #[test]
    fn bold_mark() {
        let doc = parse("**bold**");
        assert_eq!(doc["content"][0]["content"][0]["marks"][0]["type"], "strong");
    }

    #[test]
    fn italic_mark() {
        let doc = parse("*italic*");
        assert_eq!(doc["content"][0]["content"][0]["marks"][0]["type"], "em");
    }

    #[test]
    fn code_mark() {
        let doc = parse("`code`");
        assert_eq!(doc["content"][0]["content"][0]["marks"][0]["type"], "code");
    }

    #[test]
    fn strikethrough_mark() {
        let doc = parse("~~deleted~~");
        assert_eq!(doc["content"][0]["content"][0]["marks"][0]["type"], "strike");
    }

    #[test]
    fn link_mark() {
        let doc = parse("[click](https://example.com)");
        let text = &doc["content"][0]["content"][0];
        assert_eq!(text["marks"][0]["type"], "link");
        assert_eq!(text["marks"][0]["attrs"]["href"], "https://example.com");
        assert_eq!(text["marks"][0]["attrs"]["title"], Value::Null);
    }

    #[test]
    fn blockquote() {
        let doc = parse("> quoted");
        assert_eq!(doc["content"][0]["type"], "blockquote");
    }

    #[test]
    fn code_block_with_language() {
        let doc = parse("```js\nconst x = 1;\n```");
        assert_eq!(doc["content"][0]["type"], "codeblock");
        assert_eq!(doc["content"][0]["attrs"]["language"], "js");
    }

    #[test]
    fn code_block_without_language() {
        let doc = parse("```\ncode\n```");
        assert_eq!(doc["content"][0]["type"], "codeblock");
        assert_eq!(doc["content"][0]["attrs"]["language"], Value::Null);
    }

    #[test]
    fn unordered_list() {
        let doc = parse("- item 1\n- item 2");
        assert_eq!(doc["content"][0]["type"], "ul");
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn ordered_list() {
        let doc = parse("1. first\n2. second");
        assert_eq!(doc["content"][0]["type"], "ol");
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn ordered_list_custom_start() {
        let doc = parse("3. third\n4. fourth");
        assert_eq!(doc["content"][0]["type"], "ol");
        assert_eq!(doc["content"][0]["attrs"]["order"], 3);
    }

    #[test]
    fn horizontal_rule() {
        let doc = parse("---");
        assert_eq!(doc["content"][0]["type"], "divider");
    }

    #[test]
    fn image() {
        let doc = parse("![alt text](https://example.com/img.png)");
        let img = &doc["content"][0]["content"][0];
        assert_eq!(img["type"], "image");
        assert_eq!(img["attrs"]["src"], "https://example.com/img.png");
        assert_eq!(img["attrs"]["alt"], "alt text");
    }

    #[test]
    fn table() {
        let doc = parse("| A | B |\n|---|---|\n| 1 | 2 |");
        let table = &doc["content"][0];
        assert_eq!(table["type"], "table");
        assert_eq!(table["content"].as_array().unwrap().len(), 2);
        assert_eq!(table["content"][0]["type"], "table_row");
        assert_eq!(table["content"][0]["content"][0]["type"], "table_header");
        assert_eq!(table["content"][1]["content"][0]["type"], "table_cell");
    }

    #[test]
    fn doc_type_is_always_doc() {
        assert_eq!(parse("anything")["type"], "doc");
    }

    #[test]
    fn list_with_inline_formatting() {
        let doc = parse("- **bold** text\n- *italic* text");
        let list = &doc["content"][0];
        assert_eq!(list["type"], "ul");
        let li = &list["content"][0];
        assert_eq!(li["type"], "li");
        let para = &li["content"][0];
        assert_eq!(para["type"], "paragraph");
        assert_eq!(para["content"][0]["marks"][0]["type"], "strong");
    }
}
