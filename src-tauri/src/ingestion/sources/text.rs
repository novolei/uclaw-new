//! md/txt 直读。

use crate::ingestion::job::IngestError;

pub fn read_text_file(path: &str) -> Result<String, IngestError> {
    std::fs::read_to_string(path).map_err(|e| IngestError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_temp_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "# Hello\n刘磊").unwrap();
        let txt = read_text_file(f.path().to_str().unwrap()).unwrap();
        assert!(txt.contains("Hello"));
        assert!(txt.contains("刘磊"));
    }
}
