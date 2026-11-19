//! 存储设备枚举与识别。
//!
//! 识别策略：优先比对配对时写入设备的标记文件（卷标变更、盘符变化后依然有效），
//! 其次回退到卷标匹配。
//!
//! 每块物理磁盘上的分区归成一组：Windows 用
//! IOCTL_STORAGE_GET_DEVICE_NUMBER 拿磁盘序号，macOS 用 statfs 的设备名
//! （/dev/disk3s1s1 → disk3）。系统盘也列出来——它同样可能是同步的目标，
//! 例如把一个工作目录备份到另一块内置盘的某个分区。

use std::path::{Path, PathBuf};

use sysinfo::Disks;

use super::model::{SyncPair, VolumeInfo, MARKER_FILE};

/// 枚举当前接入的卷（本地分区与可移动存储）
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for disk in disks.list() {
        let mount = disk.mount_point();
        if !is_candidate_mount(mount) {
            continue;
        }
        let mount_str = normalize_mount(mount);
        let label = volume_label(disk, &mount_str);
        let (disk_no, kind, system) = disk_facts(&mount_str);
        out.push(VolumeInfo {
            mount: mount_str,
            label,
            fs: disk.file_system().to_string_lossy().to_string(),
            total: disk.total_space(),
            available: disk.available_space(),
            matched_pair_ids: Vec::new(),
            disk: disk_no,
            kind: kind.to_string(),
            system,
        });
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

#[cfg(target_os = "macos")]
fn is_candidate_mount(mount: &Path) -> bool {
    let s = mount.to_string_lossy();
    // 根卷即系统盘；/Volumes 下是用户可见的其他卷。
    // /System/Volumes 里的 Data / Preboot / VM 是 APFS 的内部构造，
    // 列出来只会让人以为盘变多了
    s == "/" || (s.starts_with("/Volumes/") && s.len() > "/Volumes/".len())
}

#[cfg(target_os = "windows")]
fn is_candidate_mount(mount: &Path) -> bool {
    let s = mount.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes.len() > 3 || bytes[1] != b':' {
        return false;
    }
    // 只留固定盘与可移动盘：光驱、网络盘、RAM 盘没有可写的稳定存储
    matches!(drive_type(&s), DRIVE_REMOVABLE | DRIVE_FIXED)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn is_candidate_mount(_mount: &Path) -> bool {
    false
}

/// 卷 → (物理磁盘序号, 卷类型, 是否系统盘)
#[cfg(target_os = "macos")]
fn disk_facts(mount: &str) -> (String, &'static str, bool) {
    let system = mount == "/";
    // 与根卷共用同一块盘的卷算内置盘；其余（/Volumes 下另挂的）算可移动
    match macos_whole_disk(mount) {
        Some(no) => {
            let builtin = root_disk() == Some(no.as_str());
            let kind = if builtin { "fixed" } else { "removable" };
            (no, kind, system)
        }
        None => (mount.to_string(), "unknown", system),
    }
}

/// 根卷所在的整盘序号，只算一次
#[cfg(target_os = "macos")]
fn root_disk() -> Option<&'static str> {
    static ROOT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| macos_whole_disk("/")).as_deref()
}

/// 挂载点所属整盘序号：/dev/disk3s1s1 → "3"
#[cfg(target_os = "macos")]
fn macos_whole_disk(mount: &str) -> Option<String> {
    let c = std::ffi::CString::new(mount).ok()?;
    let mut buf = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(c.as_ptr(), buf.as_mut_ptr()) } != 0 {
        return None;
    }
    let buf = unsafe { buf.assume_init() };
    let dev = unsafe { std::ffi::CStr::from_ptr(buf.f_mntfromname.as_ptr()) };
    let dev = dev.to_string_lossy();
    let name = dev.strip_prefix("/dev/")?.strip_prefix("disk")?;
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

#[cfg(target_os = "windows")]
const DRIVE_REMOVABLE: u32 = 2;
#[cfg(target_os = "windows")]
const DRIVE_FIXED: u32 = 3;
#[cfg(target_os = "windows")]
const DRIVE_NETWORK: u32 = 4;
#[cfg(target_os = "windows")]
const DRIVE_CDROM: u32 = 5;

#[cfg(target_os = "windows")]
fn disk_facts(mount: &str) -> (String, &'static str, bool) {
    let letter = mount.chars().next().unwrap_or('C');
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let system = format!("{letter}:").eq_ignore_ascii_case(&sys);
    let kind = match drive_type(mount) {
        DRIVE_REMOVABLE => "removable",
        DRIVE_FIXED => "fixed",
        DRIVE_NETWORK => "network",
        DRIVE_CDROM => "optical",
        _ => "unknown",
    };
    // 拿不到磁盘号（某些虚拟盘 / 权限不足）时退回盘符，仍然能分组显示
    let no = disk_number(letter).unwrap_or_else(|| letter.to_string());
    (no, kind, system)
}

