#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        match code {
            "en" => Language::En,
            _ => Language::Zh,
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
        (Language::Zh, "app_title") => "EVO-X2 控制",
        (Language::En, "app_title") => "EVO-X2 Control",
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
        (Language::Zh, "hint_curve") => "提示：5 个温度阈值（°C），用逗号分隔。",
        (Language::En, "hint_curve") => "Hint: 5 temperature thresholds (°C), comma separated.",
        (Language::Zh, "settings") => "设置",
        (Language::En, "settings") => "Settings",
        (Language::Zh, "close_window") => "关闭窗口",
        (Language::En, "close_window") => "Close window",
        (Language::Zh, "close_to_tray") => "最小化到托盘",
        (Language::En, "close_to_tray") => "Minimize to tray",
        (Language::Zh, "close_quit") => "直接退出",
        (Language::En, "close_quit") => "Quit the program",
        (Language::Zh, "start_with_windows") => "开机时启动",
        (Language::En, "start_with_windows") => "Start with Windows",
        (Language::Zh, "language") => "语言",
        (Language::En, "language") => "Language",
        (Language::Zh, "lang_zh") => "中文",
        (Language::En, "lang_zh") => "中文",
        (Language::Zh, "lang_en") => "English",
        (Language::En, "lang_en") => "English",
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
        (Language::Zh, "already_running") => "EVO-X2 控制已在运行。",
        (Language::En, "already_running") => "EVO-X2 Control is already running.",
        (Language::Zh, "auto") => "自动",
        (Language::En, "auto") => "auto",
        (Language::Zh, "fixed") => "固定",
        (Language::En, "fixed") => "fixed",
        (Language::Zh, "curve") => "曲线",
        (Language::En, "curve") => "curve",
        (Language::Zh, "usage_cli") => "用法: evox2ctl <mode|status|diagnose>",
        (Language::En, "usage_cli") => "Usage: evox2ctl <mode|status|diagnose>",
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
        (Language::Zh, "apu_temp") => "APU 温度：",
        (Language::En, "apu_temp") => "APU Temp:",
        (Language::Zh, "ec_raw_temp") => "EC 0x70 原始温度：",
        (Language::En, "ec_raw_temp") => "EC 0x70 raw temp:",
        (Language::Zh, "temp_src_gpu") => "GPU 驱动（任务管理器）",
        (Language::En, "temp_src_gpu") => "GPU driver (Task Manager)",
        (Language::Zh, "temp_src_adl") => "AMD ADL",
        (Language::En, "temp_src_adl") => "AMD ADL",
        (Language::Zh, "temp_src_ec") => "EC 0x70",
        (Language::En, "temp_src_ec") => "EC 0x70",
        (Language::Zh, "cpu_fan") => "CPU 风扇：",
        (Language::En, "cpu_fan") => "CPU Fan:",
        (Language::Zh, "secondary_fan") => "第二 CPU/APU 风扇：",
        (Language::En, "secondary_fan") => "Secondary CPU/APU Fan:",
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
        (_, key) => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_chinese() {
        assert_eq!(Language::from_code("zh"), Language::Zh);
        assert_eq!(Language::from_code("de"), Language::Zh);
        assert_eq!(Language::from_code("en"), Language::En);
        assert_eq!(t(Language::Zh, "quiet"), "安静");
        assert_eq!(t(Language::En, "quiet"), "Quiet");
    }
}
