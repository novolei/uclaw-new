//! 文本按 token 预算切块。token≈字符/4 粗估;按段落边界优先;块数上限截断。

/// 每块目标 token 预算。
pub const CHUNK_TOKEN_BUDGET: usize = 2500;
/// 每文档最大块数(成本兜底)。
pub const MAX_CHUNKS_PER_DOC: usize = 20;

/// 粗略 token 估算(字符数/4,CJK 偏保守)。
fn est_tokens(s: &str) -> usize {
    (s.chars().count() / 4).max(1)
}

/// 切块:按段落(空行分隔)累积到预算;超 MAX_CHUNKS 截断,返回 (chunks, truncated)。
pub fn split_chunks(text: &str, budget_tokens: usize, max_chunks: usize) -> (Vec<String>, bool) {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_tokens = 0usize;

    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let pt = est_tokens(para);
        if cur_tokens + pt > budget_tokens && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_tokens = 0;
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
        cur_tokens += pt;
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }

    let truncated = chunks.len() > max_chunks;
    if truncated {
        chunks.truncate(max_chunks);
    }
    (chunks, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_budget() {
        // 每段约 5 token(20 字符),预算 6 → 每段一块
        let text = "aaaaaaaaaaaaaaaaaaaa\n\nbbbbbbbbbbbbbbbbbbbb\n\ncccccccccccccccccccc";
        let (chunks, truncated) = split_chunks(text, 6, 100);
        assert_eq!(chunks.len(), 3);
        assert!(!truncated);
    }

    #[test]
    fn caps_at_max_chunks() {
        let text = (0..50).map(|i| format!("para{}aaaaaaaaaaaaaaaaa", i)).collect::<Vec<_>>().join("\n\n");
        let (chunks, truncated) = split_chunks(&text, 6, 10);
        assert_eq!(chunks.len(), 10);
        assert!(truncated);
    }

    #[test]
    fn empty_text_no_chunks() {
        let (chunks, truncated) = split_chunks("   \n\n  ", CHUNK_TOKEN_BUDGET, MAX_CHUNKS_PER_DOC);
        assert!(chunks.is_empty());
        assert!(!truncated);
    }
}
