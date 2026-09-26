//! Narrow filesystem observations for Changed File inspection and File Browser.
use std::{
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ID: AtomicUsize = AtomicUsize::new(0);
    struct Project(PathBuf);
    impl Project {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devscope-browser-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn browser_listing_order_visibility_and_parent_are_explicit() {
        let p = Project::new();
        for directory in [
            "z-dir",
            "A-dir",
            "bin",
            "obj",
            "target",
            "NODE_MODULES",
            ".DEVScope",
        ] {
            fs::create_dir(p.0.join(directory)).unwrap();
        }
        for file in ["z.txt", "A.txt", ".hidden", ".git"] {
            fs::write(p.0.join(file), "text").unwrap();
        }
        let listing = read_project_directory(&p.0, Path::new(""));
        let names: Vec<_> = listing
            .entries
            .iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["A-dir", "bin", "obj", "z-dir", ".hidden", "A.txt", "z.txt"]
        );
        assert!(listing.error.is_none());
        assert!(!listing.incomplete);
        let child = read_project_directory(&p.0, Path::new("A-dir"));
        assert_eq!(child.entries.len(), 1);
        assert_eq!(child.entries[0].kind, BrowserEntryKind::Parent);
        assert_eq!(child.entries[0].path, Path::new(""));
        fs::write(p.0.join("A-dir/target"), "ordinary file").unwrap();
        assert_eq!(
            read_project_directory(&p.0, Path::new("A-dir")).entries[1].kind,
            BrowserEntryKind::File
        );
    }

    #[test]
    fn browser_listing_rejects_escape_excluded_and_missing_directories() {
        let p = Project::new();
        for path in [
            "../outside",
            "a/../outside",
            "./a",
            "/outside",
            "C:\\outside",
        ] {
            let listing = read_project_directory(&p.0, Path::new(path));
            assert_eq!(listing.error, Some(SafeTextError::UnsafePath));
            assert!(listing.entries.is_empty());
        }
        for path in ["target", "NODE_MODULES", ".git", ".devscope/nested"] {
            assert_eq!(
                read_project_directory(&p.0, Path::new(path)).error,
                Some(SafeTextError::UnsafePath)
            );
        }
        let missing = read_project_directory(&p.0, Path::new("missing"));
        assert_eq!(missing.error, Some(SafeTextError::Missing));
        assert_eq!(missing.entries[0].kind, BrowserEntryKind::Parent);
        assert!(
            read_project_directory(&p.0.join("missing"), Path::new(""))
                .entries
                .is_empty()
        );
    }

    #[test]
    fn browser_listing_limit_counts_hidden_raw_entries() {
        let p = Project::new();
        fs::write(p.0.join(".git"), "hidden").unwrap();
        for index in 0..MAX_DIRECTORY_ENTRIES - 1 {
            fs::write(p.0.join(format!("file-{index:04}")), "").unwrap();
        }
        let exact = read_project_directory(&p.0, Path::new(""));
        assert!(!exact.incomplete);
        assert_eq!(exact.entries.len(), MAX_DIRECTORY_ENTRIES - 1);
        fs::write(p.0.join("overflow"), "").unwrap();
        let overflow = read_project_directory(&p.0, Path::new(""));
        assert!(overflow.incomplete);
        assert!(overflow.entries.len() <= MAX_DIRECTORY_ENTRIES);
        assert!(overflow.error.is_none());
    }

    #[test]
    fn browser_lists_links_but_cannot_traverse_or_read_them() {
        let p = Project::new();
        let outside = Project::new();
        fs::write(outside.0.join("secret"), "secret").unwrap();
        fs::write(p.0.join("normal"), "normal").unwrap();
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&outside.0, p.0.join("link-dir")).unwrap();
            std::os::windows::fs::symlink_file(outside.0.join("secret"), p.0.join("link-file"))
                .unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside.0, p.0.join("link-dir")).unwrap();
            std::os::unix::fs::symlink(outside.0.join("secret"), p.0.join("link-file")).unwrap();
        }
        let listing = read_project_directory(&p.0, Path::new(""));
        assert_eq!(listing.entries[0].kind, BrowserEntryKind::File);
        assert!(
            listing.entries[1..]
                .iter()
                .all(|entry| entry.kind == BrowserEntryKind::UnsupportedLink)
        );
        assert_eq!(
            read_project_directory(&p.0, Path::new("link-dir")).error,
            Some(SafeTextError::Symlink)
        );
        for path in ["link-file", "link-dir/secret"] {
            assert_eq!(
                read_safe_text_file(&p.0, Path::new(path)),
                Err(SafeTextError::Symlink)
            );
        }
    }

    #[test]
    fn browser_shared_text_is_bounded_utf8_and_rejects_binary() {
        let p = Project::new();
        fs::create_dir(p.0.join("nested")).unwrap();
        let file = Path::new("nested/text");
        fs::write(p.0.join(file), "日本語\nUTF-8").unwrap();
        assert_eq!(
            read_safe_text_file(&p.0, file),
            Ok(("日本語\nUTF-8".into(), false))
        );
        fs::write(
            p.0.join(file),
            format!("{}日本語", "a".repeat(MAX_FILE_CONTENT_BYTES - 1)),
        )
        .unwrap();
        let (text, truncated) = read_safe_text_file(&p.0, file).unwrap();
        assert!(truncated);
        assert_eq!(text.len(), MAX_FILE_CONTENT_BYTES - 1);
        for bytes in [&b"nul\0"[..], &[0xff, 0xfe][..]] {
            fs::write(p.0.join(file), bytes).unwrap();
            assert_eq!(read_safe_text_file(&p.0, file), Err(SafeTextError::Binary));
        }
        assert_eq!(
            read_safe_text_file(&p.0, Path::new("../outside")),
            Err(SafeTextError::UnsafePath)
        );
        assert_eq!(
            read_safe_text_file(&p.0, Path::new("nested")),
            Err(SafeTextError::NotRegularFile)
        );
    }

    #[test]
    fn browser_iterator_and_metadata_errors_remain_visible() {
        let p = Project::new();
        fs::write(p.0.join("vanished"), "text").unwrap();
        let entry = fs::read_dir(&p.0).unwrap().next().unwrap().unwrap();
        fs::remove_file(p.0.join("vanished")).unwrap();
        let mut listing = DirectoryListing::default();
        append_directory_entries(
            &mut listing,
            Path::new(""),
            [Ok(entry), Err(io::Error::other("injected read error"))].into_iter(),
        );
        assert!(listing.incomplete);
        assert_eq!(listing.error, Some(SafeTextError::ReadError));
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "vanished");
        assert_eq!(listing.entries[0].kind, BrowserEntryKind::Error);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeTextError {
    Missing,
    UnsafePath,
    Symlink,
    NotRegularFile,
    Binary,
    ReadError,
}

