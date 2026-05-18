use crate::browser::types::{
    BoundingBox, DOMElement, DOMState, DomElementRaw, DomStateRaw, TabInfo,
};
use std::collections::HashMap;

pub const DOM_QUERY_SCRIPT: &str = r#"
(function() {
    const INTERACTIVE_TAGS = new Set(['a','button','input','select','textarea','details','summary']);
    const INTERACTIVE_ROLES = new Set(['button','link','checkbox','radio','tab','menuitem','option','combobox','textbox','spinbutton','slider']);
    const INTERACTIVE_ATTRS = ['onclick','onchange','onsubmit','onkeydown','onkeyup'];

    function isInteractive(el) {
        const tag = el.tagName.toLowerCase();
        if (INTERACTIVE_TAGS.has(tag)) return true;
        for (const attr of INTERACTIVE_ATTRS) {
            if (el.hasAttribute(attr)) return true;
        }
        const role = el.getAttribute('role') || '';
        if (INTERACTIVE_ROLES.has(role)) return true;
        if (el.tabIndex >= 0 && el.tabIndex !== -1 && tag !== 'div' && tag !== 'span') return true;
        return false;
    }

    function isVisible(el) {
        const style = window.getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
        if (el.offsetWidth === 0 && el.offsetHeight === 0) return false;
        return true;
    }

    function isInViewport(rect) {
        return rect.top < window.innerHeight && rect.bottom > 0 &&
               rect.left < window.innerWidth && rect.right > 0;
    }

    function getXpath(el) {
        if (el.id) return '//*[@id="' + el.id + '"]';
        if (el === document.body) return '/html/body';
        const parent = el.parentElement;
        if (!parent) return '//' + el.tagName.toLowerCase();
        const siblings = Array.from(parent.children).filter(c => c.tagName === el.tagName);
        const idx = siblings.indexOf(el) + 1;
        return getXpath(parent) + '/' + el.tagName.toLowerCase() + '[' + idx + ']';
    }

    function getText(el) {
        const t = (el.innerText || el.textContent || el.value || el.placeholder || el.alt || el.title || '').trim();
        return t.substring(0, 80);
    }

    let idx = 0;
    const elements = [];

    document.querySelectorAll('*').forEach(function(el) {
        if (!isInteractive(el)) return;
        if (!isVisible(el)) return;

        el.setAttribute('data-uclaw-index', String(idx));
        const rect = el.getBoundingClientRect();

        const attrs = {};
        ['type','href','name','placeholder','role','aria-label','aria-describedby','value','disabled','checked'].forEach(function(a) {
            const v = el.getAttribute(a);
            if (v !== null) attrs[a] = v;
        });

        elements.push({
            index: idx,
            tag: el.tagName.toLowerCase(),
            text: getText(el),
            attributes: attrs,
            isInViewport: isInViewport(rect),
            xpath: getXpath(el),
            boundingBox: {
                x: rect.left + window.scrollX,
                y: rect.top + window.scrollY,
                width: rect.width,
                height: rect.height
            }
        });
        idx++;
    });

    return JSON.stringify({
        url: window.location.href,
        title: document.title,
        elements: elements,
        pageText: (document.body && document.body.innerText || '').substring(0, 40000)
    });
})()
"#;

pub fn dom_state_from_raw(raw: DomStateRaw, tabs: Vec<TabInfo>) -> DOMState {
    let elements = raw.elements.into_iter().map(|r| {
        let attrs: HashMap<String, Option<String>> = r.attributes
            .into_iter()
            .map(|(k, v)| (k, v.as_str().map(|s| s.to_string())))
            .collect();
        DOMElement {
            index: r.index,
            tag: r.tag,
            text: r.text,
            attributes: attrs,
            is_in_viewport: r.is_in_viewport,
            xpath: r.xpath,
            bounding_box: r.bounding_box.map(|bb| BoundingBox {
                x: bb.x, y: bb.y, width: bb.width, height: bb.height,
            }),
        }
    }).collect();

    DOMState {
        url: raw.url,
        title: raw.title,
        elements,
        page_text: raw.page_text.chars().take(40_000).collect(),
        tabs,
    }
}

pub fn format_dom_state_for_llm(state: &DOMState) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(&format!("URL: {}\nTitle: {}\n\n", state.url, state.title));

    for el in &state.elements {
        let vp = if el.is_in_viewport { "" } else { " [offscreen]" };
        let attrs: String = el.attributes.iter()
            .filter_map(|(k, v)| v.as_ref().map(|val| format!(" {k}={val}")))
            .collect::<Vec<_>>()
            .join("");
        let body = if el.text.is_empty() {
            attrs
        } else {
            format!("  {}{}", el.text, attrs)
        };
        out.push_str(&format!("[{}] <{}>{}{}\n", el.index, el.tag, vp, body));
    }

    if !state.page_text.is_empty() {
        out.push_str("\n=== PAGE TEXT ===\n");
        let truncated: String = state.page_text.chars().take(3000).collect();
        out.push_str(&truncated);
        if state.page_text.len() > 3000 {
            out.push_str("\n... [truncated]");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::types::{BoundingBoxRaw, DomElementRaw};

    #[test]
    fn elements_from_raw_maps_fields() {
        let raw = DomStateRaw {
            url: "https://example.com".into(),
            title: "Example".into(),
            page_text: "Hello world".into(),
            elements: vec![DomElementRaw {
                index: 0,
                tag: "button".into(),
                text: "Click".into(),
                attributes: Default::default(),
                is_in_viewport: true,
                xpath: "//button[1]".into(),
                bounding_box: Some(BoundingBoxRaw { x: 10.0, y: 20.0, width: 80.0, height: 30.0 }),
            }],
        };
        let state = dom_state_from_raw(raw, vec![]);
        assert_eq!(state.elements.len(), 1);
        assert_eq!(state.elements[0].index, 0);
        assert_eq!(state.elements[0].tag, "button");
        assert!(state.elements[0].bounding_box.is_some());
    }

    #[test]
    fn page_text_truncated_to_40000() {
        let long = "a".repeat(50_000);
        let raw = DomStateRaw {
            url: "".into(), title: "".into(),
            page_text: long,
            elements: vec![],
        };
        let state = dom_state_from_raw(raw, vec![]);
        assert!(state.page_text.len() <= 40_000);
    }

    #[test]
    fn format_dom_state_includes_url_and_index() {
        let state = DOMState {
            url: "https://example.com".into(),
            title: "Example".into(),
            page_text: "body text".into(),
            tabs: vec![],
            elements: vec![DOMElement {
                index: 5,
                tag: "a".into(),
                text: "Link".into(),
                attributes: Default::default(),
                is_in_viewport: true,
                xpath: "//a[1]".into(),
                bounding_box: None,
            }],
        };
        let formatted = format_dom_state_for_llm(&state);
        assert!(formatted.contains("https://example.com"), "missing url");
        assert!(formatted.contains("[5]"), "missing index");
        assert!(formatted.contains("<a>"), "missing tag");
        assert!(formatted.contains("Link"), "missing text");
    }
}
