//! Implementation of binary download and replacement for online update.
//!
//! this solution no longer operates docker daemon:
//! 1. from GitHub Releases Downloads the one matching the current platform. `kiro-rs-<ver>-<plat>.tar.gz`/`.zip`;
//! 2. verify `SHA256SUMS.txt` checksum;
//! 3. Unpacks the new binary and atomically replaces the current one. exe, the old version writes to `<exe>.backup`;
//! 4. callsidereceived `need_restart=true` then triggers the process exit, by docker of
//!    `restart: unless-stopped` takes over the restart, and the new version then takes effect.
//!
//! The benefit is that the update process does not rely on the container managing itself, and if the network drops/when validation fails the old binary
//! is still running, to avoid the previous docker compose pull pathupof"oldstopnewhang"incident.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::admin::error::AdminServiceError;

/// The maximum bytes of a single download body, avoiding GitHub filling the disk on an abnormal return.
/// kiro-rs musl binarymeasured < 50 MB, keep 200 MB The limit is enough to cover future growth.
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// GitHub Releases repository owner/repo.
const GITHUB_REPO: &str = "ZyphrZero/kiro.rs";

/// release The binary file name inside the package (after unpacking).Linux/macOS is `kiro-rs`,
/// Windows is `kiro-rs.exe`.
fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "kiro-rs.exe"
    } else {
        "kiro-rs"
    }
}

/// Absolute path of the current process binary (symlinks resolved); an update replaces it.
pub fn current_executable() -> Result<PathBuf, AdminServiceError> {
    let exe = std::env::current_exe().map_err(|e| {
        AdminServiceError::InternalError(format!("Cannot obtain the current executable path.: {}", e))
    })?;
    let resolved = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(resolved)
}

/// backup file path:`<exe>.backup`. The rollback interface swaps it straight back.
pub fn backup_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_os_string();
    s.push(".backup");
    PathBuf::from(s)
}

/// the one corresponding to the current platform release archive suffix fragment, for example `Linux-musl-x64.tar.gz`.
///
/// and `.github/workflows/release.yaml` consistent with the matrix in:
/// - Linux x86_64:musl static binary (container/host machine general)
/// - Linux aarch64:musl staticbinary
/// - macOS x86_64 / aarch64:tar.gz
/// - Windows x86_64:zip
fn platform_suffix() -> Result<&'static str, AdminServiceError> {
    let suffix = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "Linux-musl-x64.tar.gz",
        ("linux", "aarch64") => "Linux-musl-arm64.tar.gz",
        ("macos", "x86_64") => "macOS-x64.tar.gz",
        ("macos", "aarch64") => "macOS-arm64.tar.gz",
        ("windows", "x86_64") => "Windows-x64.zip",
        (os, arch) => {
            return Err(AdminServiceError::InternalError(format!(
                "unsupported platform {}/{}: online update only supports release.yaml the published target in the matrix",
                os, arch
            )));
        }
    };
    Ok(suffix)
}

/// expected release archive file name, for example `kiro-rs-0.3.0-Linux-musl-x64.tar.gz`.
fn archive_filename(version: &str) -> Result<String, AdminServiceError> {
    let v = version.trim().trim_start_matches('v');
    if v.is_empty() {
        return Err(AdminServiceError::InternalError(
            "The version number is empty; cannot locate it. release asset".to_string(),
        ));
    }
    Ok(format!("kiro-rs-{}-{}", v, platform_suffix()?))
}

