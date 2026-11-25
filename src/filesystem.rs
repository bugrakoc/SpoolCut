use std::{fs::File, io, path::Path, time::SystemTime};

use anyhow::{Result, ensure};

#[derive(PartialEq, Eq)]
pub(crate) struct SourceIdentity {
    id: FileId,
    size: u64,
    modified: SystemTime,
}

impl SourceIdentity {
    pub(crate) fn read(file: &File) -> Result<Self> {
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "source must be a regular file");
        Ok(Self {
            id: file_id(file, &metadata)?,
            size: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

#[cfg(unix)]
type FileId = (u64, u64);

#[cfg(unix)]
fn file_id(_: &File, metadata: &std::fs::Metadata) -> io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
#[derive(PartialEq, Eq)]
enum FileId {
    Extended(u64, [u8; 16]),
    Legacy(u32, u64),
}

#[cfg(windows)]
fn file_id(file: &File, _: &std::fs::Metadata) -> io::Result<FileId> {
    use std::{mem::size_of_val, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO, FileIdInfo, GetFileInformationByHandle,
        GetFileInformationByHandleEx,
    };
    let handle = file.as_raw_handle();
    let mut extended = FILE_ID_INFO::default();
    // SAFETY: The file handle and writable buffer are valid for the call.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            std::ptr::from_mut(&mut extended).cast(),
            size_of_val(&extended) as u32,
        )
    } != 0
    {
        return Ok(FileId::Extended(
            extended.VolumeSerialNumber,
            extended.FileId.Identifier,
        ));
    }
    // Some removable filesystems only expose the older file index.
    let mut legacy = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: The file handle and writable buffer are valid for the call.
    if unsafe { GetFileInformationByHandle(handle, &mut legacy) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(legacy.nFileIndexHigh) << 32) | u64::from(legacy.nFileIndexLow);
    Ok(FileId::Legacy(legacy.dwVolumeSerialNumber, index))
}

#[cfg(unix)]
pub(crate) fn publish(staged: &Path, output: &Path) -> io::Result<()> {
    std::fs::hard_link(staged, output)
}

#[cfg(windows)]
pub(crate) fn publish(staged: &Path, output: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>()
    };
    let staged = wide(staged);
    let output = wide(output);
    // The caller stages on the destination volume. No copy or replacement flags:
    // even an existing destination created concurrently must retain its contents.
    // SAFETY: Both path buffers are null-terminated and live through the call.
    if unsafe { MoveFileExW(staged.as_ptr(), output.as_ptr(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, fs::FileTimes};

    #[test]
    fn identifies_a_replacement_with_the_same_size_and_time() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.mp4");
        let replacement = directory.path().join("replacement.mp4");
        fs::write(&original, b"aaaa").unwrap();
        fs::write(&replacement, b"bbbb").unwrap();
        let original_file = File::open(&original).unwrap();
        let timestamp = original_file.metadata().unwrap().modified().unwrap();
        let replacement_file = File::options().write(true).open(&replacement).unwrap();
        replacement_file
            .set_times(FileTimes::new().set_modified(timestamp))
            .unwrap();
        let original_identity = SourceIdentity::read(&original_file).unwrap();
        let replacement_identity = SourceIdentity::read(&replacement_file).unwrap();
        assert_eq!(original_identity.size, replacement_identity.size);
        assert_eq!(original_identity.modified, replacement_identity.modified);
        assert!(original_identity.id != replacement_identity.id);
    }
}
