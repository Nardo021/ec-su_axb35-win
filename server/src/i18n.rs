pub const APP_NAME: &str = "ec-su_axb35-win";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        match code {
            "zh" => Language::Zh,
            _ => Language::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::Zh => "zh",
            Language::En => "en",
        }
    }
}

pub fn t(language: Language, key: &str) -> &str {
    match (language, key) {
        (Language::Zh, "app_title") => APP_NAME,
        (Language::En, "app_title") => APP_NAME,
        (Language::Zh, "ec_firmware") => "EC 固件版本：",
        (Language::En, "ec_firmware") => "EC firmware version:",
        (Language::Zh, "secure_boot") => "安全启动：",
        (Language::En, "secure_boot") => "Secure Boot:",
        (Language::Zh, "loading") => "正在读取状态…",
        (Language::En, "loading") => "Loading metrics...",
        (Language::Zh, "temperature") => "温度：",
        (Language::En, "temperature") => "Temperature:",
        (Language::Zh, "current_mode") => "当前模式：",
        (Language::En, "current_mode") => "Current mode:",
        (Language::Zh, "power_mode") => "电源模式",
        (Language::En, "power_mode") => "Power Mode",
        (Language::Zh, "processor") => "处理器",
        (Language::En, "processor") => "Processor",
        (Language::Zh, "quiet") => "安静",
        (Language::En, "quiet") => "Quiet",
        (Language::Zh, "balanced") => "均衡",
        (Language::En, "balanced") => "Balanced",
        (Language::Zh, "performance") => "性能",
        (Language::En, "performance") => "Performance",
        (Language::Zh, "fan1") => "风扇 1",
        (Language::En, "fan1") => "Fan1",
        (Language::Zh, "fan2") => "风扇 2",
        (Language::En, "fan2") => "Fan2",
        (Language::Zh, "fan3") => "风扇 3",
        (Language::En, "fan3") => "Fan3",
        (Language::Zh, "fan_cpu") => "CPU 风扇",
        (Language::En, "fan_cpu") => "CPU Fan",
        (Language::Zh, "fan_secondary") => "第二 CPU 风扇",
        (Language::En, "fan_secondary") => "Secondary CPU Fan",
        (Language::Zh, "fan_system") => "系统风扇",
        (Language::En, "fan_system") => "System Fan",
        (Language::Zh, "mode") => "模式：",
        (Language::En, "mode") => "Mode:",
        (Language::Zh, "rpm") => "转速：",
        (Language::En, "rpm") => "RPM:",
        (Language::Zh, "level") => "档位：",
        (Language::En, "level") => "Level:",
        (Language::Zh, "ramp_up") => "升温曲线：",
        (Language::En, "ramp_up") => "Ramp-Up:",
        (Language::Zh, "ramp_down") => "降温曲线：",
        (Language::En, "ramp_down") => "Ramp-Down:",
        (Language::Zh, "hint_curve") => "改升档温度；右侧是自动降档（约低 8°C）。",
        (Language::En, "hint_curve") => {
            "Edit ramp-up temperatures. The right column is automatic ramp-down (about 8°C lower)."
        }
        (Language::Zh, "settings") => "设置",
        (Language::En, "settings") => "Settings",
        (Language::Zh, "back") => "返回",
        (Language::En, "back") => "Back",
        (Language::Zh, "close_window") => "关闭窗口",
        (Language::En, "close_window") => "Close window",
        (Language::Zh, "close_to_tray") => "最小化到托盘",
        (Language::En, "close_to_tray") => "Minimize to tray",
        (Language::Zh, "close_quit") => "直接退出",
        (Language::En, "close_quit") => "Quit the program",
        (Language::Zh, "start_with_windows") => "开机时启动",
        (Language::En, "start_with_windows") => "Start with Windows",
        (Language::Zh, "autostart_needs_install") => {
            "开机启动仅支持安装到 Program Files 的版本。请使用安装包。"
        }
        (Language::En, "autostart_needs_install") => {
            "Start with Windows is only available for the Program Files install. Use the installer."
        }
        (Language::Zh, "language") => "语言",
        (Language::En, "language") => "Language",
        (Language::Zh, "lang_zh") => "中文",
        (Language::En, "lang_zh") => "中文",
        (Language::Zh, "lang_en") => "English",
        (Language::En, "lang_en") => "English",
        (Language::Zh, "edit") => "编辑",
        (Language::En, "edit") => "Edit",
        (Language::Zh, "rename") => "重命名",
        (Language::En, "rename") => "Rename",
        (Language::Zh, "restore_default") => "恢复默认",
        (Language::En, "restore_default") => "Restore default",
        (Language::Zh, "apply") => "应用",
        (Language::En, "apply") => "Apply",
        (Language::Zh, "show_window") => "显示窗口",
        (Language::En, "show_window") => "Show window",
        (Language::Zh, "exit") => "退出",
        (Language::En, "exit") => "Exit",
        (Language::Zh, "error") => "错误：",
        (Language::En, "error") => "Error:",
        (Language::Zh, "admin_required") => {
            "需要管理员权限才能通过 PawnIO 访问 EC。"
        }
        (Language::En, "admin_required") => {
            "This application must be run as Administrator to access the EC through PawnIO."
        }
        (Language::Zh, "pawnio_missing") => {
            "需要安装官方 PawnIO 才能访问硬件。\n\n安全启动可以保持开启。\n\n请安装官方 PawnIO 后重新打开本程序。\n\n确定后将打开 https://pawnio.eu/"
        }
        (Language::En, "pawnio_missing") => {
            "PawnIO is required for hardware access.\n\nSecure Boot can remain enabled.\n\nInstall the official PawnIO release and restart this application.\n\nOK will open https://pawnio.eu/"
        }
        (Language::Zh, "already_running") => "ec-su_axb35-win 已在运行。",
        (Language::En, "already_running") => "ec-su_axb35-win is already running.",
        (Language::Zh, "auto") => "自动",
        (Language::En, "auto") => "auto",
        (Language::Zh, "fixed") => "固定",
        (Language::En, "fixed") => "fixed",
        (Language::Zh, "curve") => "曲线",
        (Language::En, "curve") => "curve",
        (Language::Zh, "usage_cli") => "用法: evox2ctl [--json] <mode|status|diagnose|fan>",
        (Language::En, "usage_cli") => "Usage: evox2ctl [--json] <mode|status|diagnose|fan>",
        (Language::Zh, "invalid_power_mode") => "无效电源模式。请使用 quiet、balanced 或 performance。",
        (Language::En, "invalid_power_mode") => {
            "Invalid power mode. Use quiet, balanced, or performance."
        }
        (Language::Zh, "would_set_pmode") => "将设置 EVO-X2 P-MODE：",
        (Language::En, "would_set_pmode") => "Would set EVO-X2 P-MODE:",
        (Language::Zh, "register") => "寄存器：",
        (Language::En, "register") => "register:",
        (Language::Zh, "value_label") => "数值：",
        (Language::En, "value_label") => "value:",
        (Language::Zh, "mode_name") => "模式：",
        (Language::En, "mode_name") => "mode:",
        (Language::Zh, "model") => "型号：",
        (Language::En, "model") => "Model:",
        (Language::Zh, "power_mode_status") => "电源模式：",
        (Language::En, "power_mode_status") => "Power Mode:",
        (Language::Zh, "apu_temp") => "处理器温度：",
        (Language::En, "apu_temp") => "Processor temp:",
        (Language::Zh, "ec_raw_temp") => "EC 0x70 原始温度：",
        (Language::En, "ec_raw_temp") => "EC 0x70 raw temp:",
        (Language::Zh, "temp_src_gpu") => "GPU（任务管理器 / ADL）",
        (Language::En, "temp_src_gpu") => "GPU (Task Manager / ADL)",
        (Language::Zh, "temp_src_cpu") => "CPU（AMD ADL）",
        (Language::En, "temp_src_cpu") => "CPU (AMD ADL)",
        (Language::Zh, "temp_src_soc") => "SoC（AMD ADL）",
        (Language::En, "temp_src_soc") => "SoC (AMD ADL)",
        (Language::Zh, "temp_src_hotspot") => "GPU 热点（AMD ADL）",
        (Language::En, "temp_src_hotspot") => "GPU hotspot (AMD ADL)",
        (Language::Zh, "temp_src_ec") => "EC 0x70（不准）",
        (Language::En, "temp_src_ec") => "EC 0x70 (inaccurate)",
        (Language::Zh, "temp_source") => "温度基准",
        (Language::En, "temp_source") => "Temperature source",
        (Language::Zh, "temp_source_hint") => {
            "主页大数字、风扇曲线和温度告警使用此传感器。读不到时回退到 GPU。"
        }
        (Language::En, "temp_source_hint") => {
            "Used for the processor reading, fan curves, and alerts. Falls back to GPU if unavailable."
        }
        (Language::Zh, "smoothing_window") => "曲线平滑窗口",
        (Language::En, "smoothing_window") => "Curve smoothing window",
        (Language::Zh, "smoothing_window_hint") => {
            "用最近 N 次温度的平均再决定升/降档。1 为每次采样立即换档。"
        }
        (Language::En, "smoothing_window_hint") => {
            "Average the last N temperature readings before changing fan level. 1 is immediate."
        }
        (Language::Zh, "curve_needs_gui") => {
            "曲线模式需要正在运行的窗口（evox2-control）。请用图形界面设置，或改用 auto / fixed。"
        }
        (Language::En, "curve_needs_gui") => {
            "Curve mode requires the running window (evox2-control). Use the GUI, or fan auto / fan fixed."
        }
        (Language::Zh, "temp_gpu") => "GPU 温度：",
        (Language::En, "temp_gpu") => "GPU temp:",
        (Language::Zh, "temp_cpu") => "CPU 温度：",
        (Language::En, "temp_cpu") => "CPU temp:",
        (Language::Zh, "temp_soc") => "SoC 温度：",
        (Language::En, "temp_soc") => "SoC temp:",
        (Language::Zh, "temp_hotspot") => "GPU 热点：",
        (Language::En, "temp_hotspot") => "GPU hotspot:",
        (Language::Zh, "cpu_fan") => "CPU 风扇：",
        (Language::En, "cpu_fan") => "CPU Fan:",
        (Language::Zh, "secondary_fan") => "第二 CPU 风扇：",
        (Language::En, "secondary_fan") => "Secondary CPU Fan:",
        (Language::Zh, "system_fan") => "系统风扇：",
        (Language::En, "system_fan") => "System Fan:",
        (Language::Zh, "pawnio") => "PawnIO：",
        (Language::En, "pawnio") => "PawnIO:",
        (Language::Zh, "os") => "操作系统：",
        (Language::En, "os") => "Operating system:",
        (Language::Zh, "architecture") => "架构：",
        (Language::En, "architecture") => "Architecture:",
        (Language::Zh, "admin_status") => "管理员状态：",
        (Language::En, "admin_status") => "Administrator status:",
        (Language::Zh, "administrator") => "管理员",
        (Language::En, "administrator") => "Administrator",
        (Language::Zh, "standard_user") => "标准用户",
        (Language::En, "standard_user") => "Standard user",
        (Language::Zh, "pawnio_status") => "PawnIO 状态：",
        (Language::En, "pawnio_status") => "PawnIO status:",
        (Language::Zh, "pawnio_connected") => "已连接",
        (Language::En, "pawnio_connected") => "Connected",
        (Language::Zh, "pawnio_connected_ver") => "已连接（已安装 ",
        (Language::En, "pawnio_connected_ver") => "Connected (installed ",
        (Language::Zh, "pawnio_unavailable") => {
            "不可用 — 请从 https://pawnio.eu/ 安装官方版本"
        }
        (Language::En, "pawnio_unavailable") => {
            "Unavailable — install the official release from https://pawnio.eu/"
        }
        (Language::Zh, "lpcacpiec_status") => "LpcACPIEC 模块状态：",
        (Language::En, "lpcacpiec_status") => "LpcACPIEC module status:",
        (Language::Zh, "motherboard") => "检测到的主板/产品：",
        (Language::En, "motherboard") => "Detected motherboard/product:",
        (Language::Zh, "firmware_status") => "EC 固件版本：",
        (Language::En, "firmware_status") => "EC firmware version:",
        (Language::Zh, "current_pmode") => "当前 P-MODE：",
        (Language::En, "current_pmode") => "Current P-MODE:",
        (Language::Zh, "secure_boot_status") => "安全启动状态：",
        (Language::En, "secure_boot_status") => "Secure Boot status:",
        (Language::Zh, "hardware_unsupported") => "硬件支持：不受支持 — 已禁止 EC 写入",
        (Language::En, "hardware_unsupported") => {
            "Hardware support: unsupported — EC writes are disabled"
        }
        (Language::Zh, "unknown") => "未知",
        (Language::En, "unknown") => "unknown",
        (Language::Zh, "error_rampup") => "升温曲线必须正好有 5 个值",
        (Language::En, "error_rampup") => "Rampup curve must have exactly 5 values",
        (Language::Zh, "error_rampdown") => "降温曲线必须正好有 5 个值",
        (Language::En, "error_rampdown") => "Rampdown curve must have exactly 5 values",
        (Language::Zh, "about") => "关于",
        (Language::En, "about") => "About",
        (Language::Zh, "diagnostics") => "诊断",
        (Language::En, "diagnostics") => "Diagnostics",
        (Language::Zh, "version") => "版本：",
        (Language::En, "version") => "Version:",
        (Language::Zh, "author") => "作者：",
        (Language::En, "author") => "Author:",
        (Language::Zh, "forked_from") => "派生自：",
        (Language::En, "forked_from") => "Forked from:",
        (Language::Zh, "repository") => "仓库：",
        (Language::En, "repository") => "Repository:",
        (Language::Zh, "copy_diagnostics") => "复制到剪贴板",
        (Language::En, "copy_diagnostics") => "Copy to clipboard",
        (Language::Zh, "open_log") => "打开日志",
        (Language::En, "open_log") => "Open log",
        (Language::Zh, "open_log_dir") => "打开日志目录",
        (Language::En, "open_log_dir") => "Open log folder",
        (Language::Zh, "export_config") => "导出配置",
        (Language::En, "export_config") => "Export config",
        (Language::Zh, "import_config") => "导入配置",
        (Language::En, "import_config") => "Import config",
        (Language::Zh, "import_ok") => "配置已导入并应用到 EC。",
        (Language::En, "import_ok") => "Configuration imported and applied to the EC.",
        (Language::Zh, "temp_alert") => "温度告警",
        (Language::En, "temp_alert") => "Temperature alert",
        (Language::Zh, "temp_alert_threshold") => "告警阈值（°C）",
        (Language::En, "temp_alert_threshold") => "Alert threshold (°C)",
        (Language::Zh, "temp_alert_body") => "处理器温度达到 ",
        (Language::En, "temp_alert_body") => "Processor temperature reached ",
        (Language::Zh, "pawnio_lgpl_note") => "内置 LpcACPIEC 模块来自 PawnIO.Modules，许可证为 LGPL-2.1-or-later。PawnIO 驱动需单独从 pawnio.eu 安装。",
        (Language::En, "pawnio_lgpl_note") => "The bundled LpcACPIEC module is from PawnIO.Modules under LGPL-2.1-or-later. Install the PawnIO driver separately from pawnio.eu.",
        (Language::Zh, "copied") => "已复制",
        (Language::En, "copied") => "Copied",
        (Language::Zh, "app_name") => APP_NAME,
        (Language::En, "app_name") => APP_NAME,
        (_, key) => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_english() {
        assert_eq!(Language::from_code("zh"), Language::Zh);
        assert_eq!(Language::from_code("de"), Language::En);
        assert_eq!(Language::from_code("en"), Language::En);
        assert_eq!(t(Language::Zh, "apply"), "应用");
        assert_eq!(t(Language::En, "edit"), "Edit");
        assert_eq!(t(Language::Zh, "back"), "返回");
        assert_eq!(t(Language::En, "settings"), "Settings");
        assert_eq!(t(Language::En, "processor"), "Processor");
        assert_eq!(t(Language::Zh, "processor"), "处理器");
        assert_eq!(t(Language::Zh, "restore_default"), "恢复默认");
        assert_eq!(t(Language::En, "rename"), "Rename");
        assert_eq!(
            t(Language::En, "smoothing_window"),
            "Curve smoothing window"
        );
        assert_eq!(
            t(Language::Zh, "curve_needs_gui"),
            "曲线模式需要正在运行的窗口（evox2-control）。请用图形界面设置，或改用 auto / fixed。"
        );
        assert_eq!(
            t(Language::Zh, "hint_curve"),
            "改升档温度；右侧是自动降档（约低 8°C）。"
        );
        assert!(t(Language::En, "hint_curve").contains("8°C"));
        assert!(t(Language::Zh, "autostart_needs_install").contains("Program Files"));
        assert!(t(Language::En, "autostart_needs_install").contains("Program Files"));
    }
}
