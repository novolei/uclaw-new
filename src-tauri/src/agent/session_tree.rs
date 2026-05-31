//! session_tree —— fork/rewind 谱系存取(Sprint 3 ③)。
//! lazy-materialize:agent_messages 是 source of truth;树按需从它构建。
//! 纯函数 over &rusqlite::Connection。读/写主路径不接管(getPathToRoot 备而不用)。
use crate::error::Error;
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TreeNode {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub entry_type: String,
    pub data_json: String,
    pub created_at: i64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 追加一个节点,返回其 id。created_at 显式传入(materialize 用消息 created_at,以便按时间剪枝/排序)。
pub fn append_node(
    conn: &Connection,
    session_id: &str,
    parent_id: Option<&str>,
    entry_type: &str,
    data_json: &str,
    created_at: i64,
) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO session_tree (id, session_id, parent_id, entry_type, data_json, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![id, session_id, parent_id, entry_type, data_json, created_at],
    )?;
    Ok(id)
}

pub fn get_leaf(conn: &Connection, session_id: &str) -> Result<Option<String>, Error> {
    let leaf: Option<String> = conn
        .query_row(
            "SELECT leaf_id FROM session_leaves WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    Ok(leaf)
}

pub fn set_leaf(conn: &Connection, session_id: &str, node_id: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO session_leaves (session_id, leaf_id, updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(session_id) DO UPDATE SET leaf_id = excluded.leaf_id, updated_at = excluded.updated_at",
        params![session_id, node_id, now_ms()],
    )?;
    Ok(())
}

fn count_messages(conn: &Connection, session_id: &str) -> Result<i64, Error> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
        params![session_id],
        |r| r.get(0),
    )?)
}

fn count_tree_message_nodes(conn: &Connection, session_id: &str) -> Result<i64, Error> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM session_tree WHERE session_id = ?1 AND entry_type = 'message'",
        params![session_id],
        |r| r.get(0),
    )?)
}

fn clear_session_tree(conn: &Connection, session_id: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM session_tree WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute(
        "DELETE FROM session_leaves WHERE session_id = ?1",
        params![session_id],
    )?;
    Ok(())
}

