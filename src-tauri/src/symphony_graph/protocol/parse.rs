//! WORKFLOW.md parser.
//!
//! Format mirrors OpenAI Symphony's reference: a Markdown document with a
//! YAML front-matter block delimited by `---` fences, followed by an
//! optional Markdown body that serves as the default prompt template for
//! nodes whose `prompt` field is empty.
//!
//! ```text
//! ---
//! id: wf-demo
//! name: Demo workflow
//! nodes:
//!   - id: a
//!     label: First step
//! ---
//! You are working on workflow {{ workflow.name }}.
//! Use the existing project conventions. Report back via report_to_user.
//! ```
//!
//! Both halves are optional. A pure-YAML document (no `---` fences) is also
//! accepted — the YAML may live at the top level. A pure-Markdown document
//! (no YAML) is rejected because we have no workflow id to anchor the run.
//!
//! Crate choice: `serde_yml` (same as `automation::protocol::parse`).

use thiserror::Error;

use super::types::SymphonyWorkflowDef;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("workflow body missing YAML front matter and could not be parsed as standalone YAML: {0}")]
    NoYaml(String),
    #[error("yaml syntax: {0}")]
    YamlSyntax(#[from] serde_yml::Error),
    #[error("workflow must declare at least one node")]
    EmptyNodes,
    #[error("workflow id and name are required")]
    MissingIdOrName,
}

/// Parse a WORKFLOW.md string into a `SymphonyWorkflowDef`.
///
/// The Markdown body (after the closing `---` fence) is preserved on the
/// returned struct's nodes only when a node's own `prompt` field is empty
/// — in that case the body is copied in as the per-node prompt template,
/// so a workflow that ships one shared prompt for every node doesn't have
/// to repeat itself.
pub fn parse_workflow_md(input: &str) -> Result<SymphonyWorkflowDef, ParseError> {
    let (yaml, body) = split_front_matter(input);

    let yaml = yaml.unwrap_or(input); // fall back to the whole document
    if yaml.trim().is_empty() {
        return Err(ParseError::NoYaml("empty document".to_string()));
    }

    let mut def: SymphonyWorkflowDef = serde_yml::from_str(yaml)?;

    if def.id.trim().is_empty() || def.name.trim().is_empty() {
        return Err(ParseError::MissingIdOrName);
    }
    if def.nodes.is_empty() {
        return Err(ParseError::EmptyNodes);
    }

    // Body fallback: any node with an empty prompt inherits the workflow's
    // Markdown body if one was provided.
    let body_trim = body.unwrap_or("").trim();
    if !body_trim.is_empty() {
        for n in &mut def.nodes {
            if n.prompt.trim().is_empty() {
                n.prompt = body_trim.to_string();
            }
        }
    }

    Ok(def)
}

/// Returns `(yaml, body)` when the document has a `---`-fenced front matter.
/// Otherwise returns `(None, None)` and the caller should treat the entire
/// document as YAML.
fn split_front_matter(input: &str) -> (Option<&str>, Option<&str>) {
    let trimmed = input.trim_start_matches('\u{feff}'); // BOM
    let trimmed = trimmed.trim_start();
    if !trimmed.starts_with("---") {
        return (None, None);
    }
    // Find the closing `---` on its own line after the opening one.
    let after_open = match trimmed.find('\n') {
        Some(i) => &trimmed[i + 1..],
        None => return (None, None),
    };
    let close_pat = "\n---";
    let close_idx = match after_open.find(close_pat) {
        Some(i) => i,
        None => return (None, None),
    };
    let yaml = &after_open[..close_idx];
    // Body is everything past the closing fence's newline.
    let body_start = close_idx + close_pat.len();
    let body = if body_start < after_open.len() {
        // Skip the rest of the closing-fence line and the following newline.
        let rest = &after_open[body_start..];
        let after_line = rest.find('\n').map(|i| &rest[i + 1..]).unwrap_or("");
        Some(after_line)
    } else {
        None
    };
    (Some(yaml), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_workflow() {
        let src = "---\nid: wf-1\nname: Demo\nnodes:\n  - id: a\n    label: A\n---\nshared body";
        let def = parse_workflow_md(src).unwrap();
        assert_eq!(def.id, "wf-1");
        assert_eq!(def.name, "Demo");
        assert_eq!(def.nodes.len(), 1);
        assert_eq!(def.nodes[0].prompt, "shared body");
    }

    #[test]
    fn parses_two_node_workflow_with_deps() {
        let src = "\
---
id: wf-chain
name: Chain
nodes:
  - id: fetch
    label: Fetch
    prompt: 'fetch the data'
  - id: process
    label: Process
    deps: [fetch]
edges:
  - from: fetch
    to: process
    label: data
---
";
        let def = parse_workflow_md(src).unwrap();
        assert_eq!(def.nodes.len(), 2);
        assert_eq!(def.nodes[1].deps, vec!["fetch".to_string()]);
        assert_eq!(def.edges.len(), 1);
        assert_eq!(def.edges[0].label.as_deref(), Some("data"));
    }

    #[test]
    fn body_only_fills_empty_prompts() {
        let src = "---\nid: wf\nname: N\nnodes:\n  - id: a\n    label: A\n    prompt: 'explicit'\n  - id: b\n    label: B\n---\nshared default";
        let def = parse_workflow_md(src).unwrap();
        assert_eq!(def.nodes[0].prompt, "explicit");
        assert_eq!(def.nodes[1].prompt, "shared default");
    }

    #[test]
    fn pure_yaml_document_works() {
        let src = "id: wf\nname: N\nnodes:\n  - id: a\n    label: A\n";
        let def = parse_workflow_md(src).unwrap();
        assert_eq!(def.id, "wf");
    }

    #[test]
    fn rejects_missing_id() {
        let src = "---\nname: N\nnodes:\n  - id: a\n    label: A\n---";
        let err = parse_workflow_md(src).unwrap_err();
        assert!(matches!(err, ParseError::YamlSyntax(_) | ParseError::MissingIdOrName));
    }

    #[test]
    fn rejects_empty_nodes() {
        let src = "---\nid: wf\nname: N\nnodes: []\n---";
        let err = parse_workflow_md(src).unwrap_err();
        assert!(matches!(err, ParseError::EmptyNodes));
    }

    #[test]
    fn rejects_empty_document() {
        let err = parse_workflow_md("").unwrap_err();
        assert!(matches!(err, ParseError::NoYaml(_)));
    }

    #[test]
    fn bom_prefix_does_not_break_front_matter_split() {
        let src = "\u{feff}---\nid: wf\nname: N\nnodes:\n  - id: a\n    label: A\n---\nbody";
        let def = parse_workflow_md(src).unwrap();
        assert_eq!(def.id, "wf");
        assert_eq!(def.nodes[0].prompt, "body");
    }
}