#[cfg(target_os = "windows")]
fn drive_type(root: &str) -> u32 {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;

    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) }
}

/// 盘符 → 物理磁盘序号（设备管理器里那个"磁盘 0 / 磁盘 1"）
#[cfg(target_os = "windows")]
fn disk_number(letter: char) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    /// IOCTL_STORAGE_GET_DEVICE_NUMBER：CTL_CODE(0x2D, 0x0420, METHOD_BUFFERED, FILE_ANY_ACCESS)
    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D_1080;

    #[repr(C)]
    #[derive(Default)]
    struct StorageDeviceNumber {
        device_type: u32,
        device_number: u32,
        partition_number: u32,
    }

    // "\\.\C:" 打开卷句柄；只查设备号，不要读写权限
    let path: Vec<u16> = format!("\\\\.\\{letter}:")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()?;

    let mut info = StorageDeviceNumber::default();
    let mut returned = 0u32;
    let res = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut info as *mut _ as *mut _),
            std::mem::size_of::<StorageDeviceNumber>() as u32,
            Some(&mut returned),
            None,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    res.ok()?;
    Some(info.device_number.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn disk_facts(mount: &str) -> (String, &'static str, bool) {
    (mount.to_string(), "unknown", false)
}

fn normalize_mount(mount: &Path) -> String {
    let s = mount.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        // 根卷的挂载点就是 "/"。去尾分隔符会把它变成空串，而空路径不是绝对
        // 路径：与相对路径拼出来的设备侧目录会落到进程的当前工作目录，
        // 同步就成了往编译器/启动目录里散文件
        let trimmed = s.trim_end_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            trimmed.to_string()
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows 盘符根（C:\）必须保留尾部反斜杠，去掉会变成驱动器相对路径
        s
    }
}

fn volume_label(disk: &sysinfo::Disk, mount: &str) -> String {
    let name = disk.name().to_string_lossy().trim().to_string();
    if !name.is_empty() {
        name
    } else {
        Path::new(mount)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| mount.to_string())
    }
}

/// 清理相对路径：统一 "/" 分隔，去掉空段与 "."（"..",由校验层拒绝）
pub fn normalize_rel(input: &str) -> String {
    input
        .replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// 卷内配对目录路径。rel 为空表示设备根目录。
pub fn remote_root(mount: &str, rel: &str) -> PathBuf {
    let mut p = PathBuf::from(mount);
    for part in rel.split('/') {
        if !part.is_empty() {
            p.push(part);
        }
    }
    p
}

/// 在已接入的卷中解析任务的设备侧目录：先按标记文件，再按卷标
pub fn resolve_remote_root(volumes: &[VolumeInfo], pair: &SyncPair) -> Option<PathBuf> {
    let rel = normalize_rel(&pair.remote_path);
    if let Some(expected) = pair.device_marker.as_deref() {
        if !expected.is_empty() {
            for v in volumes {
                let root = remote_root(&v.mount, &rel);
                if read_marker(&root).as_deref() == Some(expected) {
                    return Some(root);
                }
            }
        }
    }
    for v in volumes {
        if v.label == pair.device_label {
            return Some(remote_root(&v.mount, &rel));
        }
    }
    None
}

/// 裸卷列表 + 任务补全一步到位。前端命令返回与后台推送共用，
/// 只取裸卷列表会让前端拿到 `matchedPairIds: []`，刚插入的设备
/// 在界面上仍显示"设备未连接"。
pub fn snapshot_with_pairs(pairs: &[SyncPair]) -> Vec<VolumeInfo> {
    let mut vols = list_volumes();
    attach_pairs(&mut vols, pairs);
    vols
}

/// 为卷列表补上匹配到的同步任务 id。
pub fn attach_pairs(volumes: &mut [VolumeInfo], pairs: &[SyncPair]) {
    for v in volumes.iter_mut() {
        // 先复制一份只读快照，避免同时持有可变与不可变借用
        let probe = v.clone();
        let ids: Vec<String> = pairs
            .iter()
            .filter(|p| p.enabled && resolve_remote_root(std::slice::from_ref(&probe), p).is_some())
            .map(|p| p.id.clone())
            .collect();
        v.matched_pair_ids = ids;
    }
}

pub fn read_marker(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join(MARKER_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 确保配对目录存在并写入设备标记（已存在则不覆盖）
pub fn ensure_marker(root: &Path, pair: &SyncPair) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    if read_marker(root).is_some() {
        return Ok(());
    }
    let id = pair
        .device_marker
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    std::fs::write(root.join(MARKER_FILE), id)
}
