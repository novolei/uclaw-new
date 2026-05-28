use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
pub enum ArtifactStoreError {
    Io(std::io::Error),
    Serde(serde_json::Error),
}

impl fmt::Display for ArtifactStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactStoreError::Io(err) => write!(f, "artifact IO error: {err}"),
            ArtifactStoreError::Serde(err) => write!(f, "artifact JSON error: {err}"),
        }
    }
}

impl std::error::Error for ArtifactStoreError {}

impl From<std::io::Error> for ArtifactStoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ArtifactStoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalArtifact {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub path: String,
    pub mime_type: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct EvalArtifactStore {
    root: PathBuf,
}

impl EvalArtifactStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn write_json(
        &self,
        run_id: &str,
        kind: &str,
        value: &Value,
    ) -> Result<EvalArtifact, ArtifactStoreError> {
        let artifact_id = format!("artifact-{}", uuid::Uuid::new_v4());
        let run_dir = self.root.join(sanitize_path_segment(run_id));
        fs::create_dir_all(&run_dir)?;
        let path = run_dir.join(format!("{artifact_id}.json"));
        fs::write(&path, serde_json::to_vec_pretty(value)?)?;

        Ok(EvalArtifact {
            id: artifact_id,
            run_id: run_id.to_string(),
            kind: kind.to_string(),
            path: path.to_string_lossy().to_string(),
            mime_type: "application/json".to_string(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            metadata: Value::Null,
        })
    }
}

fn sanitize_path_segment(input: &str) -> String {
    input.replace(['/', '\\', '.', ':'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn artifact_store_writes_json_under_run_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = EvalArtifactStore::new(tmp.path());

        let artifact = store
            .write_json("run/1", "tool_result", &json!({ "ok": true }))
            .unwrap();

        assert_eq!(artifact.run_id, "run/1");
        assert_eq!(artifact.kind, "tool_result");
        assert!(artifact.path.contains("run_1"));
        let content = fs::read_to_string(&artifact.path).unwrap();
        assert!(content.contains("\"ok\": true"), "{content}");
    }
}
