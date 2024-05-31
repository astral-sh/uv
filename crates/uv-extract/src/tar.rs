use std::path::{Component, Path, PathBuf};

/// Determine the path at which the given tar entry will be unpacked, when unpacking into `dst`.
///
/// See: <https://github.com/vorot93/tokio-tar/blob/87338a76092330bc6fe60de95d83eae5597332e1/src/entry.rs#L418>
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn unpacked_at(dst: &Path, entry: &Path) -> Option<PathBuf> {
    let mut file_dst = dst.to_path_buf();
    {
        for part in entry.components() {
            match part {
                // Leading '/' characters, root paths, and '.'
                // components are just ignored and treated as "empty
                // components"
                Component::Prefix(..) | Component::RootDir | Component::CurDir => {
                    continue;
                }

                // If any part of the filename is '..', then skip over
                // unpacking the file to prevent directory traversal
                // security issues.  See, e.g.: CVE-2001-1267,
                // CVE-2002-0399, CVE-2005-1918, CVE-2007-4131
                Component::ParentDir => return None,

                Component::Normal(part) => file_dst.push(part),
            }
        }
    }

    // Skip cases where only slashes or '.' parts were seen, because
    // this is effectively an empty filename.
    if *dst == *file_dst {
        return None;
    }

    // Skip entries without a parent (i.e. outside of FS root)
    file_dst.parent()?;

    Some(file_dst)
}
