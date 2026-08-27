use chrono::Weekday;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    ZhCn,
    EnUs,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::ZhCn, Self::EnUs];

    pub fn from_code(code: Option<&str>) -> Self {
        if code.is_some_and(|code| code.to_ascii_lowercase().starts_with("zh")) {
            Self::ZhCn
        } else {
            Self::EnUs
        }
    }

    pub fn system_default() -> Self {
        Self::from_code(sys_locale::get_locale().as_deref())
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English (US)",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Text {
    Close,
    HideToTray,
    Settings,
    SettingsTooltip,
    LockedTooltip,
    UnlockedTooltip,
    WeatherUnsetTooltip,
    FocusPrefix,
    TodayWork,
    QuotaRemaining,
    RemainingSuffix,
    PlanSuffix,
    WaitingQuota,
    CallsToday,
    Sessions,
    TokensToday,
    LocalDailyTotal,
    WeatherAddress,
    LocationPlaceholder,
    SearchLocation,
    SearchMinimum,
    SearchingLocations,
    SearchFailed,
    SelectForecastLocation,
    SelectedLocation,
    WeatherUpdateFailed,
    LocatedAt,
    SearchLocationHint,
    FollowCodex,
    ReduceMotion,
    FollowWeather,
    AttachDesktop,
    AlwaysOnTop,
    WidgetSettings,
    Language,
    BackgroundOpacity,
    DesktopGlass,
    FiveHours,
    SevenDays,
    ThisPeriod,
    ResetAfter,
    SelectLocationInSettings,
    TrayToggle,
    TrayShow,
    TrayQuit,
    TrayTooltip,
    LiveQuotaUnavailable,
    MoreStatistics,
    UsageStatistics,
    ManualResetsLeft,
    UsedLabel,
    Back,
    RecentSevenDays,
    ProjectShare,
    DailyTokens,
    NoLocalUsage,
    CcusageHint,
    OtherProject,
}

