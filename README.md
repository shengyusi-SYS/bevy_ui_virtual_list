# bevy_ui_virtual_list

[简体中文](README.zh-CN.md) · [Developer guide](docs/development.md)

Show long, fixed-height lists in Bevy UI with a reusable pool of row entities.
The library creates and positions rows around the visible area; your systems
fill them with text, images and actions.

## Install

Requires Rust 1.95 or newer and Bevy 0.19.1.

```toml
[dependencies]
bevy = { version = "=0.19.1", default-features = false, features = ["ui", "bevy_ui_widgets"] }
bevy_ui_virtual_list = { git = "https://github.com/shengyusi-SYS/bevy_ui_virtual_list.git", tag = "v0.1.0" }
```

## A list with 10,000 rows

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

`PooledListBundle::new(count, row_height, gap)` configures a vertical list.
Use a positive, finite height and a finite, nonnegative gap, in logical pixels.
`PooledList::buffer_count` adds extra rows above and below the viewport; its
default is five per side. The pool grows with viewport capacity and reuses its
allocated rows as you scroll.

## Build once, bind when needed

| Component or system set | Your task |
| --- | --- |
| `NeedBuild` in `PooledListSystems::Build` | Create the row's persistent child nodes, then remove the marker |
| `NeedRebind` in `PooledListSystems::Bind` | Fill existing nodes using `target_index`, then remove the marker |
| `PooledListOwner` | Find the list that owns a row |
| `PooledListItem::data_index` | Read the row's current data position; `None` means the slot is unused |
| `PooledListState` | Read the visible range, including extra rows, and the latest viewport metrics |

Build runs before Bind, with deferred commands applied between them. Row roots
get their position, height, visibility and parent from the library. Style their
existing `Node` fields or put visual content in children. For a styled content
container, add a direct child marked `PooledListContent`; its height follows the
list's full extent.

When binding, update all data-dependent text, images and actions. Keep a stable
item key with asynchronous work and apply the result only if the row still
represents that key. Native `Pressed`, `Interaction` and descendant `InputFocus`
are cleared when a row is recycled or invalidated. Rebind your own selection and
drag state along with the content.

## Change the data

Set `PooledList::total_count` when items are added or removed. After replacing or
sorting data at the same length, insert `PooledListInvalidated` on the list entity.
The library updates content height, row placement and bindings. Data loading and
caching stay in your application.

## Use an outer scrolling container

`PooledListBundle::external(count, height, gap, scroll_owner, viewport_owner)`
uses an enclosing scroll container. Place the list inside its content.
`scroll_owner` holds `ScrollPosition`; `viewport_owner` provides the visible
rectangle. They can be the same entity. The list contributes its full height to
the outer container, making room for headers and other content before or after it.

The [example](examples/basic.rs) shows this mode next to a self-scrolling list.
Ranges use the offset actually rendered by Bevy, including rounding and clamping.

## Navigate and combine with scrolling

Trigger `ScrollPooledListToIndex { list_entity, index }` through `Commands` or
`World`. It aligns the row with the viewport start and clamps the final rows at
the content end. The newest request wins. Requests wait for usable layout; an
empty list or a removed owner ends the request. `CancelPooledListScroll` cancels a
pending request while keeping the current position.

Use Bevy's `ScrollArea` for native wheel input, as above. For touch, inertia and
smooth movement, combine the list with
[bevy_ui_scroll](https://github.com/shengyusi-SYS/bevy_ui_scroll).

By default, a resolved `PooledListScrollTarget` writes `ScrollPosition.y`.
A motion adapter can observe the event, call `take_over()` and apply its `offset`
through the scrolling library. Install one motion adapter per viewport.
Native wheel input cancels pending index navigation. Forward custom touch and
scrollbar input through `PooledListScrollInput { scroll_owner }`, including
input at a boundary, so it can cancel waiting navigation too.

The geometry helpers `calculate_content_height`, `calculate_item_top` and
`effective_scroll_position` are available for related layout work.
`list_extent_is_ready` reports whether native layout has caught up with the
configured extent; application data readiness is tracked by the caller.

## Try the example

```sh
git clone https://github.com/shengyusi-SYS/bevy_ui_virtual_list.git
cd bevy_ui_virtual_list
cargo run --example basic
```

Scroll either panel and resize the window. Enter moves to row 80, S shortens or
restores the data, R replaces it, and Escape cancels pending navigation.
Build setup, tests and the source map are in the [developer guide](docs/development.md).

## License

Available under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option (`MIT OR Apache-2.0`).