/// download and validate a certain release the version binary archive, and takes the internal `kiro-rs` extracted to `dest`.
///
/// `proxy` as `Some` when all HTTP The request goes through this proxy (consistent with other outbound paths in the project).
/// download and validate a certain release the version binary archive, and takes the internal `kiro-rs` extracted to `dest`.
///
/// `proxy` as `Some` when all HTTP The request goes through this proxy (consistent with other outbound paths in the project).
/// `github_token` When not empty, attaches it to all requests. `Authorization: Bearer <token>`,
/// take GitHub API limitstreamfromanonymous 60/h improvetoauth 5000/h.
pub async fn download_release_binary(
    version: &str,
    proxy: Option<&str>,
    github_token: Option<&str>,
    dest: &Path,
) -> Result<(), AdminServiceError> {
    let archive = archive_filename(version)?;
    let base = format!(
        "https://github.com/{}/releases/download/v{}",
        GITHUB_REPO,
        version.trim().trim_start_matches('v')
    );
    let archive_url = format!("{}/{}", base, archive);
    let checksums_url = format!("{}/SHA256SUMS.txt", base);

    let client = build_http_client(proxy)?;
    let token = github_token.and_then(|t| {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    // Downloads to a temporary directory to ensure a failure does not pollute exe placeindirectory
    let tmp_dir = std::env::temp_dir().join(format!(
        "kiro-rs-update-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::create_dir_all(&tmp_dir).map_err(|e| {
        AdminServiceError::InternalError(format!("Creating the update temporary directory failed.: {}", e))
    })?;
    let archive_path = tmp_dir.join(&archive);

    download_to_file(&client, &archive_url, token.as_deref(), &archive_path).await?;
    verify_checksum(
        &client,
        &checksums_url,
        token.as_deref(),
        &archive,
        &archive_path,
    )
    .await?;

    let extract_dir = tmp_dir.join("extract");
    fs::create_dir_all(&extract_dir).map_err(|e| {
        AdminServiceError::InternalError(format!("failed to create the extraction directory: {}", e))
    })?;
    extract_archive(&archive_path, &extract_dir)?;

    let extracted = locate_binary(&extract_dir)?;
    // Copies to the target location specified by the caller (usually exe a temporary file in its directory, convenient for atomic replacement).
    fs::copy(&extracted, dest).map_err(|e| {
        AdminServiceError::InternalError(format!("failed to copy the new binary: {}", e))
    })?;
    set_executable(dest)?;

    // Cleans the temporary directory (failure is only logged and does not affect the main flow).
    let _ = fs::remove_dir_all(&tmp_dir);
    Ok(())
}

pub(super) fn build_http_client(
    proxy: Option<&str>,
) -> Result<reqwest::Client, AdminServiceError> {
    let mut builder = reqwest::Client::builder()
        .user_agent("kiro-rs-updater")
        .timeout(std::time::Duration::from_secs(180));
    if let Some(url) = proxy.and_then(|u| {
        let s = u.trim();
        if s.is_empty() { None } else { Some(s) }
    }) {
        let proxy = reqwest::Proxy::all(url).map_err(|e| {
            AdminServiceError::InternalError(format!("invalid proxy configuration: {}", e))
        })?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|e| AdminServiceError::InternalError(format!("construct HTTP clientfailed: {}", e)))
}

async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    dest: &Path,
) -> Result<(), AdminServiceError> {
    let mut req = client.get(url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await.map_err(|e| {
        AdminServiceError::InternalError(format!("download {} failed: {}", url, e))
    })?;
    if !resp.status().is_success() {
        return Err(AdminServiceError::InternalError(format!(
            "download {} return {}",
            url,
            resp.status()
        )));
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_DOWNLOAD_BYTES {
            return Err(AdminServiceError::InternalError(format!(
                "downloadsize {} bytes exceed the upper limit {} bytes",
                len, MAX_DOWNLOAD_BYTES
            )));
        }
    }

    let bytes = resp.bytes().await.map_err(|e| {
        AdminServiceError::InternalError(format!("failed to read the downloaded content: {}", e))
    })?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(AdminServiceError::InternalError(format!(
            "actual download size {} bytes exceed the upper limit",
            bytes.len()
        )));
    }
    fs::write(dest, &bytes).map_err(|e| {
        AdminServiceError::InternalError(format!("write the download file {} failed: {}", dest.display(), e))
    })?;
    Ok(())
}

async fn verify_checksum(
    client: &reqwest::Client,
    checksums_url: &str,
    token: Option<&str>,
    archive_name: &str,
    archive_path: &Path,
) -> Result<(), AdminServiceError> {
    let mut req = client.get(checksums_url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let resp = req.send().await.map_err(|e| {
        AdminServiceError::InternalError(format!("download SHA256SUMS.txt failed: {}", e))
    })?;
    if !resp.status().is_success() {
        return Err(AdminServiceError::InternalError(format!(
            "download SHA256SUMS.txt return {}",
            resp.status()
        )));
    }
    let body = resp.text().await.map_err(|e| {
        AdminServiceError::InternalError(format!("read SHA256SUMS.txt failed: {}", e))
    })?;

    let expected = body
        .lines()
        .filter_map(|line| {
            let mut iter = line.split_whitespace();
            let hash = iter.next()?;
            let name = iter.next()?.trim_start_matches('*');
            if name == archive_name {
                Some(hash.to_ascii_lowercase())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            AdminServiceError::InternalError(format!(
                "SHA256SUMS.txt innot yetfind {} ofvalidateitem",
                archive_name
            ))
        })?;

    let actual = sha256_file(archive_path)?;
    if actual != expected {
        return Err(AdminServiceError::InternalError(format!(
            "checksum mismatch: expected {}, actual {}",
            expected, actual
        )));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, AdminServiceError> {
    let mut file = fs::File::open(path).map_err(|e| {
        AdminServiceError::InternalError(format!("open {} failed: {}", path.display(), e))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| {
            AdminServiceError::InternalError(format!("read {} failed: {}", path.display(), e))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<(), AdminServiceError> {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else {
        Err(AdminServiceError::InternalError(format!(
            "unsupported archive format: {}",
            name
        )))
    }
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), AdminServiceError> {
    let bytes = fs::read(archive).map_err(|e| {
        AdminServiceError::InternalError(format!("readarchive {} failed: {}", archive.display(), e))
    })?;
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest).map_err(|e| {
        AdminServiceError::InternalError(format!("unzip tar.gz failed: {}", e))
    })
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), AdminServiceError> {
    let file = fs::File::open(archive).map_err(|e| {
        AdminServiceError::InternalError(format!("open {} failed: {}", archive.display(), e))
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| {
        AdminServiceError::InternalError(format!("parse zip failed: {}", e))
    })?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| {
            AdminServiceError::InternalError(format!("read zip entryentryfailed: {}", e))
        })?;
        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let target = dest.join(entry_path);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| {
                AdminServiceError::InternalError(format!("failed to create directory: {}", e))
            })?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AdminServiceError::InternalError(format!("failed to create directory: {}", e))
            })?;
        }
        let mut out = fs::File::create(&target).map_err(|e| {
            AdminServiceError::InternalError(format!("createfile {} failed: {}", target.display(), e))
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|e| {
            AdminServiceError::InternalError(format!("write {} failed: {}", target.display(), e))
        })?;
    }
    Ok(())
}

fn locate_binary(root: &Path) -> Result<PathBuf, AdminServiceError> {
    let target = binary_name();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| {
            AdminServiceError::InternalError(format!("readdirectory {} failed: {}", dir.display(), e))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                AdminServiceError::InternalError(format!("failed to enumerate directory entries: {}", e))
            })?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == target)
                .unwrap_or(false)
            {
                return Ok(path);
            }
        }
    }
    Err(AdminServiceError::InternalError(format!(
        "not found in the archive {} binary",
        target
    )))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), AdminServiceError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| AdminServiceError::InternalError(format!("failed to read permission: {}", e)))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| {
        AdminServiceError::InternalError(format!("failed to set executable permission: {}", e))
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), AdminServiceError> {
    Ok(())
}

