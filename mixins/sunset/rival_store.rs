// Crystal Bot rival history only. The worker calls these checks at the point of
// use; passing a PathBuf from the game thread is not permission to follow links.
#[cfg(windows)]
use std::fs::File;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_BYTES: usize = 16_384;
const MAX_BATCH: usize = 64;
const MAX_SCAN: usize = 256;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "rival history requires plain files and folders",
    )
}

fn linked(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // Any reparse point, including junctions.
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn plain_file(path: &Path, optional: bool) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !linked(&meta) && meta.is_file() => Ok(()),
        Ok(_) => Err(invalid()),
        Err(e) if optional && e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

struct Directory {
    path: PathBuf,
    // Deny rename/delete of checked ancestors while the operation is running.
    #[cfg(windows)]
    _locks: Vec<File>,
}

fn directory(data: &Path, create: bool) -> io::Result<Directory> {
    if data
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid());
    }
    let wanted = std::path::absolute(data)?.join("rivals");
    let mut path = PathBuf::new();
    #[cfg(windows)]
    let mut locks = Vec::new();
    for part in wanted.components() {
        match part {
            Component::ParentDir => return Err(invalid()),
            Component::CurDir => continue,
            Component::Prefix(_) => {
                path.push(part);
                continue;
            }
            _ => path.push(part),
        }
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                match fs::create_dir(&path) {
                    Ok(()) => (),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                    Err(e) => return Err(e),
                }
                fs::symlink_metadata(&path)?
            }
            Err(e) => return Err(e),
        };
        if linked(&meta) || !meta.is_dir() {
            return Err(invalid());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let handle = OpenOptions::new()
                .access_mode(0x81) // FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES.
                .share_mode(0x1 | 0x2) // FILE_SHARE_READ | WRITE, without DELETE.
                .custom_flags(0x0200_0000 | 0x0020_0000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT.
                .open(&path)?;
            let actual = handle.metadata()?;
            if linked(&actual) || !actual.is_dir() {
                return Err(invalid());
            }
            locks.push(handle);
        }
    }
    Ok(Directory {
        path,
        #[cfg(windows)]
        _locks: locks,
    })
}

pub(super) fn read(data: &Path, uuid: u128) -> io::Result<String> {
    let folder = directory(data, false)?;
    let path = folder.path.join(format!("{uuid:032x}.json"));
    plain_file(&path, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // Open the reparse point itself.
    }
    let file = options.open(&path)?;
    let actual = file.metadata()?;
    if linked(&actual) || !actual.is_file() || actual.len() > MAX_BYTES as u64 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    String::from_utf8(bytes).map_err(|_| invalid())
}

struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        // Only a path successfully created with create_new is ever owned here.
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

fn write(folder: &Directory, uuid: u128, text: &str) -> io::Result<()> {
    let path = folder.path.join(format!("{uuid:032x}.json"));
    plain_file(&path, true)?;
    for _ in 0..16 {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let tmp = folder
            .path
            .join(format!(".{uuid:032x}.{}.{serial}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let mut file = match options.open(&tmp) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let mut owned = Temporary(Some(tmp));
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        written?;
        plain_file(&path, true)?;
        fs::rename(owned.0.as_ref().unwrap(), &path)?;
        owned.0 = None;
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "rival history temporary names are occupied",
    ))
}

fn history_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|s| {
        s.len() == 37
            && s.ends_with(".json")
            && s.as_bytes()[..32].iter().all(u8::is_ascii_hexdigit)
    })
}

fn prune(folder: &Directory) -> io::Result<()> {
    let mut files = Vec::new();
    // Bound directory work too, including unrelated entries.
    for entry in fs::read_dir(&folder.path)?.take(MAX_SCAN) {
        let entry = entry?;
        if !history_name(&entry.file_name()) {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if !linked(&meta) && meta.is_file() {
            files.push((meta.modified()?, entry.path()));
        }
    }
    files.sort_by(|a, b| b.cmp(a));
    for (_, path) in files.into_iter().skip(MAX_BATCH) {
        // Recheck that pruning still names a plain file.
        plain_file(&path, false)?;
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn save(data: &Path, entries: &[(u128, String)]) -> io::Result<()> {
    if entries.len() > MAX_BATCH || entries.iter().any(|(_, text)| text.len() > MAX_BYTES) {
        return Err(invalid());
    }
    if entries.is_empty() {
        return Ok(());
    }
    let folder = directory(data, true)?;
    for (uuid, text) in entries {
        write(&folder, *uuid, text)?;
    }
    prune(&folder)
}
