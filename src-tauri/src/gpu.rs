//! 系统 GPU 占用：进程级 GPU 使用率在三个平台上都没有公共接口，
//! 因此这里统一取整机口径——"这台机器的 GPU 有多忙"，而不是"本应用占了多少 GPU"。
//! 界面上的这一项因此标注为系统整体占用，与进程级的 CPU / 内存区分开。

/// 当前系统 GPU 占用百分比（0–100）；读不到时返回 None，界面显示占位符。
/// 底层会拉起系统命令或读 sysfs，调用方应放在阻塞线程里、并拉开轮询间隔。
pub fn system_usage() -> Option<f32> {
    platform::usage()
}

#[cfg(target_os = "macos")]
mod platform {
    /// `ioreg` 列出 IOAccelerator 实例，其 PerformanceStatistics 里带
    /// "Device Utilization %"；多显卡（含外接 eGPU）取最大值。
    pub fn usage() -> Option<f32> {
        let out = std::process::Command::new("ioreg")
            .args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"])
            .output()
            .ok()?;
        parse(&String::from_utf8_lossy(&out.stdout))
    }

    /// 从 ioreg 文本里取所有 "Device Utilization %" 的值
    fn parse(text: &str) -> Option<f32> {
        const KEY: &str = "\"Device Utilization %\"=";
        let mut best: Option<f32> = None;
        for part in text.split(KEY).skip(1) {
            let num: String = part
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let Ok(v) = num.parse::<f32>() else { continue };
            best = Some(best.map_or(v, |b| b.max(v)));
        }
        best.map(|v| v.clamp(0.0, 100.0))
    }
}