pub const fn tr(language: Language, text: Text) -> &'static str {
    match (language, text) {
        (Language::ZhCn, Text::Close) => "关闭",
        (Language::EnUs, Text::Close) => "Close",
        (Language::ZhCn, Text::HideToTray) => "隐藏到托盘",
        (Language::EnUs, Text::HideToTray) => "Hide to tray",
        (Language::ZhCn, Text::Settings) => "设置",
        (Language::EnUs, Text::Settings) => "Settings",
        (Language::ZhCn, Text::SettingsTooltip) => "地址、天气与桌面吸附",
        (Language::EnUs, Text::SettingsTooltip) => "Location, weather and desktop mode",
        (Language::ZhCn, Text::LockedTooltip) => "已锁定位置，点击解锁",
        (Language::EnUs, Text::LockedTooltip) => "Position locked; click to unlock",
        (Language::ZhCn, Text::UnlockedTooltip) => "已解锁，可拖动顶部；点击锁定",
        (Language::EnUs, Text::UnlockedTooltip) => "Unlocked; drag the top area or click to lock",
        (Language::ZhCn, Text::WeatherUnsetTooltip) => "设置地址后自动跟随今日天气",
        (Language::EnUs, Text::WeatherUnsetTooltip) => "Set a location to follow today's weather",
        (Language::ZhCn, Text::FocusPrefix) => "正在专注",
        (Language::EnUs, Text::FocusPrefix) => "Focusing on",
        (Language::ZhCn, Text::TodayWork) => "今天的工作",
        (Language::EnUs, Text::TodayWork) => "today's work",
        (Language::ZhCn, Text::QuotaRemaining) => "额度剩余",
        (Language::EnUs, Text::QuotaRemaining) => "Quota left",
        (Language::ZhCn, Text::RemainingSuffix) => "剩余",
        (Language::EnUs, Text::RemainingSuffix) => " left",
        (Language::ZhCn, Text::PlanSuffix) => "计划",
        (Language::EnUs, Text::PlanSuffix) => "plan",
        (Language::ZhCn, Text::WaitingQuota) => "等待额度数据",
        (Language::EnUs, Text::WaitingQuota) => "Waiting for quota data",
        (Language::ZhCn, Text::CallsToday) => "今日调用",
        (Language::EnUs, Text::CallsToday) => "Calls today",
        (Language::ZhCn, Text::Sessions) => "个会话",
        (Language::EnUs, Text::Sessions) => "sessions",
        (Language::ZhCn, Text::TokensToday) => "今日 Token",
        (Language::EnUs, Text::TokensToday) => "Tokens today",
        (Language::ZhCn, Text::LocalDailyTotal) => "本地今日累计",
        (Language::EnUs, Text::LocalDailyTotal) => "Local daily total",
        (Language::ZhCn, Text::WeatherAddress) => "天气地址",
        (Language::EnUs, Text::WeatherAddress) => "Weather location",
        (Language::ZhCn, Text::LocationPlaceholder) => "例如：上海市浦东新区",
        (Language::EnUs, Text::LocationPlaceholder) => "e.g. Brooklyn, New York",
        (Language::ZhCn, Text::SearchLocation) => "搜索地区",
        (Language::EnUs, Text::SearchLocation) => "Search",
        (Language::ZhCn, Text::SearchMinimum) => "请至少输入两个字符",
        (Language::EnUs, Text::SearchMinimum) => "Enter at least two characters",
        (Language::ZhCn, Text::SearchingLocations) => "正在搜索可用预报地区…",
        (Language::EnUs, Text::SearchingLocations) => "Searching forecast locations…",
        (Language::ZhCn, Text::SearchFailed) => "地区搜索失败",
        (Language::EnUs, Text::SearchFailed) => "Location search failed",
        (Language::ZhCn, Text::SelectForecastLocation) => "请选择实际预报地区：",
        (Language::EnUs, Text::SelectForecastLocation) => "Choose a forecast location:",
        (Language::ZhCn, Text::SelectedLocation) => "已选择",
        (Language::EnUs, Text::SelectedLocation) => "Selected",
        (Language::ZhCn, Text::WeatherUpdateFailed) => "天气更新失败",
        (Language::EnUs, Text::WeatherUpdateFailed) => "Weather update failed",
        (Language::ZhCn, Text::LocatedAt) => "已定位",
        (Language::EnUs, Text::LocatedAt) => "Located",
        (Language::ZhCn, Text::SearchLocationHint) => "搜索后选择一个地区，再自动更新预报",
        (Language::EnUs, Text::SearchLocationHint) => {
            "Search and choose a location to update the forecast"
        }
        (Language::ZhCn, Text::FollowCodex) => "跟随 Codex",
        (Language::EnUs, Text::FollowCodex) => "Follow Codex",
        (Language::ZhCn, Text::ReduceMotion) => "减少动态",
        (Language::EnUs, Text::ReduceMotion) => "Reduce motion",
        (Language::ZhCn, Text::FollowWeather) => "跟随天气",
        (Language::EnUs, Text::FollowWeather) => "Follow weather",
        (Language::ZhCn, Text::AttachDesktop) => "吸附到系统桌面",
        (Language::EnUs, Text::AttachDesktop) => "Attach to desktop",
        (Language::ZhCn, Text::AlwaysOnTop) => "窗口置顶",
        (Language::EnUs, Text::AlwaysOnTop) => "Always on top",
        (Language::ZhCn, Text::WidgetSettings) => "挂件设置",
        (Language::EnUs, Text::WidgetSettings) => "Widget settings",
        (Language::ZhCn, Text::Language) => "语言",
        (Language::EnUs, Text::Language) => "Language",
        (Language::ZhCn, Text::BackgroundOpacity) => "背景图片透明度",
        (Language::EnUs, Text::BackgroundOpacity) => "Background image opacity",
        (Language::ZhCn, Text::DesktopGlass) => "桌面毛玻璃",
        (Language::EnUs, Text::DesktopGlass) => "Desktop frosted glass",
        (Language::ZhCn, Text::FiveHours) => "5小时",
        (Language::EnUs, Text::FiveHours) => "5 hours",
        (Language::ZhCn, Text::SevenDays) => "7天",
        (Language::EnUs, Text::SevenDays) => "7 days",
        (Language::ZhCn, Text::ThisPeriod) => "本周期",
        (Language::EnUs, Text::ThisPeriod) => "Period",
        (Language::ZhCn, Text::ResetAfter) => "后重置",
        (Language::EnUs, Text::ResetAfter) => "until reset",
        (Language::ZhCn, Text::SelectLocationInSettings) => "请在设置中搜索并选择一个预报地区",
        (Language::EnUs, Text::SelectLocationInSettings) => {
            "Search and choose a forecast location in Settings"
        }
        (Language::ZhCn, Text::TrayToggle) => "显示 / 隐藏",
        (Language::EnUs, Text::TrayToggle) => "Show / Hide",
        (Language::ZhCn, Text::TrayShow) => "立即显示",
        (Language::EnUs, Text::TrayShow) => "Show now",
        (Language::ZhCn, Text::TrayQuit) => "退出 Codex Calendar",
        (Language::EnUs, Text::TrayQuit) => "Quit Codex Calendar",
        (Language::ZhCn, Text::TrayTooltip) => "Codex Calendar · 本周额度",
        (Language::EnUs, Text::TrayTooltip) => "Codex Calendar · Weekly quota",
        (Language::ZhCn, Text::LiveQuotaUnavailable) => "实时额度暂不可用",
        (Language::EnUs, Text::LiveQuotaUnavailable) => "Live quota temporarily unavailable",
        (Language::ZhCn, Text::MoreStatistics) => "更多统计",
        (Language::EnUs, Text::MoreStatistics) => "More statistics",
        (Language::ZhCn, Text::UsageStatistics) => "额度统计",
        (Language::EnUs, Text::UsageStatistics) => "Usage statistics",
        (Language::ZhCn, Text::ManualResetsLeft) => "剩余手动重置",
        (Language::EnUs, Text::ManualResetsLeft) => "Manual resets left",
        (Language::ZhCn, Text::UsedLabel) => "已使用",
        (Language::EnUs, Text::UsedLabel) => "Used",
        (Language::ZhCn, Text::Back) => "返回额度",
        (Language::EnUs, Text::Back) => "Back to quota",
        (Language::ZhCn, Text::RecentSevenDays) => "近 7 日统计",
        (Language::EnUs, Text::RecentSevenDays) => "Last 7 days",
        (Language::ZhCn, Text::ProjectShare) => "项目 Token 占比",
        (Language::EnUs, Text::ProjectShare) => "Tokens by project",
        (Language::ZhCn, Text::DailyTokens) => "每日总 Token",
        (Language::EnUs, Text::DailyTokens) => "Daily total tokens",
        (Language::ZhCn, Text::NoLocalUsage) => "尚未找到本地 Token 记录",
        (Language::EnUs, Text::NoLocalUsage) => "No local token history found",
        (Language::ZhCn, Text::CcusageHint) => {
            "请确认 Codex 会话目录，或用 ccusage 检查 JSONL 记录"
        }
        (Language::EnUs, Text::CcusageHint) => {
            "Check the Codex sessions directory or inspect the JSONL files with ccusage"
        }
        (Language::ZhCn, Text::OtherProject) => "其他项目",
        (Language::EnUs, Text::OtherProject) => "Other projects",
    }
}

