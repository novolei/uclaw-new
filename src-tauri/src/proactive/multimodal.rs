use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::proactive::scenarios::types::{MultimodalInput, MultimodalSourceType};

const MAX_PENDING_ITEMS: usize = 50;

/// 多模态预处理器 - 将不同类型的输入转换为统一的文本表示
pub struct MultimodalPreprocessor;

impl MultimodalPreprocessor {
    /// 预处理图片 - 生成文本描述
    /// 实际实现会调用 Vision API，当前返回结构化占位文本
    pub async fn preprocess_image(
        _content_base64: &str,
        filename: Option<&str>,
        _vision_model: Option<&str>,
    ) -> anyhow::Result<(String, String)> {
        // 返回 (text_description, caption)
        // 当前为 placeholder，后续接入 LLM Vision API
        let fname = filename.unwrap_or("unknown_image");
        let caption = format!("[Image: {}]", fname);
        let text = format!(
            "Image file: {}\nFormat: detected from content\nDescription: [Pending Vision API analysis]",
            fname
        );
        Ok((text, caption))
    }

    /// 预处理文档 - 提取文本内容和结构化摘要
    pub async fn preprocess_document(
        content: &str,
        filename: Option<&str>,
    ) -> anyhow::Result<(String, String)> {
        let fname = filename.unwrap_or("unknown_document");
        let caption = format!("[Document: {}]", fname);
        // 截断过长内容
        let max_len = 50_000;
        let text = if content.len() > max_len {
            let safe_len = content
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i < max_len)
                .last()
                .unwrap_or(content.len().min(max_len));
            format!(
                "{}\n\n[Content truncated at {} chars, total: {} chars]",
                &content[..safe_len],
                max_len,
                content.len()
            )
        } else {
            content.to_string()
        };
        Ok((text, caption))
    }

    /// 预处理代码 - 提取函数签名、文档注释、依赖关系
    pub async fn preprocess_code(
        content: &str,
        filename: Option<&str>,
    ) -> anyhow::Result<(String, String)> {
        let fname = filename.unwrap_or("unknown_code");
        let caption = format!("[Code: {}]", fname);
        let text = format!(
            "Code file: {}\nLanguage: {}\nContent:\n{}",
            fname,
            Self::detect_language(fname),
            content
        );
        Ok((text, caption))
    }

    /// 检测编程语言
    fn detect_language(filename: &str) -> &str {
        match filename.rsplit('.').next() {
            Some("rs") => "Rust",
            Some("py") => "Python",
            Some("js") | Some("jsx") => "JavaScript",
            Some("ts") | Some("tsx") => "TypeScript",
            Some("go") => "Go",
            Some("java") => "Java",
            Some("swift") => "Swift",
            Some("kt") | Some("kts") => "Kotlin",
            Some("cpp") | Some("cc") | Some("cxx") => "C++",
            Some("c") | Some("h") => "C",
            Some("rb") => "Ruby",
            Some("php") => "PHP",
            _ => "Unknown",
        }
    }

    /// 统一预处理入口
    pub async fn preprocess(input: &MultimodalInput) -> anyhow::Result<(String, String)> {
        match input.source_type {
            MultimodalSourceType::Image => {
                Self::preprocess_image(&input.content_text, input.filename.as_deref(), None).await
            }
            MultimodalSourceType::Document => {
                Self::preprocess_document(&input.content_text, input.filename.as_deref()).await
            }
            MultimodalSourceType::Code => {
                Self::preprocess_code(&input.content_text, input.filename.as_deref()).await
            }
            MultimodalSourceType::Audio => {
                let fname = input.filename.as_deref().unwrap_or("unknown_audio");
                Ok((
                    format!(
                        "Audio file: {}\nTranscription: [Pending speech-to-text analysis]",
                        fname
                    ),
                    format!("[Audio: {}]", fname),
                ))
            }
        }
    }
}

/// 多模态输入队列 - 管理待处理的多模态输入
#[derive(Clone)]
pub struct MultimodalQueue {
    items: Arc<RwLock<VecDeque<MultimodalInput>>>,
    max_size: usize,
}