const MAX_FILE_CONTENT_BYTES: usize = 64 * 1024;

const MAX_DIRECTORY_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BrowserEntryKind {
    Parent,
    Directory,
    File,
    UnsupportedLink,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub name: OsString,
    pub kind: BrowserEntryKind,
}

#[derive(Debug, Default)]
pub struct DirectoryListing {
    pub entries: Vec<BrowserEntry>,
    pub incomplete: bool,
    pub error: Option<SafeTextError>,
}

fn hidden_name(name: &std::ffi::OsStr, directory: bool) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.eq_ignore_ascii_case(".git")
        || name.eq_ignore_ascii_case(".devscope")
        || (directory
            && (name.eq_ignore_ascii_case("target") || name.eq_ignore_ascii_case("node_modules")))
}

fn checked_directory(root: &Path, relative: &Path) -> Result<PathBuf, SafeTextError> {
    if !relative.as_os_str().is_empty() && !safe_inspection_path(relative) {
        return Err(SafeTextError::UnsafePath);
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        if hidden_name(component.as_os_str(), true) {
            return Err(SafeTextError::UnsafePath);
        }
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(content_io_error)?;
        if is_inspection_link(&metadata) {
            return Err(SafeTextError::Symlink);
        }
        if !metadata.is_dir() {
            return Err(SafeTextError::NotRegularFile);
        }
    }
    // The project root keeps the caller's existing root semantics.
    if !fs::metadata(&path).map_err(content_io_error)?.is_dir() {
        return Err(SafeTextError::NotRegularFile);
    }
    if !path
        .canonicalize()
        .map_err(content_io_error)?
        .starts_with(root.canonicalize().map_err(content_io_error)?)
    {
        return Err(SafeTextError::UnsafePath);
    }
    Ok(path)
}

/// One level only; excluded entries count toward the raw enumeration budget.
pub fn read_project_directory(root: &Path, relative: &Path) -> DirectoryListing {
    let mut listing = DirectoryListing::default();
    // Invalid caller paths must not even manufacture a parent outside the root.
    if !relative.as_os_str().is_empty() && !safe_inspection_path(relative) {
        listing.error = Some(SafeTextError::UnsafePath);
        return listing;
    }
    if !relative.as_os_str().is_empty() {
        listing.entries.push(BrowserEntry {
            path: relative.parent().unwrap_or(Path::new("")).to_path_buf(),
            name: "..".into(),
            kind: BrowserEntryKind::Parent,
        });
    }
    let directory = match checked_directory(root, relative)
        .and_then(|path| fs::read_dir(path).map_err(content_io_error))
    {
        Ok(directory) => directory,
        Err(error) => {
            listing.error = Some(error);
            return listing;
        }
    };
    append_directory_entries(&mut listing, relative, directory);
    listing
}