pub fn month_name(language: Language, month: u32) -> &'static str {
    let index = (month.saturating_sub(1) as usize).min(11);
    match language {
        Language::ZhCn => [
            "一月",
            "二月",
            "三月",
            "四月",
            "五月",
            "六月",
            "七月",
            "八月",
            "九月",
            "十月",
            "十一月",
            "十二月",
        ][index],
        Language::EnUs => [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ][index],
    }
}

pub const fn weekday_name(language: Language, day: Weekday) -> &'static str {
    match (language, day) {
        (Language::ZhCn, Weekday::Mon) => "星期一",
        (Language::ZhCn, Weekday::Tue) => "星期二",
        (Language::ZhCn, Weekday::Wed) => "星期三",
        (Language::ZhCn, Weekday::Thu) => "星期四",
        (Language::ZhCn, Weekday::Fri) => "星期五",
        (Language::ZhCn, Weekday::Sat) => "星期六",
        (Language::ZhCn, Weekday::Sun) => "星期日",
        (Language::EnUs, Weekday::Mon) => "Monday",
        (Language::EnUs, Weekday::Tue) => "Tuesday",
        (Language::EnUs, Weekday::Wed) => "Wednesday",
        (Language::EnUs, Weekday::Thu) => "Thursday",
        (Language::EnUs, Weekday::Fri) => "Friday",
        (Language::EnUs, Weekday::Sat) => "Saturday",
        (Language::EnUs, Weekday::Sun) => "Sunday",
    }
}

pub const fn weekday_short(language: Language) -> [&'static str; 7] {
    match language {
        Language::ZhCn => ["一", "二", "三", "四", "五", "六", "日"],
        Language::EnUs => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_regions_use_chinese_and_everything_else_uses_english() {
        for code in ["zh-CN", "zh-TW", "zh-HK", "ZH-sg"] {
            assert_eq!(Language::from_code(Some(code)), Language::ZhCn);
        }
        for code in ["en-US", "ja-JP", "de-DE"] {
            assert_eq!(Language::from_code(Some(code)), Language::EnUs);
        }
        assert_eq!(Language::from_code(None), Language::EnUs);
    }

    #[test]
    fn english_weekday_abbreviations_are_unambiguous() {
        assert_eq!(
            weekday_short(Language::EnUs),
            ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        );
    }
}
