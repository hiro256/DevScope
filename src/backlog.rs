//! Read-only candidates from the dedicated Backlog document, independent of Plan.

use std::{fs, io, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogCandidate {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Backlog {
    pub candidates: Vec<BacklogCandidate>,
}

/// Missing documents are normal; an absent candidate section produces an empty Backlog.
pub fn read_backlog(root: &Path) -> io::Result<Option<Backlog>> {
    match fs::read_to_string(root.join("docs/backlog.md")) {
        Ok(text) => parse_backlog(&text).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Parse only top-level `- **Title.** description` entries in the exact section.
pub fn parse_backlog(text: &str) -> io::Result<Backlog> {
    let mut backlog = Backlog::default();
    let mut in_section = false;
    let mut current: Option<usize> = None;
    for (index, line) in text.lines().enumerate() {
        if line.trim_end() == "## Implementation candidates" && !in_section {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if line.starts_with("## ") || line == "##" {
            break;
        }
        if let Some(item) = line.strip_prefix("- **") {
            let malformed = || {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed Backlog candidate title at line {}", index + 1),
                )
            };
            let (title, description) = item.split_once("**").ok_or_else(malformed)?;
            let title = title
                .trim()
                .strip_suffix('.')
                .unwrap_or(title.trim())
                .trim();
            if title.is_empty()
                || (!description.is_empty() && !description.starts_with(char::is_whitespace))
            {
                return Err(malformed());
            }
            backlog.candidates.push(BacklogCandidate {
                title: title.into(),
                description: description.trim().into(),
            });
            current = Some(backlog.candidates.len() - 1);
        } else if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
            // Other top-level items, including task checkboxes, are not candidates.
            current = None;
        } else if line.starts_with(char::is_whitespace)
            && let Some(current) = current
            && !line.trim().is_empty()
        {
            let description = &mut backlog.candidates[current].description;
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str(line.trim());
        } else if !line.trim().is_empty() {
            current = None;
        }
    }
    Ok(backlog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_titles_descriptions_and_order_only_in_the_candidate_section() {
        let backlog = parse_backlog(
            "- **Outside.** ignored\n## Implementation candidates\n\n\
             - **First.** opening\n  wrapped description\n  - **Nested.** context only\n\
             - [ ] Not a candidate\n- **日本語** second\n\
             ## Promotion flow\n- **After.** ignored\n",
        )
        .unwrap();
        assert_eq!(
            backlog.candidates,
            vec![
                BacklogCandidate {
                    title: "First".into(),
                    description: "opening wrapped description - **Nested.** context only".into()
                },
                BacklogCandidate {
                    title: "日本語".into(),
                    description: "second".into()
                },
            ]
        );
    }

    #[test]
    fn absent_or_empty_exact_section_has_no_candidates() {
        for text in [
            "",
            "## Other\n- **Title.** text",
            "## Implementation candidates\n",
            "### Implementation candidates\n- **Title.** text",
            "## Implementation candidates\n- [ ] task",
        ] {
            assert!(parse_backlog(text).unwrap().candidates.is_empty());
        }
    }

    #[test]
    fn malformed_candidate_titles_are_explicit_errors() {
        for item in [
            "- **Unclosed",
            "- **** description",
            "- **.** text",
            "- **Title**joined",
        ] {
            let error =
                parse_backlog(&format!("## Implementation candidates\n{item}\n")).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("line 2"));
        }
    }

    #[test]
    fn reads_only_the_dedicated_document_and_distinguishes_read_errors() {
        let root = std::env::temp_dir().join(format!(
            "devscope-backlog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs")).unwrap();
        assert_eq!(read_backlog(&root).unwrap(), None);
        fs::write(
            root.join("docs/backlog.md"),
            "## Implementation candidates\n- **Candidate.** text",
        )
        .unwrap();
        assert_eq!(
            read_backlog(&root).unwrap().unwrap().candidates[0].title,
            "Candidate"
        );
        fs::write(
            root.join("docs/backlog.md"),
            "## Implementation candidates\n- **broken",
        )
        .unwrap();
        assert_eq!(
            read_backlog(&root).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::write(root.join("docs/backlog.md"), [0xff]).unwrap();
        assert_eq!(
            read_backlog(&root).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_dir_all(root).unwrap();
    }
}