/// Windows：直接问系统的性能计数器（PDH），与任务管理器同一条数据源。
///
/// 早先走的是 `Get-CimInstance` 拉 WMI 的 GPU 计数器再取单条记录的最大值，
/// 三处都不对：
///   1. 单个计数器实例是"某个进程的某个引擎"，一个浏览器拆成十几个实例、
///      各自个位数，取最大值自然远低于任务管理器看到的整机占用；
///   2. 每次取数都要起一个 PowerShell，冷启动几百毫秒到几秒。界面允许把
///      GPU 调到 0.1 秒一刷，于是请求堆在一起，数字乱跳；
///   3. 数值以文本回传，小数点跟随系统语言，非英文系统直接解析失败。
///
/// 现在改为进程内查询 PDH，按引擎类型（3D / Copy / VideoDecode…）把各进程
/// 的占用加起来、取最忙的一类——这正是任务管理器的口径。查询句柄常驻，
/// 每次取数只做一次采样，没有外部进程、没有文本解析。
#[cfg(target_os = "windows")]
mod platform {
    use std::sync::{Mutex, OnceLock};

    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
        PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_INSUFFICIENT_BUFFER, PDH_MORE_DATA,
    };

    /// 计数器路径写成英文：PdhAddEnglishCounterW 会把它映射到当前系统语言
    /// 下的计数器名，中文 / 日文 / 德文 Windows 不必各写一套。
    const COUNTER_PATH: &str = "\\GPU Engine(*)\\Utilization Percentage";

    /// 一次查询的句柄对。速率型计数器要两次采样才有值，句柄因此必须常驻：
    /// 每次新建查询都只能拿到第一次采样，永远读不出数字。
    struct Query {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }

    // SAFETY: 句柄只在持锁期间使用，且归本进程所有、与线程无关；
    // 编译器拒绝 Send 只是因为字段是裸指针。
    unsafe impl Send for Query {}

    /// None = 这台机器读不到 GPU 计数器（无 WDDM 显卡、虚拟机等），
    /// 下次取数再试一次：装好驱动后不必重启应用就能自己恢复。
    static QUERY: OnceLock<Mutex<Option<Query>>> = OnceLock::new();

    pub fn usage() -> Option<f32> {
        let slot = QUERY.get_or_init(|| Mutex::new(open()));
        // 取数串行化：两次轮询撞在一起时，后一次等前一次读完，
        // 不会并发操作同一对句柄
        let Ok(mut guard) = slot.lock() else {
            return None;
        };
        if guard.is_none() {
            *guard = open();
        }
        read(guard.as_ref()?)
    }

    fn open() -> Option<Query> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: 空数据源 = 本机实时数据；句柄由 PDH 填写
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
            return None;
        }

        let path: Vec<u16> = COUNTER_PATH.encode_utf16().chain(std::iter::once(0)).collect();
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: path 在调用期间存活，且以 NUL 结尾
        let status = unsafe {
            PdhAddEnglishCounterW(query, PCWSTR::from_raw(path.as_ptr()), 0, &mut counter)
        };
        if status != 0 {
            // SAFETY: query 刚由 PdhOpenQueryW 得到且未被使用
            unsafe { PdhCloseQuery(query) };
            return None;
        }

        // 先采一次、稍等再采一次：第一次取数就能给出真实读数，
        // 而不是让卡片空挂一个刷新周期
        unsafe {
            PdhCollectQueryData(query);
            std::thread::sleep(std::time::Duration::from_millis(120));
            PdhCollectQueryData(query);
        }
        Some(Query { query, counter })
    }

    fn read(q: &Query) -> Option<f32> {
        // SAFETY: 句柄来自 open() 且在本函数返回前一直有效；缓冲区按 PDH
        // 报的长度分配，读取范围不超出 PDH 填写的条目数
        unsafe {
            // 本次采样：读数覆盖"距上一次采样"的这段时间，轮询越密越接近瞬时值
            if PdhCollectQueryData(q.query) != 0 {
                return None;
            }

            let mut size: u32 = 0;
            let mut count: u32 = 0;
            // 第一次只问"要多大"，正常返回 PDH_MORE_DATA（个别驱动给
            // PDH_INSUFFICIENT_BUFFER，同样带着所需长度）
            let mut status = PdhGetFormattedCounterArrayW(
                q.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                None,
            );
            if (status != PDH_MORE_DATA && status != PDH_INSUFFICIENT_BUFFER)
                || size == 0
                || count == 0
            {
                return None;
            }

            // 条目里既有指针又有 8 字节联合体，缓冲区必须 8 字节对齐，
            // Vec<u8> 只有 1 字节对齐，不能直接用
            let mut buf = vec![0u64; size as usize / 8 + 1];
            status = PdhGetFormattedCounterArrayW(
                q.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
            );
            if status != 0 || count == 0 {
                return None;
            }

            let items = std::slice::from_raw_parts(
                buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
                count as usize,
            );

            // 与任务管理器同一口径：按引擎类型把各进程的占用加起来，
            // 再取最忙的那一类。单个实例的最大值只会给出"最忙的那个进程
            // 在那个引擎上用了多少"，整机数字会低到不像话。
            let mut sums: Vec<(String, f64)> = Vec::new();
            for item in items {
                let cs = item.FmtValue.CStatus;
                // 第一次采样、计数器刚重建等情况下 PDH 会给出无效状态，
                // 这些条目跳过，不当成 0
                if cs != PDH_CSTATUS_VALID_DATA && cs != PDH_CSTATUS_NEW_DATA {
                    continue;
                }
                let v = item.FmtValue.Anonymous.doubleValue;
                if !v.is_finite() || v < 0.0 {
                    continue;
                }
                let name = wide_to_string(item.szName.0 as *const u16);
                // 实例名形如 pid_1234_luid_0x…_phys_0_eng_0_engtype_3D
                let engine = name
                    .rsplit("engtype_")
                    .next()
                    .unwrap_or("other")
                    .to_string();
                match sums.iter_mut().find(|(k, _)| *k == engine) {
                    Some((_, total)) => *total += v,
                    None => sums.push((engine, v)),
                }
            }

            if sums.is_empty() {
                // 全是无效条目 = 还没有可信读数，交给界面显示占位符
                return None;
            }
            let busiest = sums.iter().map(|(_, v)| *v).fold(0.0, f64::max);
            Some((busiest as f32).clamp(0.0, 100.0))
        }
    }

    /// 计数器实例名以 NUL 结尾、长度不单独给出，按 NUL 读并设上限防越界
    fn wide_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: 指针来自 PDH 填写的条目，指向以 NUL 结尾的实例名；
        // 读到 NUL 或 512 个单元即停，不越界
        unsafe {
            let mut len = 0usize;
            while len < 512 && *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    /// DRM 内核驱动导出的忙时占比（AMD / 部分 Intel 与 Nouveau 驱动支持）
    pub fn usage() -> Option<f32> {
        for i in 0..8 {
            let path = format!("/sys/class/drm/card{i}/device/gpu_busy_percent");
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(v) = text.trim().parse::<f32>() {
                    return Some(v.clamp(0.0, 100.0));
                }
            }
        }
        None
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
mod platform {
    pub fn usage() -> Option<f32> {
        None
    }
}
