//! Bounded disk usage sampling for long-running benchmark units.
//!
//! Filesystem capacity is sampled at the monitor's fixed work, report, and
//! evidence roots, independently of the paths whose usage a phase records.
//! The reported free-space values are the minimum across those filesystems,
//! while used space is summed once per distinct filesystem. This keeps disk
//! deltas comparable across phases and lets headroom checks protect every
//! configured volume. `sampledPeakUsedBytes` is the largest sampled value
//! observed by this monitor; it is not a continuous
//! peak. `reclaimedBytesSincePreviousSample` is only the decrease between two
//! samples and does not attribute that decrease to a particular operation.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions, ReadDir},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_DIRECTORY_ENTRIES: u64 = 1_000_000;
const MAX_DIRECTORY_DEPTH: usize = 256;
const MAX_RELEVANT_PATHS: usize = 128;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskPathUsage {
    pub label: String,
    pub path: String,
    pub logical_bytes: u64,
    /// `None` means the host platform did not expose allocated size for every
    /// entry beneath this path.
    pub allocated_bytes: Option<u64>,
    pub entry_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskSample {
    pub phase: String,
    pub unit: String,
    pub sampled_at_unix_millis: u64,
    pub free_bytes: u64,
    /// `None` when the host or any sampled filesystem does not expose inode
    /// counts. The configured inode floor is enforced when this is available.
    pub free_inodes: Option<u64>,
    /// Filesystem usage, summed once for each distinct filesystem sampled.
    pub used_bytes: u64,
    /// High-water mark among samples made by this monitor, not a continuous
    /// measurement of transient usage.
    pub sampled_peak_used_bytes: u64,
    /// Observed decrease in filesystem usage since the preceding sample.
    /// This is `None` for the first sample and is not proof of reclamation by
    /// any particular operation.
    pub reclaimed_bytes_since_previous_sample: Option<u64>,
    /// Capacity and sampled usage for each distinct configured-root volume.
    pub filesystems: Vec<DiskFilesystemSample>,
    pub relevant_paths: Vec<DiskPathUsage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskFilesystemSample {
    pub filesystem: String,
    pub configured_roots: Vec<String>,
    pub free_bytes: u64,
    /// `None` explicitly records that this filesystem does not expose inode
    /// counts; a configured inode floor is enforced when available.
    pub free_inodes: Option<u64>,
    pub used_bytes: u64,
}

#[derive(Debug, Clone)]
struct Root {
    label: &'static str,
    path: PathBuf,
}

pub(crate) struct DiskMonitor {
    roots: [Root; 3],
    minimum_free_bytes: u64,
    minimum_free_inodes: u64,
    telemetry: File,
    previous_used_bytes: Option<u64>,
    sampled_peak_used_bytes: u64,
}

impl DiskMonitor {
    pub(crate) fn new(
        work_root: &Path,
        report_root: &Path,
        evidence_root: &Path,
        minimum_free_bytes: u64,
        minimum_free_inodes: u64,
    ) -> Result<Self> {
        fs::create_dir_all(evidence_root).with_context(|| {
            format!("create disk evidence directory {}", evidence_root.display())
        })?;
        let telemetry_path = evidence_root.join("disk.jsonl");
        let telemetry = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&telemetry_path)
            .with_context(|| format!("open disk telemetry {}", telemetry_path.display()))?;

        Ok(Self {
            roots: [
                Root {
                    label: "work",
                    path: work_root.to_path_buf(),
                },
                Root {
                    label: "report",
                    path: report_root.to_path_buf(),
                },
                Root {
                    label: "evidence",
                    path: evidence_root.to_path_buf(),
                },
            ],
            minimum_free_bytes,
            minimum_free_inodes,
            telemetry,
            previous_used_bytes: None,
            sampled_peak_used_bytes: 0,
        })
    }

    pub(crate) fn record(
        &mut self,
        phase: &str,
        unit: &str,
        paths: &[(&str, &Path)],
    ) -> Result<DiskSample> {
        if paths.is_empty() {
            bail!("disk sample must include at least one relevant path");
        }
        if paths.len() > MAX_RELEVANT_PATHS {
            bail!(
                "disk sample has {} relevant paths, exceeding the {MAX_RELEVANT_PATHS} path limit",
                paths.len()
            );
        }

        let mut relevant_paths = Vec::with_capacity(paths.len());
        let mut remaining_entries = MAX_DIRECTORY_ENTRIES;

        for (label, path) in paths {
            let (logical_bytes, allocated_bytes, entry_count) =
                measure_path(path, remaining_entries)?;
            remaining_entries = remaining_entries.saturating_sub(entry_count);
            relevant_paths.push(DiskPathUsage {
                label: (*label).to_owned(),
                path: path.to_string_lossy().into_owned(),
                logical_bytes,
                allocated_bytes,
                entry_count,
            });
        }

        // Keep the capacity sample set fixed across phases. A phase may report
        // usage for a narrower set of paths, but its peak/reclaimed values
        // still compare the same configured volumes.
        let mut filesystem_samples = BTreeMap::<String, DiskFilesystemSample>::new();
        for root in &self.roots {
            let stats = filesystem_stats(&root.path).with_context(|| {
                format!(
                    "sample filesystem containing configured {} root",
                    root.label
                )
            })?;
            match filesystem_samples.entry(stats.identity.clone()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().configured_roots.push(root.label.to_owned());
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(DiskFilesystemSample {
                        filesystem: stats.identity,
                        configured_roots: vec![root.label.to_owned()],
                        free_bytes: stats.free_bytes,
                        free_inodes: stats.free_inodes,
                        used_bytes: stats.used_bytes,
                    });
                }
            }
        }
        let filesystems: Vec<DiskFilesystemSample> = filesystem_samples.into_values().collect();
        let free_bytes = filesystems
            .iter()
            .map(|filesystem| filesystem.free_bytes)
            .min()
            .expect("the monitor always samples its three configured roots");
        let mut free_inodes = Some(u64::MAX);
        let mut used_bytes = 0_u64;
        for filesystem in &filesystems {
            free_inodes = match (free_inodes, filesystem.free_inodes) {
                (Some(current), Some(value)) => Some(current.min(value)),
                _ => None,
            };
            used_bytes = used_bytes.saturating_add(filesystem.used_bytes);
        }

        let sampled_peak_used_bytes = self.sampled_peak_used_bytes.max(used_bytes);
        let reclaimed_bytes_since_previous_sample = self
            .previous_used_bytes
            .map(|previous| previous.saturating_sub(used_bytes));
        let sample = DiskSample {
            phase: phase.to_owned(),
            unit: unit.to_owned(),
            sampled_at_unix_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock predates Unix epoch while sampling disk")?
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            free_bytes,
            free_inodes,
            used_bytes,
            sampled_peak_used_bytes,
            reclaimed_bytes_since_previous_sample,
            filesystems,
            relevant_paths,
        };

        serde_json::to_writer(&mut self.telemetry, &sample)
            .context("serialize disk telemetry record")?;
        self.telemetry
            .write_all(b"\n")
            .context("append disk telemetry newline")?;
        self.telemetry.flush().context("flush disk telemetry")?;
        self.telemetry
            .sync_data()
            .context("sync disk telemetry to storage")?;

        self.previous_used_bytes = Some(used_bytes);
        self.sampled_peak_used_bytes = sampled_peak_used_bytes;
        Ok(sample)
    }

    pub(crate) fn ensure_headroom(&mut self, unit: &str) -> Result<()> {
        let roots = self.roots.clone();
        let paths: Vec<(&str, &Path)> = roots
            .iter()
            .map(|root| (root.label, root.path.as_path()))
            .collect();
        let sample = self.record("headroom_check", unit, &paths)?;

        if sample.free_bytes < self.minimum_free_bytes {
            bail!(
                "insufficient disk headroom before unit {unit}: sampled minimum free space is {} bytes, configured minimum is {} bytes",
                sample.free_bytes,
                self.minimum_free_bytes
            );
        }
        if self.minimum_free_inodes > 0 {
            for filesystem in &sample.filesystems {
                if let Some(free) = filesystem.free_inodes {
                    if free < self.minimum_free_inodes {
                        bail!(
                            "insufficient inode headroom before unit {unit} on filesystem {}: sampled free inodes are {free}, configured minimum is {}",
                            filesystem.filesystem,
                            self.minimum_free_inodes
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

fn measure_path(root: &Path, entry_limit: u64) -> Result<(u64, Option<u64>, u64)> {
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0, Some(0), 0));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect relevant disk path {}", root.display()));
        }
    };
    if entry_limit == 0 {
        bail!("disk usage traversal has no entry budget remaining");
    }
    let mut logical_bytes = root_metadata.len();
    let mut allocated_bytes = allocated_size(root, &root_metadata);
    let mut entry_count = 1_u64;

    if !root_metadata.is_dir() {
        return Ok((logical_bytes, allocated_bytes, entry_count));
    }

    // Keep one iterator per directory level, rather than collecting every
    // child path. The stack therefore grows with depth, not directory width.
    let mut directories: Vec<(PathBuf, ReadDir)> = vec![(
        root.to_path_buf(),
        fs::read_dir(root).with_context(|| format!("read directory {}", root.display()))?,
    )];

    while let Some((_, entries)) = directories.last_mut() {
        let next = entries.next();
        let Some(entry) = next.transpose().with_context(|| {
            format!(
                "read directory entry in {}",
                directories
                    .last()
                    .expect("directory stack is nonempty")
                    .0
                    .display()
            )
        })?
        else {
            directories.pop();
            continue;
        };

        entry_count = entry_count.saturating_add(1);
        if entry_count > entry_limit {
            bail!(
                "disk usage traversal of {} exceeded its remaining {entry_limit} entry budget",
                root.display(),
            );
        }

        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("inspect relevant disk entry {}", path.display()))?;
        logical_bytes = logical_bytes.saturating_add(metadata.len());
        allocated_bytes = match (allocated_bytes, allocated_size(&path, &metadata)) {
            (Some(total), Some(value)) => Some(total.saturating_add(value)),
            _ => None,
        };

        if metadata.is_dir() {
            let depth = directories.len();
            if depth >= MAX_DIRECTORY_DEPTH {
                bail!(
                    "disk usage traversal of {} exceeded the {MAX_DIRECTORY_DEPTH} directory depth limit",
                    root.display()
                );
            }
            let entries = fs::read_dir(&path)
                .with_context(|| format!("read directory {}", path.display()))?;
            directories.push((path, entries));
        }
    }

    Ok((logical_bytes, allocated_bytes, entry_count))
}

fn existing_capacity_path(path: &Path) -> Result<PathBuf> {
    let mut candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("resolve current directory for disk sampling")?
            .join(path)
    };

    loop {
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                match fs::canonicalize(&candidate) {
                    Ok(target) => return Ok(target),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("resolve disk sampling path {}", candidate.display())
                        });
                    }
                }
            }
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect disk sampling path {}", candidate.display())
                });
            }
        }

        candidate = candidate.parent().map(Path::to_path_buf).with_context(|| {
            format!(
                "find existing ancestor for disk sampling path {}",
                path.display()
            )
        })?;
    }
}