fn append_missing_message_nodes(conn: &Connection, session_id: &str) -> Result<(), Error> {
    let mut stmt = conn.prepare(
        "SELECT id, role, created_at FROM agent_messages WHERE session_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut parent = get_leaf(conn, session_id)?;
    let mut last = parent.clone();
    for (msg_id, role, created_at) in rows {
        if node_for_message(conn, session_id, &msg_id)?.is_some() {
            continue;
        }
        let data = serde_json::json!({ "message_id": msg_id, "role": role }).to_string();
        let node = append_node(
            conn,
            session_id,
            parent.as_deref(),
            "message",
            &data,
            created_at,
        )?;
        parent = Some(node.clone());
        last = Some(node);
    }
    if let Some(leaf) = last {
        set_leaf(conn, session_id, &leaf)?;
    }
    Ok(())
}

/// 幂等:确保树行与 agent_messages 保持同步,从消息构线性 message-node 链(created_at 序,parent=前一节点),
/// 节点 created_at = 消息 created_at;set_leaf 到末节点。
pub fn materialize_session_tree(conn: &Connection, session_id: &str) -> Result<(), Error> {
    let message_count = count_messages(conn, session_id)?;
    let tree_count = count_tree_message_nodes(conn, session_id)?;
    let leaf = get_leaf(conn, session_id)?;
    if message_count == tree_count && (message_count == 0 || leaf.is_some()) {
        return Ok(());
    }
    if tree_count > message_count || (message_count > 0 && leaf.is_none()) {
        clear_session_tree(conn, session_id)?;
    }
    append_missing_message_nodes(conn, session_id)?;
    Ok(())
}

/// 从 leaf 沿 parent_id 递归走到 root,返回 root→leaf(created_at 序)。本 slice 建好 + 单测;读路径暂不接管。
pub fn get_path_to_root(conn: &Connection, leaf_id: &str) -> Result<Vec<TreeNode>, Error> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE path(id, session_id, parent_id, entry_type, data_json, created_at) AS (
            SELECT id, session_id, parent_id, entry_type, data_json, created_at FROM session_tree WHERE id = ?1
            UNION ALL
            SELECT t.id, t.session_id, t.parent_id, t.entry_type, t.data_json, t.created_at
              FROM session_tree t JOIN path p ON t.id = p.parent_id
        )
        SELECT id, session_id, parent_id, entry_type, data_json, created_at FROM path ORDER BY created_at ASC",
    )?;
    let nodes = stmt
        .query_map(params![leaf_id], |r| {
            Ok(TreeNode {
                id: r.get(0)?,
                session_id: r.get(1)?,
                parent_id: r.get(2)?,
                entry_type: r.get(3)?,
                data_json: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(nodes)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStoreReplayEntry {
    pub sequence: usize,
    pub message_id: String,
    pub node_id: Option<String>,
    pub role: String,
    pub created_at: i64,
    pub compacted: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStoreIndexHealth {
    pub message_count: i64,
    pub tree_message_count: i64,
    pub leaf_id: Option<String>,
    pub fresh: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStoreCompactionAnchor {
    pub message_id: String,
    pub node_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStoreSnapshot {
    pub session_id: String,
    pub replay_entries: Vec<SessionStoreReplayEntry>,
    pub index_health: SessionStoreIndexHealth,
    pub compaction_anchor: Option<SessionStoreCompactionAnchor>,
}

fn load_replay_entries(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<SessionStoreReplayEntry>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, role, created_at, compacted FROM agent_messages WHERE session_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;
    let rows: Vec<(String, String, i64, i64)> = stmt
        .query_map(params![session_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    rows.into_iter()
        .enumerate()
        .map(|(sequence, (message_id, role, created_at, compacted))| {
            let node_id = node_for_message(conn, session_id, &message_id)?;
            Ok(SessionStoreReplayEntry {
                sequence,
                message_id,
                node_id,
                role,
                created_at,
                compacted: compacted != 0,
            })
        })
        .collect()
}

fn load_compaction_anchor(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<SessionStoreCompactionAnchor>, Error> {
    let anchor: Option<(String, i64)> = conn
        .query_row(
            "SELECT id, created_at FROM agent_messages WHERE session_id = ?1 AND compacted != 0 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((message_id, created_at)) = anchor else {
        return Ok(None);
    };
    let node_id = node_for_message(conn, session_id, &message_id)?.ok_or_else(|| {
        Error::Internal(format!(
            "compaction anchor {message_id} has no session_tree node after refresh"
        ))
    })?;
    Ok(Some(SessionStoreCompactionAnchor {
        message_id,
        node_id,
        created_at,
    }))
}

pub fn load_session_store_snapshot(
    conn: &Connection,
    session_id: &str,
) -> Result<SessionStoreSnapshot, Error> {
    materialize_session_tree(conn, session_id)?;
    let mut replay_entries = load_replay_entries(conn, session_id)?;
    if replay_entries.iter().any(|entry| entry.node_id.is_none()) {
        clear_session_tree(conn, session_id)?;
        append_missing_message_nodes(conn, session_id)?;
        replay_entries = load_replay_entries(conn, session_id)?;
    }

    let message_count = count_messages(conn, session_id)?;
    let tree_message_count = count_tree_message_nodes(conn, session_id)?;
    let leaf_id = get_leaf(conn, session_id)?;
    let fresh = message_count == tree_message_count
        && (message_count == 0 || leaf_id.is_some())
        && replay_entries.iter().all(|entry| entry.node_id.is_some());
    let compaction_anchor = load_compaction_anchor(conn, session_id)?;

    Ok(SessionStoreSnapshot {
        session_id: session_id.to_string(),
        replay_entries,
        index_health: SessionStoreIndexHealth {
            message_count,
            tree_message_count,
            leaf_id,
            fresh,
        },
        compaction_anchor,
    })
}

/// 找某 session 中对应 message_id 的节点 id。
pub(crate) fn node_for_message(
    conn: &Connection,
    session_id: &str,
    message_id: &str,
) -> Result<Option<String>, Error> {
    let id: Option<String> = conn.query_row(
        "SELECT id FROM session_tree WHERE session_id = ?1 AND entry_type = 'message' AND json_extract(data_json, '$.message_id') = ?2",
        params![session_id, message_id], |r| r.get(0),
    ).ok();
    Ok(id)
}

/// fork 返回:新会话 meta(前端 push 进 agentSessionsAtom)。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ForkResult {
    pub id: String,
    pub title: String,
    pub message_count: i64,
}

/// rewind 返回:删除条数(文件态 rewind 范围外)。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RewindResult {
    pub deleted: i64,
}

/// fork:把 source 中 created_at <= up_to_message 的消息复制进新会话,记录 fork 边。
pub fn fork_at(
    conn: &Connection,
    source_session: &str,
    up_to_message_id: &str,
) -> Result<ForkResult, Error> {
    materialize_session_tree(conn, source_session)?;

    let fork_ts: i64 = conn
        .query_row(
            "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
            params![up_to_message_id, source_session],
            |r| r.get(0),
        )
        .map_err(|_| {
            Error::NotFound(format!(
                "message {up_to_message_id} not in session {source_session}"
            ))
        })?;

    let (space_id, src_title, metadata_json, attached_dirs): (String, String, String, String) = conn.query_row(
        "SELECT space_id, title, metadata_json, attached_dirs FROM agent_sessions WHERE id = ?1",
        params![source_session],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).map_err(|_| Error::NotFound(format!("session {source_session}")))?;
    let new_id = uuid::Uuid::new_v4().to_string();
    let new_title = format!("{src_title} (fork)");
    let now = now_ms();

    let mut stmt = conn.prepare(
        "SELECT role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted \
         FROM agent_messages WHERE session_id = ?1 AND created_at <= ?2 ORDER BY created_at ASC, rowid ASC",
    )?;
    #[allow(clippy::type_complexity)]
    let copied: Vec<(
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<f64>,
        i64,
    )> = stmt
        .query_map(params![source_session, fork_ts], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    conn.execute(
        "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at, attached_dirs) VALUES (?1,?2,?3,?4,?5,0,0,?6,?6,?7)",
        params![new_id, space_id, new_title, metadata_json, copied.len() as i64, now, attached_dirs],
    )?;
    for (
        role,
        content,
        created_at,
        reasoning,
        tool_activities_json,
        events_json,
        model,
        duration_ms,
        input_tokens,
        output_tokens,
        cost_usd,
        compacted,
    ) in &copied
    {
        let nid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![nid, new_id, role, content, created_at, reasoning, tool_activities_json, events_json, model, duration_ms, input_tokens, output_tokens, cost_usd, compacted],
        )?;
    }

    materialize_session_tree(conn, &new_id)?;
    if let Some(src_node) = node_for_message(conn, source_session, up_to_message_id)? {
        conn.execute(
            "UPDATE session_tree SET parent_id = ?1 WHERE session_id = ?2 AND parent_id IS NULL",
            params![src_node, new_id],
        )?;
    }

    Ok(ForkResult {
        id: new_id,
        title: new_title,
        message_count: copied.len() as i64,
    })
}

/// rewind:删 target 之后的 agent_messages(保留含 target),重建本会话树,移 leaf。
pub fn rewind_to(
    conn: &Connection,
    session_id: &str,
    target_message_id: &str,
) -> Result<RewindResult, Error> {
    let target_ts: i64 = conn
        .query_row(
            "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
            params![target_message_id, session_id],
            |r| r.get(0),
        )
        .map_err(|_| {
            Error::NotFound(format!(
                "message {target_message_id} not in session {session_id}"
            ))
        })?;

    let deleted = conn.execute(
        "DELETE FROM agent_messages WHERE session_id = ?1 AND created_at > ?2",
        params![session_id, target_ts],
    )? as i64;

    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
        params![session_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE agent_sessions SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
        params![remaining, now_ms(), session_id],
    )?;

    conn.execute(
        "DELETE FROM session_tree WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute(
        "DELETE FROM session_leaves WHERE session_id = ?1",
        params![session_id],
    )?;
    materialize_session_tree(conn, session_id)?;

    Ok(RewindResult { deleted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use rusqlite::OptionalExtension;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        crate::db::migrations::run(&conn).expect("run migrations");
        conn
    }

    fn seed_session(conn: &Connection, session_id: &str, n: usize) -> Vec<String> {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES (?1,'default','S','{}',?2,0,0,1000,1000)",
            params![session_id, n as i64],
        ).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let mid = format!("m{i}");
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,?2,?3,?4,?5,0)",
                params![mid, session_id, role, format!("c{i}"), 1000 + i as i64],
            ).unwrap();
            ids.push(mid);
        }
        ids
    }

    fn append_message(conn: &Connection, session_id: &str, index: usize) -> String {
        let mid = format!("m{index}");
        let role = if index % 2 == 0 { "user" } else { "assistant" };
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,?2,?3,?4,?5,0)",
            params![mid, session_id, role, format!("c{index}"), 1000 + index as i64],
        ).unwrap();
        conn.execute(
            "UPDATE agent_sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
            params![1000 + index as i64, session_id],
        ).unwrap();
        mid
    }

    #[test]
    fn materialize_builds_linear_chain_and_is_idempotent() {
        let conn = setup_db();
        seed_session(&conn, "s1", 3);
        materialize_session_tree(&conn, "s1").unwrap();
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_tree WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 3);
        materialize_session_tree(&conn, "s1").unwrap();
        let cnt2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_tree WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt2, 3);
        let roots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_tree WHERE session_id='s1' AND parent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(roots, 1);
    }

    #[test]
    fn materialize_refreshes_stale_tree_after_append() {
        let conn = setup_db();
        seed_session(&conn, "s1", 2);
        materialize_session_tree(&conn, "s1").unwrap();

        let appended = append_message(&conn, "s1", 2);
        materialize_session_tree(&conn, "s1").unwrap();

        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_tree WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 3);
        assert!(node_for_message(&conn, "s1", &appended).unwrap().is_some());
        let leaf = get_leaf(&conn, "s1").unwrap().unwrap();
        let path = get_path_to_root(&conn, &leaf).unwrap();
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn get_path_to_root_returns_root_to_leaf() {
        let conn = setup_db();
        seed_session(&conn, "s1", 3);
        materialize_session_tree(&conn, "s1").unwrap();
        let leaf = get_leaf(&conn, "s1").unwrap().unwrap();
        let path = get_path_to_root(&conn, &leaf).unwrap();
        assert_eq!(path.len(), 3);
        assert!(path[0].parent_id.is_none());
        assert_eq!(path[2].id, leaf);
    }

    #[test]
    fn leaf_round_trips() {
        let conn = setup_db();
        seed_session(&conn, "s1", 1);
        materialize_session_tree(&conn, "s1").unwrap();
        let leaf = get_leaf(&conn, "s1").unwrap().unwrap();
        set_leaf(&conn, "s1", &leaf).unwrap();
        assert_eq!(get_leaf(&conn, "s1").unwrap().unwrap(), leaf);
    }

    #[test]
    fn fork_at_copies_messages_and_records_edge() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 4);
        let res = fork_at(&conn, "s1", &ids[1]).unwrap();
        assert_eq!(res.message_count, 2);
        assert!(res.title.ends_with("(fork)"));
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
                params![res.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        let root_parent: Option<String> = conn.query_row(
            "SELECT parent_id FROM session_tree WHERE session_id = ?1 AND parent_id IS NOT NULL ORDER BY created_at ASC LIMIT 1",
            params![res.id], |r| r.get(0)).optional().unwrap().flatten();
        assert!(root_parent.is_some());
        let src_n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src_n, 4);
    }

    #[test]
    fn fork_at_records_edge_for_message_appended_after_materialize() {
        let conn = setup_db();
        seed_session(&conn, "s1", 2);
        materialize_session_tree(&conn, "s1").unwrap();
        let appended = append_message(&conn, "s1", 2);

        let res = fork_at(&conn, "s1", &appended).unwrap();

        assert_eq!(res.message_count, 3);
        let src_node = node_for_message(&conn, "s1", &appended).unwrap().unwrap();
        let root_parent: Option<String> = conn.query_row(
            "SELECT parent_id FROM session_tree WHERE session_id = ?1 ORDER BY created_at ASC LIMIT 1",
            params![res.id],
            |r| r.get(0),
        ).optional().unwrap().flatten();
        assert_eq!(root_parent.as_deref(), Some(src_node.as_str()));
    }

    #[test]
    fn snapshot_returns_replay_entries_in_message_order() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 3);

        let snapshot = load_session_store_snapshot(&conn, "s1").unwrap();

        let replay_ids: Vec<_> = snapshot
            .replay_entries
            .iter()
            .map(|entry| entry.message_id.as_str())
            .collect();
        assert_eq!(replay_ids, ids);
        assert_eq!(snapshot.replay_entries[0].sequence, 0);
        assert_eq!(snapshot.replay_entries[2].sequence, 2);
        assert!(
            snapshot
                .replay_entries
                .iter()
                .all(|entry| entry.node_id.is_some())
        );
        assert!(snapshot.index_health.fresh);
        assert_eq!(snapshot.index_health.message_count, 3);
        assert_eq!(snapshot.index_health.tree_message_count, 3);
    }

    #[test]
    fn snapshot_refreshes_stale_tree_before_reporting_index_health() {
        let conn = setup_db();
        seed_session(&conn, "s1", 2);
        materialize_session_tree(&conn, "s1").unwrap();
        let appended = append_message(&conn, "s1", 2);

        let snapshot = load_session_store_snapshot(&conn, "s1").unwrap();

        assert!(snapshot.index_health.fresh);
        assert_eq!(snapshot.index_health.message_count, 3);
        assert_eq!(snapshot.index_health.tree_message_count, 3);
        assert_eq!(
            snapshot
                .replay_entries
                .last()
                .map(|entry| entry.message_id.as_str()),
            Some(appended.as_str())
        );
        assert!(snapshot.index_health.leaf_id.is_some());
    }

    #[test]
    fn snapshot_resolves_latest_compacted_message_anchor() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 4);
        conn.execute(
            "UPDATE agent_messages SET compacted = 1 WHERE id IN (?1, ?2)",
            params![ids[0], ids[2]],
        )
        .unwrap();

        let snapshot = load_session_store_snapshot(&conn, "s1").unwrap();
        let anchor = snapshot.compaction_anchor.unwrap();

        assert_eq!(anchor.message_id, ids[2]);
        assert_eq!(anchor.created_at, 1002);
        assert_eq!(
            Some(anchor.node_id.as_str()),
            node_for_message(&conn, "s1", &ids[2]).unwrap().as_deref()
        );
    }

    #[test]
    fn rewind_to_truncates_after_target() {
        let conn = setup_db();
        let ids = seed_session(&conn, "s1", 4);
        let res = rewind_to(&conn, "s1", &ids[1]).unwrap();
        assert_eq!(res.deleted, 2);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        let mc: i64 = conn
            .query_row(
                "SELECT message_count FROM agent_sessions WHERE id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mc, 2);
        let tn: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_tree WHERE session_id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tn, 2);
    }

    #[test]
    fn fork_unknown_message_errors() {
        let conn = setup_db();
        seed_session(&conn, "s1", 2);
        assert!(matches!(
            fork_at(&conn, "s1", "nope"),
            Err(Error::NotFound(_))
        ));
    }
}
