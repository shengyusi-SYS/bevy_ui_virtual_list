# 开发指南

[English](development.md) · [使用说明](../README.zh-CN.md)

## 准备环境

安装 Rust 1.95 或更新版本，以及 Cargo 和 rustfmt。库使用官方 Bevy 0.19.1。以下命令在本仓库根目录运行。

Ubuntu 的桌面构建依赖：

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev libwayland-dev libxkbcommon-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxinerama-dev
```

Windows 安装 Visual Studio C++ 构建工具和 Windows SDK。macOS 通过 `xcode-select --install` 安装 Xcode 命令行工具。运行示例需要桌面会话和 Bevy 支持的显卡驱动。

## 构建与检查

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo test --lib
cargo doc --no-deps
cargo run --example basic
```

自动化测试在内存中验证几何、ECS 系统和布局；输入与外观通过窗口示例检查。生成的 API 文档位于 `target/doc/bevy_ui_virtual_list/index.html`。CI 使用 Linux 和稳定版 Rust，执行格式、编译、单元测试及 API 文档检查。

## 源码导航

| 文件 | 职责 |
| --- | --- |
| `src/lib.rs` | 公开配置、插件和可见范围更新 |
| `src/geometry.rs` | 内容高度、行位置、实际偏移和外层视口交集 |
| `src/managed.rs` | 内容节点、行分配、位置和交互清理 |
| `src/navigation.rs` | 索引定位、取消和运动适配事件 |
| `src/tests.rs`、`src/managed_tests.rs` | 几何、生命周期和布局回归测试 |
| `examples/basic.rs` | 可运行的交互演示 |

Update 在 `PooledListRangeUpdate` 中依次准备内容高度、处理等待定位、计算可见范围和分配行。随后按顺序运行应用的 Build 和 Bind 系统。PostUpdate 检查布局，新内容高度或视口需要跟进时请求下一帧。

修改行分配或定位后，在示例的两种视口模式中检查滚动、窗口缩放、同数量替换、缩短数据和尾部定位。确认复用后的文字与动作对应新索引。异步内容的条目身份检查保留在调用方的绑定代码中。

## 修改流程

从受影响的公开组件或事件出发，追踪到消费它的系统。行为变化增加有针对性的回归测试。库的数据使用 Bevy 实体、组件和几何信息表达，应用内容通过公开事件和绑定入口接入。

公开 API 或准备步骤变化时，同步更新中英文 README。开发步骤集中在本指南，API 细节放在公开类型旁。提交 pull request 前运行上面的检查，并说明用户可见的结果和验证方式。

## 发布

更新包版本及两份 README 安装示例中的 Git tag。完成检查并提交后，在该提交上创建 `vX.Y.Z` 标签，推送提交和标签，再创建含简短中英文说明的 GitHub release。发布内容保留随库提供的许可证文件。
