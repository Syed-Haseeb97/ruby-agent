//! Read-only inspection of Antigravity conversation traces and artifact metadata on disk.
//! Inspects `%USERPROFILE%\.gemini\antigravity\brain` without touching or running any tools.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Resolves the Antigravity brain directory on disk.
pub fn get_antigravity_brain_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let path = PathBuf::from(home)
        .join(".gemini")
        .join("antigravity")
        .join("brain");
    if path.exists() && path.is_dir() {
        Some(path)
    } else {
        None
    }
}

/// Inspects conversations or a specific conversation ID in the Antigravity brain directory.
pub fn inspect_antigravity_brain(target_id: Option<&str>) -> String {
    let brain_dir = match get_antigravity_brain_path() {
        Some(p) => p,
        None => {
            return "Antigravity brain directory not found at ~/.gemini/antigravity/brain"
                .to_string()
        }
    };

    inspect_brain_directory(&brain_dir, target_id)
}

/// Core brain directory inspector parameterized by path for unit testing.
pub fn inspect_brain_directory(brain_dir: &Path, target_id: Option<&str>) -> String {
    if !brain_dir.exists() || !brain_dir.is_dir() {
        return format!(
            "Antigravity brain directory does not exist: {}",
            brain_dir.display()
        );
    }

    if let Some(target) = target_id {
        let clean_target = target.trim();
        let entries = match std::fs::read_dir(brain_dir) {
            Ok(e) => e,
            Err(err) => return format!("Failed to read brain directory: {err}"),
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(clean_target) {
                let conv_path = entry.path();
                let mut report = format!("Antigravity Conversation: {}\nArtifacts:\n", name);
                let mut found_artifacts = 0;

                if let Ok(sub_entries) = std::fs::read_dir(&conv_path) {
                    for sub in sub_entries.flatten() {
                        let sub_name = sub.file_name().to_string_lossy().to_string();
                        if sub_name.ends_with(".metadata.json") {
                            found_artifacts += 1;
                            let art_name = sub_name.trim_end_matches(".metadata.json");
                            if let Ok(content) = std::fs::read_to_string(sub.path()) {
                                if let Ok(json_val) = serde_json::from_str::<Value>(&content) {
                                    let summary =
                                        json_val["summary"].as_str().unwrap_or("No summary");
                                    let updated =
                                        json_val["updatedAt"].as_str().unwrap_or("unknown");
                                    report.push_str(&format!(
                                        "• {} (updated: {}):\n  {}\n",
                                        art_name, updated, summary
                                    ));
                                }
                            }
                        }
                    }
                }

                if found_artifacts == 0 {
                    report.push_str("  (No artifacts recorded for this conversation)\n");
                }

                report.push_str("\n[Runtime Notice: Live Antigravity agent execution/thinking state is ephemeral in-process memory and cannot be derived without an active host bridge.]");
                return report;
            }
        }
        return format!(
            "No conversation matching prefix '{}' found in brain directory.",
            clean_target
        );
    }

    // List recent conversations
    let entries = match std::fs::read_dir(brain_dir) {
        Ok(e) => e,
        Err(err) => return format!("Failed to read brain directory: {err}"),
    };

    let mut convs = Vec::new();
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            let mod_time = entry.metadata().ok().and_then(|m| m.modified().ok());
            let mod_secs = mod_time
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let mut artifact_count = 0;
            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                for sub in sub_entries.flatten() {
                    let sub_name = sub.file_name().to_string_lossy().to_string();
                    if sub_name.ends_with(".md") && !sub_name.ends_with(".metadata.json") {
                        artifact_count += 1;
                    }
                }
            }
            convs.push((name, mod_secs, artifact_count));
        }
    }

    convs.sort_by_key(|a| std::cmp::Reverse(a.1));
    convs.truncate(10);

    let mut out = String::from("Antigravity Conversations (Recent Active):\n");
    for (id, _secs, arts) in convs {
        let prefix = if id.len() >= 8 { &id[..8] } else { &id };
        out.push_str(&format!("• {} ({}) - {} artifact(s)\n", prefix, id, arts));
    }
    out.push_str("\nSend /ag <id> to inspect conversation artifacts & summary.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_inspect_brain_directory_list_and_detail() {
        let temp = tempdir().unwrap();
        let conv_dir = temp.path().join("a1b2c3d4-test-conversation");
        std::fs::create_dir_all(&conv_dir).unwrap();

        let metadata_path = conv_dir.join("REPORT.md.metadata.json");
        std::fs::write(
            &metadata_path,
            r#"{"summary": "Test report summary for M1 acceptance", "updatedAt": "2026-11-17T12:00:00Z"}"#,
        )
        .unwrap();

        // 1. List conversations
        let list_output = inspect_brain_directory(temp.path(), None);
        assert!(list_output.contains("a1b2c3d4"));

        // 2. Detail query for prefix
        let detail_output = inspect_brain_directory(temp.path(), Some("a1b2c3d4"));
        assert!(detail_output.contains("REPORT.md"));
        assert!(detail_output.contains("Test report summary for M1 acceptance"));
        assert!(detail_output.contains("Runtime Notice"));
    }
}