/// take `staged`installs (the already downloaded new binary) as the current one. exe,andtake current exe backup to `<exe>.backup`.
///
/// Windows the one running under exe Cannot be overwritten directly but can be renamed; here it always goes through
/// "rename current → backup; rename staged → current" the two step flow,
/// Ensures any step failing can be rolled back.
pub fn install_binary(exe: &Path, staged: &Path) -> Result<(), AdminServiceError> {
    let backup = backup_path(exe);
    // old backup kept but unused, clear it first.
    let _ = fs::remove_file(&backup);
    fs::rename(exe, &backup).map_err(|e| {
        AdminServiceError::InternalError(format!(
            "back up the current executable file {} failed: {}",
            exe.display(),
            e
        ))
    })?;
    if let Err(e) = fs::rename(staged, exe) {
        // staged → exe failedtake when backup Restores, ensuring the old version is still usable.
        let _ = fs::rename(&backup, exe);
        return Err(AdminServiceError::InternalError(format!(
            "Installing the new executable failed.: {}",
            e
        )));
    }
    Ok(())
}

/// use `<exe>.backup` overridecurrent exe, implement"roll back to the previous version".
pub fn restore_backup(exe: &Path) -> Result<(), AdminServiceError> {
    let backup = backup_path(exe);
    if !backup.exists() {
        return Err(AdminServiceError::InternalError(
            "The local backup binary was not found (<exe>.backup does not exist), cannot roll back offline.".to_string(),
        ));
    }
    // take current exe stash to .rollback-current, then take backup swapintonewof exe.
    let mut rollback_tmp = exe.as_os_str().to_os_string();
    rollback_tmp.push(".rollback-current");
    let rollback_tmp = PathBuf::from(rollback_tmp);
    let _ = fs::remove_file(&rollback_tmp);
    fs::rename(exe, &rollback_tmp).map_err(|e| {
        AdminServiceError::InternalError(format!("stage temporarilycurrent exe failed: {}", e))
    })?;
    if let Err(e) = fs::rename(&backup, exe) {
        let _ = fs::rename(&rollback_tmp, exe);
        return Err(AdminServiceError::InternalError(format!(
            "fallbackfailed: {}",
            e
        )));
    }
    let _ = fs::remove_file(&rollback_tmp);
    Ok(())
}

/// Starts an asynchronous task, and `delay` afterwards let the process exit (exit code 0).
/// docker of `restart: unless-stopped` takes over the restart, and the new binary then takes effect.
pub fn schedule_self_exit(delay: std::time::Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        // give stdout a flush chance, avoiding losing the last line of log.
        let _ = std::io::stdout().flush();
        std::process::exit(0);
    });
}