impl MultimodalQueue {
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(VecDeque::new())),
            max_size: MAX_PENDING_ITEMS,
        }
    }

    /// 添加多模态输入到队列
    pub async fn push(&self, input: MultimodalInput) {
        let mut q = self.items.write().await;
        if q.len() >= self.max_size {
            q.pop_front();
        }
        q.push_back(input);
    }

    /// 取出所有待处理的输入
    pub async fn drain_all(&self) -> Vec<MultimodalInput> {
        let mut q = self.items.write().await;
        q.drain(..).collect()
    }

    /// 获取待处理数量
    pub async fn pending_count(&self) -> usize {
        self.items.read().await.len()
    }

    /// 查看但不取出
    pub async fn peek_all(&self) -> Vec<MultimodalInput> {
        self.items.read().await.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proactive::scenarios::types::{MultimodalInput, MultimodalSourceType};

    fn make_input(source_type: MultimodalSourceType, content: &str, filename: Option<&str>) -> MultimodalInput {
        MultimodalInput {
            source_type,
            content_text: content.to_string(),
            caption: String::new(),
            mime_type: "application/octet-stream".to_string(),
            filename: filename.map(|s| s.to_string()),
            metadata: serde_json::Value::Null,
            ingested_at: 0,
        }
    }

    #[tokio::test]
    async fn test_preprocess_image() {
        let (text, caption) = MultimodalPreprocessor::preprocess_image(
            "base64data",
            Some("photo.png"),
            None,
        )
        .await
        .unwrap();
        assert!(caption.contains("photo.png"));
        assert!(text.contains("photo.png"));
        assert!(text.contains("Pending Vision API"));
    }

    #[tokio::test]
    async fn test_preprocess_document_short() {
        let content = "Hello world document content.";
        let (text, caption) = MultimodalPreprocessor::preprocess_document(content, Some("readme.txt"))
            .await
            .unwrap();
        assert_eq!(caption, "[Document: readme.txt]");
        assert_eq!(text, content);
    }

    #[tokio::test]
    async fn test_preprocess_document_truncation() {
        let long_content = "x".repeat(60_000);
        let (text, _caption) = MultimodalPreprocessor::preprocess_document(&long_content, None)
            .await
            .unwrap();
        assert!(text.contains("[Content truncated"));
        assert!(text.len() < long_content.len());
    }

    #[tokio::test]
    async fn test_preprocess_code_detects_language() {
        let (text, caption) = MultimodalPreprocessor::preprocess_code(
            "fn main() {}",
            Some("main.rs"),
        )
        .await
        .unwrap();
        assert_eq!(caption, "[Code: main.rs]");
        assert!(text.contains("Language: Rust"));
    }

    #[tokio::test]
    async fn test_preprocess_unified_entry() {
        let input = make_input(MultimodalSourceType::Audio, "", Some("speech.wav"));
        let (text, caption) = MultimodalPreprocessor::preprocess(&input).await.unwrap();
        assert!(caption.contains("Audio"));
        assert!(text.contains("speech.wav"));
    }

    #[test]
    fn test_detect_language_various() {
        assert_eq!(MultimodalPreprocessor::detect_language("app.ts"), "TypeScript");
        assert_eq!(MultimodalPreprocessor::detect_language("app.tsx"), "TypeScript");
        assert_eq!(MultimodalPreprocessor::detect_language("main.go"), "Go");
        assert_eq!(MultimodalPreprocessor::detect_language("lib.java"), "Java");
        assert_eq!(MultimodalPreprocessor::detect_language("no_ext"), "Unknown");
    }

    #[tokio::test]
    async fn test_queue_push_and_drain() {
        let queue = MultimodalQueue::new();
        let input = make_input(MultimodalSourceType::Image, "data", Some("img.png"));
        queue.push(input).await;
        assert_eq!(queue.pending_count().await, 1);

        let items = queue.drain_all().await;
        assert_eq!(items.len(), 1);
        assert_eq!(queue.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_queue_overflow_evicts_oldest() {
        let queue = MultimodalQueue::new();
        // Push MAX_PENDING_ITEMS + 1 items
        for i in 0..=MAX_PENDING_ITEMS {
            let input = make_input(
                MultimodalSourceType::Document,
                &format!("doc_{}", i),
                Some(&format!("doc_{}.txt", i)),
            );
            queue.push(input).await;
        }
        // Should not exceed max
        assert_eq!(queue.pending_count().await, MAX_PENDING_ITEMS);
        // The first item (doc_0) should have been evicted
        let items = queue.peek_all().await;
        assert_eq!(items[0].content_text, "doc_1");
    }

    #[tokio::test]
    async fn test_queue_peek_does_not_remove() {
        let queue = MultimodalQueue::new();
        let input = make_input(MultimodalSourceType::Code, "code", Some("main.py"));
        queue.push(input).await;

        let peeked = queue.peek_all().await;
        assert_eq!(peeked.len(), 1);
        // Still there after peek
        assert_eq!(queue.pending_count().await, 1);
    }
}
