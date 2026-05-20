//! PDF 文本层抽取。扫描件无文本层 → 返回空串(上层标 Partial)。

use crate::ingestion::job::IngestError;

pub fn extract_pdf(path: &str) -> Result<String, IngestError> {
    let bytes = std::fs::read(path).map_err(|e| IngestError::Io(e.to_string()))?;
    pdf_extract::extract_text_from_mem(&bytes).map_err(|e| IngestError::Parse(e.to_string()))
}
