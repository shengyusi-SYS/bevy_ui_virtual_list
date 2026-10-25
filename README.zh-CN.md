# bevy_ui_virtual_list

[English](README.md) · [开发指南](docs/development.zh-CN.md)

在 Bevy UI 中显示固定行高的长列表，复用一组行实体。库负责创建和排列可见区域附近的行，你的系统负责填入文字、图片和交互动作。

## 安装

使用 Rust 1.95 或更新版本，以及 Bevy 0.19.1。

```toml
[dependencies]
bevy = { version = "=0.19.1", default-features = false, features = ["ui", "bevy_ui_widgets"] }
bevy_ui_virtual_list = { git = "https://github.com/shengyusi-SYS/bevy_ui_virtual_list.git", tag = "v0.1.0" }
```

## 创建包含 10,000 行的列表

```rust
use bevy::{prelude::*, ui_widgets::ScrollArea};
use bevy_ui_virtual_list::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, PooledListPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (
            build_rows.in_set(PooledListSystems::Build),
            bind_rows.in_set(PooledListSystems::Bind),
        ))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        PooledListBundle::new(10_000, 32.0, 4.0),
        ScrollArea,
        Node {
            width: px(400),
            height: px(400),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    ));
}

fn build_rows(mut commands: Commands, rows: Query<Entity, With<NeedBuild>>) {
    for row in &rows {
        commands.entity(row).with_child(Text::new(""));
        commands.entity(row).remove::<NeedBuild>();
    }
}

fn bind_rows(
    mut commands: Commands,
    rows: Query<(Entity, &NeedRebind, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (row, binding, children) in &rows {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = format!("Row {}", binding.target_index + 1);
            }
        }
        commands.entity(row).remove::<NeedRebind>();
    }
}
```

`PooledListBundle::new(count, row_height, gap)` 配置一个纵向列表。行高使用正的有限值，间距使用非负有限值，单位均为逻辑像素。`PooledList::buffer_count` 控制视口上下额外保留的行数，默认每侧五行。实体池随视口容量增长，滚动时复用已经分配的行。

## 创建一次，按需重绑

| 组件或系统集合 | 调用方的工作 |
| --- | --- |
| `PooledListSystems::Build` 中的 `NeedBuild` | 创建行内长期保留的子节点，完成后移除标记 |
| `PooledListSystems::Bind` 中的 `NeedRebind` | 按 `target_index` 更新已有节点，完成后移除标记 |
| `PooledListOwner` | 找到这行所属的列表 |
| `PooledListItem::data_index` | 读取当前数据位置；`None` 表示槽位空闲 |
| `PooledListState` | 读取包含额外行的可见范围，以及最近的视口尺寸和偏移 |

Build 先于 Bind 执行，两者之间会应用延迟命令。库管理行根节点的位置、高度、显示状态和父节点。可以修改其现有 `Node` 的视觉样式，或将外观放在子节点上。需要自定义内容容器时，添加带 `PooledListContent` 的直接子节点，它的高度会跟随列表完整内容。

每次重绑都应更新与数据有关的文字、图片和动作。异步工作携带稳定的条目 key，完成时核对该行仍代表同一个 key。行被复用或失效时，库会清除原生 `Pressed`、`Interaction` 和后代的 `InputFocus`；应用自有的选中和拖拽状态随内容一起重绑。

## 更新数据

增删条目时修改 `PooledList::total_count`。同数量替换或排序后，在列表实体上插入 `PooledListInvalidated`。库会更新内容高度、行位置和绑定。数据读取和缓存由应用管理。

## 复用外层滚动容器

`PooledListBundle::external(count, height, gap, scroll_owner, viewport_owner)` 使用外层滚动容器。把列表放进外层内容中：`scroll_owner` 持有 `ScrollPosition`，`viewport_owner` 提供可见区域，两者可以是同一个实体。列表向外层贡献完整高度，方便在它前后放置页头或其它内容。

[示例](examples/basic.rs) 将外层滚动模式与自身滚动模式并排展示。范围计算使用 Bevy 实际呈现的偏移，包含像素取整和边界限制。

## 定位与滚动组合

通过 `Commands` 或 `World` 触发 `ScrollPooledListToIndex { list_entity, index }`，将指定行对齐到视口起始处；末尾条目会限制在内容边界。新请求替换旧请求，定位会等待可用布局；列表为空或所属实体被移除时，请求结束。`CancelPooledListScroll` 可取消等待，同时保持当前偏移。

上面的示例使用 Bevy `ScrollArea` 提供原生滚轮输入。需要触摸、惯性和平滑移动时，可以组合 [bevy_ui_scroll](https://github.com/shengyusi-SYS/bevy_ui_scroll)。

默认情况下，解析后的 `PooledListScrollTarget` 会写入 `ScrollPosition.y`。负责运动的适配器可以观察该事件、调用 `take_over()`，再通过滚动库应用它的 `offset`。每个视口安装一个运动适配器。原生滚轮输入会取消等待中的索引定位；自定义触摸和滚动条输入通过 `PooledListScrollInput { scroll_owner }` 转发，边界处的输入也要转发，以便同样取消等待。

相关布局可以使用 `calculate_content_height`、`calculate_item_top` 和 `effective_scroll_position`。`list_extent_is_ready` 用于检查原生布局是否已经反映配置的内容高度；应用自行记录数据是否就绪。

## 运行示例

```sh
git clone https://github.com/shengyusi-SYS/bevy_ui_virtual_list.git
cd bevy_ui_virtual_list
cargo run --example basic
```

滚动任一面板并调整窗口大小。Enter 定位到第 80 行，S 缩短或恢复数据，R 替换数据，Escape 取消等待中的定位。构建准备、测试和源码导航见[开发指南](docs/development.zh-CN.md)。

## 许可证

[MIT](LICENSE)。
