// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use crate::agent::anchor_state::generate_anchor;

/// Generates a collapsed symbol skeleton for code compression.
/// Supports Rust, JS/TS, Swift, and Python.
pub fn generate_skeleton(path: &Path, content: &str, anchors: &[String]) -> String {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let is_python = extension == "py";

    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut result = Vec::with_capacity(lines.len());

    if is_python {
        let mut in_body = false;
        let mut body_indent = 0;
        let mut anchor_opt: Option<String> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let indent = line.len() - line.trim_start().len();

            if in_body {
                if trimmed.is_empty() {
                    continue;
                }
                if indent > body_indent {
                    continue;
                } else {
                    if let Some(anchor) = anchor_opt.take() {
                        let indent_str = " ".repeat(body_indent);
                        result.push(format!("{}# ... §{} ...", indent_str, anchor));
                    }
                    in_body = false;
                }
            }

            if trimmed.starts_with("def ") {
                result.push(line.clone());
                in_body = true;
                body_indent = indent + 4;
                let anchor = anchors.get(i).cloned().unwrap_or_else(|| generate_anchor(line));
                anchor_opt = Some(anchor);
            } else {
                result.push(line.clone());
            }
        }
        if in_body {
            if let Some(anchor) = anchor_opt.take() {
                let indent_str = " ".repeat(body_indent);
                result.push(format!("{}# ... §{} ...", indent_str, anchor));
            }
        }
    } else {
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            let trimmed = line.trim();

            if is_function_decl(trimmed, extension) {
                result.push(line.clone());
                let anchor = anchors.get(i).cloned().unwrap_or_else(|| generate_anchor(line));

                let mut brace_found = false;
                let mut brace_depth = 0;
                let mut start_i = i;

                let mut j = i;
                while j < lines.len() {
                    let jl = &lines[j];
                    if jl.contains('{') {
                        brace_found = true;
                        brace_depth = jl.chars().filter(|&c| c == '{').count() as i32
                                    - jl.chars().filter(|&c| c == '}').count() as i32;
                        start_i = j;
                        break;
                    }
                    j += 1;
                }

                if brace_found {
                    if brace_depth <= 0 {
                        for k in (i + 1)..=j {
                            result.push(lines[k].clone());
                        }
                        i = j + 1;
                        continue;
                    }

                    let mut k = start_i + 1;
                    let mut collapsed_any = false;
                    while k < lines.len() {
                        let kl = &lines[k];
                        let opened = kl.chars().filter(|&c| c == '{').count() as i32;
                        let closed = kl.chars().filter(|&c| c == '}').count() as i32;
                        brace_depth += opened - closed;

                        if brace_depth <= 0 {
                            let indent = kl.len() - kl.trim_start().len();
                            let indent_str = " ".repeat(indent + 4);
                            result.push(format!("{}// ... §{} ...", indent_str, anchor));
                            result.push(kl.clone());
                            collapsed_any = true;
                            i = k + 1;
                            break;
                        }
                        k += 1;
                    }

                    if !collapsed_any {
                        result.push(line.clone());
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            } else {
                result.push(line.clone());
                i += 1;
            }
        }
    }

    result.join("\n")
}

fn is_function_decl(trimmed: &str, ext: &str) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    
    let excluded_keywords = ["if", "for", "while", "match", "switch", "let", "const", "var", "import", "export", "return", "class", "struct", "impl", "interface", "enum", "type"];
    for kw in excluded_keywords {
        if trimmed == kw || trimmed.starts_with(&format!("{kw} ")) {
            if kw == "const" && (trimmed.contains("=> {") || trimmed.contains("=>")) {
                continue;
            }
            return false;
        }
    }

    if ext == "rs" {
        (trimmed.contains("fn ") || trimmed.starts_with("fn ")) && !trimmed.ends_with(';')
    } else if ext == "swift" {
        trimmed.contains("func ") || trimmed.starts_with("func ")
    } else if ext == "js" || ext == "ts" || ext == "jsx" || ext == "tsx" {
        trimmed.contains("function ") || trimmed.starts_with("function ") || trimmed.contains("=>") || (trimmed.contains('(') && trimmed.contains('{'))
    } else {
        trimmed.contains("fn ") || trimmed.contains("func ") || trimmed.contains("def ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_function_decl() {
        assert!(is_function_decl("pub fn test() {", "rs"));
        assert!(!is_function_decl("let x = 10;", "rs"));
        assert!(is_function_decl("async bar() {", "ts"));
    }

    #[test]
    fn test_generate_skeleton_rust() {
        let content = "fn foo() {\n    println!(\"hello\");\n}";
        let anchors = vec!["Apple§123".to_string(), "Banana§456".to_string(), "Cherry§789".to_string()];
        let path = Path::new("test.rs");
        let skeleton = generate_skeleton(&path, content, &anchors);
        assert!(skeleton.contains("// ... §Apple§123 ..."));
        assert!(!skeleton.contains("println!"));
    }
}
