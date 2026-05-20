use super::CommentBatch;
use crate::automation::live_room::types::LiveComment;

pub fn parse_scan_comments(raw: serde_json::Value) -> Result<CommentBatch, String> {
    let next_cursor = raw
        .get("nextCursor")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let comments = raw
        .get("comments")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "douyin_comments_missing".to_string())?
        .iter()
        .map(|item| LiveComment {
            platform: "douyin".to_string(),
            platform_comment_id: item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            author_id: item
                .get("userId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            author_name: item
                .get("nickname")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            text: item
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            timestamp_ms: item.get("ts").and_then(|v| v.as_i64()).unwrap_or_default(),
            badges: Vec::new(),
            is_new: true,
        })
        .collect();
    Ok(CommentBatch {
        next_cursor,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fixture_comments() {
        let raw = serde_json::json!({
            "nextCursor": "c2",
            "comments": [
                {"id":"m1","userId":"u1","nickname":"Alice","text":"价格多少","ts":1000}
            ]
        });
        let batch = parse_scan_comments(raw).unwrap();
        assert_eq!(batch.next_cursor.as_deref(), Some("c2"));
        assert_eq!(batch.comments[0].platform, "douyin");
        assert_eq!(batch.comments[0].platform_comment_id, "m1");
        assert_eq!(batch.comments[0].author_id, "u1");
        assert_eq!(batch.comments[0].text, "价格多少");
    }
}