#[cfg(unix)]
fn allocated_size(_path: &Path, metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.blocks().saturating_mul(512))
}

#[cfg(windows)]
fn allocated_size(path: &Path, _metadata: &fs::Metadata) -> Option<u64> {
    windows_platform::allocated_size(path)
}

#[cfg(not(any(unix, windows)))]
fn allocated_size(_path: &Path, _metadata: &fs::Metadata) -> Option<u64> {
    None
}

#[derive(Debug)]
struct FileSystemStats {
    identity: String,
    free_bytes: u64,
    free_inodes: Option<u64>,
    used_bytes: u64,
}

#[cfg(unix)]
fn filesystem_stats(path: &Path) -> Result<FileSystemStats> {
    use std::process::Command;

    let absolute_path = existing_capacity_path(path)?;
    let output = Command::new("df")
        .arg("-Pk")
        .arg(&absolute_path)
        .env("LC_ALL", "C")
        .output()
        .context("run df to sample filesystem capacity")?;
    if !output.status.success() {
        bail!(
            "df could not sample {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let line = output
        .stdout
        .split(|byte| *byte == b'\n')
        .rfind(|line| !line.iter().all(u8::is_ascii_whitespace))
        .context("df returned no filesystem capacity row")?;
    let columns: Vec<&str> = std::str::from_utf8(line)
        .context("df returned non-UTF-8 filesystem capacity data")?
        .split_whitespace()
        .collect();
    if columns.len() < 4 {
        bail!("df returned a malformed filesystem capacity row: {columns:?}");
    }
    let blocks_used = parse_df_number(columns[2], "used blocks")?;
    let blocks_free = parse_df_number(columns[3], "available blocks")?;
    let identity = columns[0].to_owned();

    let free_inodes = Command::new("df")
        .arg("-Pi")
        .arg(&absolute_path)
        .env("LC_ALL", "C")
        .output()
        .context("run df to sample inode capacity")?;
    if !free_inodes.status.success() {
        bail!(
            "df could not sample inode capacity for {}: {}",
            path.display(),
            String::from_utf8_lossy(&free_inodes.stderr).trim()
        );
    }
    let inode_table = String::from_utf8(free_inodes.stdout)
        .context("df returned non-UTF-8 inode capacity data")?;
    let mut inode_lines = inode_table.lines().filter(|line| !line.trim().is_empty());
    let inode_header: Vec<&str> = inode_lines
        .next()
        .context("df returned no inode capacity header")?
        .split_whitespace()
        .collect();
    let inode_row: Vec<&str> = inode_lines
        .next_back()
        .context("df returned no inode capacity row")?
        .split_whitespace()
        .collect();
    let free_inode_index = inode_header
        .iter()
        .position(|column| column.eq_ignore_ascii_case("ifree"))
        .context("df inode capacity header has no IFree column")?;
    let free_inode_value = inode_row
        .get(free_inode_index)
        .context("df inode capacity row does not include its IFree value")?;
    let free_inodes = if *free_inode_value == "-" {
        None
    } else {
        Some(
            free_inode_value
                .parse::<u64>()
                .with_context(|| format!("parse df free inode value {free_inode_value:?}"))?,
        )
    };

    Ok(FileSystemStats {
        identity,
        free_bytes: blocks_free.saturating_mul(1024),
        free_inodes,
        used_bytes: blocks_used.saturating_mul(1024),
    })
}

#[cfg(unix)]
fn parse_df_number(value: &str, name: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("parse df {name} value {value:?}"))
}

#[cfg(windows)]
fn filesystem_stats(path: &Path) -> Result<FileSystemStats> {
    windows_platform::filesystem_stats(path)
}

#[cfg(not(any(unix, windows)))]
fn filesystem_stats(_path: &Path) -> Result<FileSystemStats> {
    bail!("disk capacity sampling is unsupported on this platform")
}

#[cfg(windows)]
mod windows_platform {
    use super::FileSystemStats;
    use anyhow::{Context, Result};
    use std::{
        ffi::c_void,
        mem::{size_of, MaybeUninit},
        os::windows::ffi::OsStrExt,
        path::Path,
    };

    type Handle = *mut c_void;

    #[repr(C)]
    struct FileStandardInfo {
        allocation_size: i64,
        end_of_file: i64,
        number_of_links: u32,
        delete_pending: u8,
        directory: u8,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available_to_caller: *mut u64,
            total_number_of_bytes: *mut u64,
            total_number_of_free_bytes: *mut u64,
        ) -> i32;
        fn GetVolumePathNameW(
            file_name: *const u16,
            volume_path_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
        fn GetVolumeNameForVolumeMountPointW(
            volume_mount_point: *const u16,
            volume_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn GetFileInformationByHandleEx(
            file: Handle,
            information_class: i32,
            information: *mut c_void,
            buffer_size: u32,
        ) -> i32;
        fn CloseHandle(object: Handle) -> i32;
    }

    pub(super) fn filesystem_stats(path: &Path) -> Result<FileSystemStats> {
        let absolute_path = super::existing_capacity_path(path)?;
        let path_wide = wide_path(&absolute_path);
        let mut volume_path = vec![0_u16; 32_768];
        let volume_ok = unsafe {
            GetVolumePathNameW(
                path_wide.as_ptr(),
                volume_path.as_mut_ptr(),
                volume_path.len() as u32,
            )
        };
        if volume_ok == 0 {
            return Err(std::io::Error::last_os_error())
                .context("resolve disk volume containing relevant path");
        }
        // A volume may have several mounted-folder paths. The mount manager's
        // canonical GUID, not a particular mount path, identifies the volume.
        let mut volume_guid = [0_u16; 50];
        let identity_ok = unsafe {
            GetVolumeNameForVolumeMountPointW(
                volume_path.as_ptr(),
                volume_guid.as_mut_ptr(),
                volume_guid.len() as u32,
            )
        };
        if identity_ok == 0 {
            return Err(std::io::Error::last_os_error())
                .context("resolve canonical disk volume identity");
        }
        let guid_end = volume_guid
            .iter()
            .position(|value| *value == 0)
            .expect("successful volume GUID lookup is null-terminated");

        let mut free_available = 0_u64;
        let mut total_bytes = 0_u64;
        let mut total_free_bytes = 0_u64;
        let capacity_ok = unsafe {
            GetDiskFreeSpaceExW(
                volume_path.as_ptr(),
                &mut free_available,
                &mut total_bytes,
                &mut total_free_bytes,
            )
        };
        if capacity_ok == 0 {
            return Err(std::io::Error::last_os_error())
                .context("sample disk capacity for relevant volume");
        }

        Ok(FileSystemStats {
            identity: String::from_utf16_lossy(&volume_guid[..guid_end]).to_lowercase(),
            free_bytes: free_available,
            free_inodes: None,
            used_bytes: total_bytes.saturating_sub(total_free_bytes),
        })
    }

    pub(super) fn allocated_size(path: &Path) -> Option<u64> {
        let path_wide = wide_path(path);
        const FILE_READ_ATTRIBUTES: u32 = 0x0080;
        const FILE_SHARE_READ: u32 = 0x0001;
        const FILE_SHARE_WRITE: u32 = 0x0002;
        const FILE_SHARE_DELETE: u32 = 0x0004;
        const OPEN_EXISTING: u32 = 3;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_STANDARD_INFO_CLASS: i32 = 1;
        let handle = unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle as isize == -1 {
            return None;
        }

        let mut info = MaybeUninit::<FileStandardInfo>::uninit();
        let result = unsafe {
            GetFileInformationByHandleEx(
                handle,
                FILE_STANDARD_INFO_CLASS,
                info.as_mut_ptr().cast(),
                size_of::<FileStandardInfo>() as u32,
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if result == 0 {
            return None;
        }
        let allocation_size = unsafe { info.assume_init() }.allocation_size;
        u64::try_from(allocation_size).ok()
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};
    use tempfile::tempdir;

    #[test]
    fn telemetry_appends_samples_and_tracks_sampled_usage() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        let work = root.join("work");
        let reports = root.join("reports");
        let evidence = root.join("evidence");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&reports).unwrap();
        fs::create_dir_all(&evidence).unwrap();
        let fixture = work.join("case.txt");
        fs::write(&fixture, "some benchmark data").unwrap();

        let mut monitor = DiskMonitor::new(&work, &reports, &evidence, 0, 0).unwrap();
        let first = monitor
            .record("prepare", "case-1", &[("fixture", fixture.as_path())])
            .unwrap();
        let second = monitor
            .record("query", "case-1", &[("fixture", fixture.as_path())])
            .unwrap();
        assert!(first.free_bytes > 0);
        assert!(first.relevant_paths[0].logical_bytes >= "some benchmark data".len() as u64);
        assert!(second.reclaimed_bytes_since_previous_sample.is_some());
        let first_filesystems: Vec<&str> = first
            .filesystems
            .iter()
            .map(|filesystem| filesystem.filesystem.as_str())
            .collect();
        let second_filesystems: Vec<&str> = second
            .filesystems
            .iter()
            .map(|filesystem| filesystem.filesystem.as_str())
            .collect();
        assert_eq!(first_filesystems, second_filesystems);
        for root in ["work", "report", "evidence"] {
            assert!(first.filesystems.iter().any(|filesystem| {
                filesystem
                    .configured_roots
                    .iter()
                    .any(|configured_root| configured_root == root)
            }));
        }

        let telemetry = fs::read_to_string(evidence.join("disk.jsonl")).unwrap();
        let records: Vec<serde_json::Value> = telemetry
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["phase"], "prepare");
        assert!(records[0]["sampledPeakUsedBytes"].as_u64().is_some());
        assert!(records[0]["filesystems"].is_array());
        assert!(records[1]["reclaimedBytesSincePreviousSample"]
            .as_u64()
            .is_some());
    }

    #[test]
    fn headroom_refusal_is_recorded_before_returning_error() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        let mut monitor = DiskMonitor::new(root, root, root, u64::MAX, 0).unwrap();
        assert!(monitor.ensure_headroom("large-case").is_err());

        let telemetry = fs::read_to_string(root.join("disk.jsonl")).unwrap();
        let record: serde_json::Value =
            serde_json::from_str(telemetry.lines().next().unwrap()).unwrap();
        assert_eq!(record["phase"], "headroom_check");
        assert_eq!(record["unit"], "large-case");
    }

    #[test]
    fn missing_relevant_path_has_zero_usage_and_uses_existing_ancestor_capacity() {
        let temporary = tempdir().unwrap();
        let missing = temporary.path().join("removed-after-cleanup");
        let evidence = temporary.path().join("evidence");
        fs::create_dir_all(&evidence).unwrap();
        let mut monitor =
            DiskMonitor::new(temporary.path(), temporary.path(), &evidence, 0, 0).unwrap();

        let sample = monitor
            .record("reclaimed", "case-2", &[("removed", missing.as_path())])
            .unwrap();
        assert_eq!(sample.relevant_paths[0].logical_bytes, 0);
        assert_eq!(sample.relevant_paths[0].allocated_bytes, Some(0));
        assert_eq!(sample.relevant_paths[0].entry_count, 0);
        assert!(sample.free_bytes > 0);
    }

    #[cfg(unix)]
    #[test]
    fn directory_measurement_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let temporary = tempdir().unwrap();
        let root = temporary.path().join("root");
        fs::create_dir(&root).unwrap();
        let target = temporary.path().join("large-target.bin");
        let mut file = File::create(&target).unwrap();
        file.write_all(&vec![0_u8; 2 * 1024 * 1024]).unwrap();
        symlink(&target, root.join("link-to-target")).unwrap();

        let (logical_bytes, _, entry_count) = measure_path(&root, MAX_DIRECTORY_ENTRIES).unwrap();
        assert!(logical_bytes < 2 * 1024 * 1024);
        assert_eq!(entry_count, 2);
    }
}
