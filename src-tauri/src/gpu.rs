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

#[cfg(target_os = "windows")]
mod platform {
    use std::os::windows::process::CommandExt;

    /// 不弹控制台窗口（便携 exe 直接运行时尤其明显）
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// GPU 引擎性能计数器：每个引擎一条记录，取占用最高的那条（与任务管理器口径一致）。
    /// 计数器类别与属性名是英文标识、不随系统语言变化，因此不必本地化。
    pub fn usage() -> Option<f32> {
        let script = "try { $v = (Get-CimInstance -ClassName \
             Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop \
             | Measure-Object -Property UtilizationPercentage -Maximum).Maximum; \
             if ($null -eq $v) { '' } else { $v } } catch { '' }";
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v.clamp(0.0, 100.0))
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
