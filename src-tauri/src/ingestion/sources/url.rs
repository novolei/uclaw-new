//! URL 抓取 + 正文抽取(reqwest + scraper)。

use crate::ingestion::job::IngestError;

pub async fn fetch_readable(url: &str) -> Result<String, IngestError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| IngestError::Io(e.to_string()))?;
    let html = resp.text().await.map_err(|e| IngestError::Io(e.to_string()))?;
    Ok(extract_main_text(&html))
}

/// 取 article/main/body 的可读文本,剥 script/style/nav。
fn extract_main_text(html: &str) -> String {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    for sel in ["article", "main", "body"] {
        if let Ok(s) = Selector::parse(sel) {
            if let Some(el) = doc.select(&s).next() {
                let text: String = el
                    .text()
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.trim().is_empty() {
                    return text;
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_text() {
        let html = "<html><body><nav>menu</nav><article><h1>Title</h1><p>正文段落</p></article></body></html>";
        let txt = extract_main_text(html);
        assert!(txt.contains("Title"));
        assert!(txt.contains("正文段落"));
    }
}