fn append_directory_entries(
    listing: &mut DirectoryListing,
    relative: &Path,
    directory: impl Iterator<Item = io::Result<fs::DirEntry>>,
) {
    for (index, result) in directory.take(MAX_DIRECTORY_ENTRIES + 1).enumerate() {
        if index == MAX_DIRECTORY_ENTRIES {
            listing.incomplete = true;
            if result.is_err() {
                listing.error = Some(SafeTextError::ReadError);
            }
            break;
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => {
                listing.incomplete = true;
                listing.error = Some(SafeTextError::ReadError);
                continue;
            }
        };
        let name = entry.file_name();
        if hidden_name(&name, false) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path());
        if metadata
            .as_ref()
            .is_ok_and(|metadata| hidden_name(&name, metadata.is_dir()))
        {
            continue;
        }
        let kind = match metadata {
            Ok(metadata) if is_inspection_link(&metadata) => BrowserEntryKind::UnsupportedLink,
            Ok(metadata) if metadata.is_dir() => BrowserEntryKind::Directory,
            Ok(metadata) if metadata.is_file() => BrowserEntryKind::File,
            Ok(_) => BrowserEntryKind::Unsupported,
            Err(_) => BrowserEntryKind::Error,
        };
        listing.entries.push(BrowserEntry {
            path: relative.join(&name),
            name,
            kind,
        });
    }
    listing.entries.sort_by(|a, b| {
        entry_group(a.kind)
            .cmp(&entry_group(b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn entry_group(kind: BrowserEntryKind) -> u8 {
    match kind {
        BrowserEntryKind::Parent => 0,
        BrowserEntryKind::Directory => 1,
        BrowserEntryKind::File => 2,
        BrowserEntryKind::UnsupportedLink
        | BrowserEntryKind::Unsupported
        | BrowserEntryKind::Error => 3,
    }
}

pub(crate) fn safe_inspection_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    !text.is_empty()
        && !text.contains(['\0', ':'])
        && !text
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

pub(crate) fn content_io_error(error: io::Error) -> SafeTextError {
    if error.kind() == io::ErrorKind::NotFound {
        SafeTextError::Missing
    } else {
        SafeTextError::ReadError
    }
}

pub fn read_safe_text_file(root: &Path, path: &Path) -> Result<(String, bool), SafeTextError> {
    use SafeTextError as Reason;
    if !safe_inspection_path(path) {
        return Err(Reason::UnsafePath);
    }
    let mut target = root.to_path_buf();
    for component in path.components() {
        target.push(component);
        let metadata = std::fs::symlink_metadata(&target).map_err(content_io_error)?;
        if is_inspection_link(&metadata) {
            return Err(Reason::Symlink);
        }
        if !metadata.is_dir() && !metadata.is_file() {
            return Err(Reason::NotRegularFile);
        }
    }
    let metadata = std::fs::symlink_metadata(&target).map_err(content_io_error)?;
    if !metadata.is_file() {
        return Err(Reason::NotRegularFile);
    }
    // Component checks reject links before canonicalization or opening content.
    let canonical_root = root.canonicalize().map_err(content_io_error)?;
    if !target
        .canonicalize()
        .map_err(content_io_error)?
        .starts_with(&canonical_root)
    {
        return Err(Reason::UnsafePath);
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Do not follow a final-component reparse point replaced before open.
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(&target).map_err(content_io_error)?;
    let opened = file.metadata().map_err(content_io_error)?;
    if is_inspection_link(&opened) {
        return Err(Reason::Symlink);
    }
    if !opened.is_file() {
        return Err(Reason::NotRegularFile);
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE_CONTENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(content_io_error)?;
    let truncated = bytes.len() > MAX_FILE_CONTENT_BYTES;
    if bytes.contains(&0) {
        return Err(Reason::Binary);
    }
    if truncated {
        bytes.truncate(MAX_FILE_CONTENT_BYTES);
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if truncated && error.error_len().is_none() => {
            // A bounded read can split a valid UTF-8 character; omit that suffix.
            std::str::from_utf8(&bytes[..error.valid_up_to()]).map_err(|_| Reason::Binary)?
        }
        Err(_) => return Err(Reason::Binary),
    };
    Ok((text.to_owned(), truncated))
}

pub(crate) fn is_inspection_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points, not only symlinks.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}
