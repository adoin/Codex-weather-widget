# Codex Calendar

一个用 Rust 原生重写的 Codex 额度桌面挂件。它保留了 [Codex Quota Weather](https://github.com/fantarunning/codex-quota-weather) 的本地额度读取、实时计划额度、Codex 显隐跟随和 `/quota` 控制能力，并改成安静的桌面日程卡视觉。

![Rust](https://img.shields.io/badge/Rust-1.85%2B-3f4f45?style=flat-square) ![Windows desktop](https://img.shields.io/badge/Windows-desktop%20widget-b86548?style=flat-square)

## 已实现

- 读取 `~/.codex/sessions/**/*.jsonl`，统计今日 Token、调用数、会话数和当前上下文；并按项目与本地日期汇总近 7 日 Token。累计计数使用差分，子代理重放的父任务历史只作为基线，不重复计入。
- 使用 Codex 本地登录凭据请求官方客户端使用的 `wham/usage` 接口；失败时自动回退到最近会话里的 `rate_limits`。
- Windows 原生圆角裁剪、系统桌面吸附、托盘、单实例唤醒、Codex/ChatGPT 进程显隐跟随；也可切回普通置顶窗口。
- 设置中可启用“开机启动”；Windows 会为当前用户创建延迟 10 秒的登录计划任务，并在安装路径变化后自动同步。
- 雨、雷雨、雪、晴、多云、阴、雾七组天气场景，每组 3 张本地背景；雨天使用玻璃雨滴动画，雷雨额外模拟远处闪电的明暗变化，雪天使用多层雪花动画。天气支持免配置的 Open-Meteo，也支持更适合中国网络环境的和风天气；自动模式会优先和风并在失败时回退。
- 背景图片透明度可在设置中调节并持久化；照片与毛玻璃采样同步衰减，底层直接透出桌面。
- Windows 桌面吸附模式支持系统壁纸采样式毛玻璃：后台将当前壁纸缩小并预模糊，按挂件在当前显示器中的位置裁切；拖动期间暂时使用真实透明背景，松手后恢复，避免模糊纹理追赶。
- 日程卡式日期、周视图、额度圆环和今日活动；支持系统减少动态设置。
- 一周预报使用内置的 QWeather Filled 彩色 SVG 图标，不依赖操作系统字体或在线图标服务。
- 内置 `zh-CN` 与 `en-US` 界面；首次启动根据系统语言选择（中文区域使用中文，其他区域使用英文），也可在设置中手动切换并持久化。
- 仅监听 `127.0.0.1` 的动作 API，与 `/quota` 技能兼容。

所有额度和账户数据只在本机内存中处理。应用只读取 `auth.json` 和会话日志，不会修改它们，也不会把访问令牌写入日志或 API 响应。只有在搜索天气地址时，地址文字会发送到当前选择的天气服务；选中的经纬度用于请求天气预报，不会发送 Codex 账户或用量数据。和风天气 API Key 保存在 Windows 凭据管理器或 macOS 钥匙串中，不会写入 `config.json`。

## 开发环境

需要：

- Rust stable（建议 `rustup update stable`）
- Windows 10/11，或 macOS 13+
- Windows 使用 MSVC toolchain：`rustup default stable-x86_64-pc-windows-msvc`

```powershell
git clone https://github.com/adoin/Codex-weather-widget.git
cd Codex-weather-widget
cargo test --all-targets
powershell -ExecutionPolicy Bypass -File .\scripts\dev.ps1
```

`dev.ps1` 会监控 `src/`、`assets/`、`Cargo.toml` 和 `Cargo.lock`。文件写入稳定后自动增量构建；构建成功才替换并重启当前开发挂件，构建失败时保留上一版继续运行。按 `Ctrl+C` 会同时关闭由脚本启动的开发实例。

为了避免 Windows 锁定正在运行的构建产物，脚本从 `target/dev-run/` 中的临时副本运行挂件。启动前如果检测到已安装版本占用了 `127.0.0.1:8787`，脚本会提示先退出，不会结束仓库之外的进程。

调试构建首次会编译 `wgpu`，耗时会明显长于之后的增量构建。仍可使用 `cargo run` 做单次运行。正式构建：

```powershell
cargo build --release
```

Windows 也可以运行 `pwsh -File scripts/install-dev.ps1`：它会构建 release 版本、安装到当前用户目录并安装 `/quota` 技能，不需要管理员权限。开机启动由挂件设置中的开关控制。

## 安装

从 [GitHub Releases](https://github.com/adoin/Codex-weather-widget/releases) 下载 `CodexWeatherWidgetSetup-<版本>.exe` 并运行。安装器不需要管理员权限，会安装到当前用户的 `%LOCALAPPDATA%\Programs\CodexWeatherWidget`，同时安装 `/quota` 技能和开始菜单快捷方式。是否随 Windows 登录启动由挂件设置中的“开机启动”独立控制。

## 交互

| 操作 | 结果 |
| --- | --- |
| 点击上半部分的背景照片 | 切换同一天气主题的下一张照片 |
| 点击 `设置` | 设置语言、天气地址、桌面吸附、开机启动、跟随和减少动态 |
| 点击右上角锁图标 | 在“锁定位置”和“解锁可移动”之间切换 |
| 解锁后拖动卡片顶部 | 移动挂件；停止后位置自动保存到本地配置 |
| 点击额度卡右上角统计图标 | 在原卡片区域查看项目 Token 占比和近 7 日每日总 Token |
| 点击 `×` | 隐藏到托盘 |
| 托盘菜单 | 显示、隐藏或退出 |
| 在 Codex 输入 `/quota` | 显示或隐藏挂件 |

## Action 监听

默认端口为 `8787`，仅绑定本机回环地址：

```text
GET  /health
GET  /quota
POST /panel/show
POST /panel/settings/language
POST /panel/statistics
POST /panel/quota
POST /panel/hide
POST /panel/toggle
POST /panel/quit
```

示例：

```powershell
Invoke-RestMethod http://127.0.0.1:8787/quota
Invoke-RestMethod -Method Post http://127.0.0.1:8787/panel/toggle
```

第二次启动程序时不会再创建窗口，而是调用现有实例的 `toggle` 动作。

## 配置

首次修改设置后会生成：

- Windows：`%APPDATA%\CodexCalendar\config.json`
- macOS：`~/Library/Application Support/CodexCalendar/config.json`

完整字段见 [`config.example.json`](config.example.json)。如需读取另一套 Codex 数据目录，可以在启动前设置 `CODEX_HOME`。
也可以在配置中设置 `codexHome`；未设置时，应用会在本机磁盘中选择最近仍有会话更新的 Codex 数据目录。

`weatherLocation` 接受城市、地区或邮编，默认每 30 分钟刷新一次。在设置中搜索后必须从候选地区中选择一项，应用会保存该项的经纬度。`weatherProvider` 可选 `auto`、`openMeteo` 或 `qWeather`：

- `openMeteo`：使用 [Open-Meteo Geocoding API](https://open-meteo.com/en/docs/geocoding-api) 与 [Forecast API](https://open-meteo.com/en/docs)，无需账户，适合作为全球免配置服务。
- `qWeather`：使用 [和风天气 GeoAPI](https://dev.qweather.com/docs/api/geoapi/city-lookup/) 与七日预报。请在和风控制台创建项目，把专属 API Host 和 API Key 填入设置；API Key 通过系统凭据库安全保存。和风服务覆盖全球，但具体免费额度与可用服务以其当前订阅政策为准，界面中会保留 QWeather 数据来源标识。
- `auto`：如果 Host 和 Key 已配置则优先和风；请求、鉴权或网络失败时自动回退 Open-Meteo。未配置和风时等同 Open-Meteo。

不再请求在中国网络环境下不稳定的 Nominatim。Open-Meteo 找不到完整区县名时，会继续尝试上级城市。每天进入新的本地日期后，预报首日和背景场景会随下一次刷新自动更新。

`desktopGlass` 控制 Windows 桌面毛玻璃，默认开启且只在 `desktopAttached` 生效。`backgroundOpacity` 控制天气照片覆盖毛玻璃的比例：`1.0` 为完整天气照片，`0.0` 为完整桌面毛玻璃；关闭 `desktopGlass` 后，透明区域直接显示桌面。

## 构建与发布

GitHub Actions 只在推送与 `Cargo.toml` 版本一致的 `v*` 标签时触发。`.github/workflows/release.yml` 会在 Windows runner 上完成测试和 release 构建，通过 Inno Setup 生成 Windows x64 安装程序与 SHA-256 校验文件，并自动发布到 GitHub Release。Release 工作流只使用 3 个可复用 Action，并在每次运行结束时将整个仓库的 workflow runs 清理到最多 3 个；正式 Release 文件不受该清理影响。普通分支 push 和 pull request 不会触发 Actions。

1.1.1 的发布标签为：

```powershell
git tag v1.1.1
git push origin v1.1.1
```

## 致谢与许可

产品功能参考了 MIT 许可的 [fantarunning/codex-quota-weather](https://github.com/fantarunning/codex-quota-weather)。本仓库是独立 Rust 实现，不隶属于 OpenAI。

MIT License，见 [`LICENSE`](LICENSE)。背景照片按 Unsplash License 分发，来源见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。
