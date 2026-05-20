//! 源探测 + 文本抽取分派。

pub mod text;
pub mod url;
pub mod pdf;

use crate::ingestion::job::{IngestError, IngestionSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Text,
    Pdf,
    Url,
    Media,
    Unsupported,
}

/// 抽出的纯文本 + 给用户看的来源标签。
#[derive(Debug, Clone)]
pub struct ExtractedDoc {
    pub text: String,
    pub source_label: String,
}

/// 按扩展名 / URL 探测源类型。
pub fn detect(source: &IngestionSource) -> SourceKind {
    match source {
        IngestionSource::Url(_) => SourceKind::Url,
        IngestionSource::File(path) => {
            let ext = path
                .rsplit('.')
                .next()
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            match ext.as_str() {
                "md" | "markdown" | "txt" | "text" => SourceKind::Text,
                "pdf" => SourceKind::Pdf,
                "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "mp4" | "mov" | "webm" => {
                    SourceKind::Media
                }
                _ => SourceKind::Unsupported,
            }
        }
    }
}

/// 抽文本入口。Media 在 Task 3 接入 —— 这里先对 Media 返回 Unsupported 占位,Task 3 改。
pub async fn extract_text(source: &IngestionSource) -> Result<ExtractedDoc, IngestError> {
    let label = source.label();
    match (detect(source), source) {
        (SourceKind::Text, IngestionSource::File(p)) => {
            Ok(ExtractedDoc { text: text::read_text_file(p)?, source_label: label })
        }
        (SourceKind::Pdf, IngestionSource::File(p)) => {
            Ok(ExtractedDoc { text: pdf::extract_pdf(p)?, source_label: label })
        }
        (SourceKind::Url, IngestionSource::Url(u)) => {
            Ok(ExtractedDoc { text: url::fetch_readable(u).await?, source_label: label })
        }
        (SourceKind::Media, _) => Err(IngestError::Unsupported("media (wired in Task 3)".into())),
        _ => Err(IngestError::Unsupported(label)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::job::IngestionSource;

    #[test]
    fn detect_by_extension() {
        assert_eq!(detect(&IngestionSource::File("a/b.md".into())), SourceKind::Text);
        assert_eq!(detect(&IngestionSource::File("a/b.PDF".into())), SourceKind::Pdf);
        assert_eq!(detect(&IngestionSource::File("a/b.mp3".into())), SourceKind::Media);
        assert_eq!(detect(&IngestionSource::File("a/b.mp4".into())), SourceKind::Media);
        assert_eq!(detect(&IngestionSource::File("a/b.xyz".into())), SourceKind::Unsupported);
        assert_eq!(detect(&IngestionSource::Url("https://x".into())), SourceKind::Url);
    }
}
