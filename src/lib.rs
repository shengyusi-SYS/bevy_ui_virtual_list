//! Fixed-height pooled lists for Bevy UI.
//!
//! Install [`PooledListPlugin`] and spawn [`PooledListBundle`] on a bounded viewport.
//! The library owns content extent and row geometry. Construct persistent content
//! on [`NeedBuild`] slots in [`PooledListSystems::Build`], then consume [`NeedRebind`]
//! in [`PooledListSystems::Bind`]. No initial pool or guessed viewport is needed.
//! Business identity, data reads and asynchronous result validation stay with callers.

mod geometry;
pub use geometry::{calculate_content_height, calculate_item_top, effective_scroll_position, list_extent_is_ready};
use geometry::{calculate_visible_range, clamped_pending_scroll_target, resolve_external_metrics};
mod managed;
mod navigation;
pub use managed::{NeedBuild, PooledListContent, PooledListOwner, PooledListSystems};
pub use navigation::{CancelPooledListScroll, PooledListScrollInput, PooledListScrollTarget, ScrollPooledListToIndex};
use navigation::{PendingPooledListScroll, apply_pending_pooled_list_scrolls};

use bevy::{
    app::{App, Plugin, Update},
    ecs::{
        bundle::Bundle,
        component::Component,
        entity::Entity,
        hierarchy::Children,
        name::Name,
        schedule::{IntoScheduleConfigs, SystemSet},
        system::{Commands, Query},
    },
    prelude::{ChildOf, DetectChanges, Ref, RemovedComponents, Visibility},
    ui::{ComputedNode, Node, PositionType, ScrollPosition, UiGlobalTransform, Val},
};
use std::{collections::HashSet, ops::Range};

/// 池化列表配置组件
#[derive(Component)]
pub struct PooledList {
    /// 每个项目的固定高度
    pub item_height: f32,
    /// 总项目数量
    pub total_count: usize,
    /// 上下额外渲染的缓冲项目数
    pub buffer_count: usize,
    /// 列表项之间的间距
    pub gap: f32,
}

/// 池化列表滚动绑定模式。
#[derive(Component, Clone, Copy, Default)]
pub enum PooledListBinding {
    /// 列表实体自身持有 `ScrollPosition + ComputedNode`。
    #[default]
    SelfOwned,
    /// 列表复用外部滚动实体和外部 viewport 几何信息。
    External {
        scroll_owner: Entity,
        viewport_owner: Entity,
    },
}

/// 池化列表运行状态
#[derive(Component, Default)]
pub struct PooledListState {
    /// 当前可见的项目索引范围
    pub visible_range: Range<usize>,
    /// 容器高度
    pub container_height: f32,
    /// 上次观察到的滚动位置
    pub scroll_top: f32,
}

/// 池化列表项标记
#[derive(Component)]
pub struct PooledListItem {
    /// 当前绑定的数据索引（None 表示未绑定）
    pub data_index: Option<usize>,
}

/// 标记需要重新绑定数据的列表项。
///
/// 当滚动导致列表项需要绑定新数据时，系统会添加此组件。
/// 使用者应添加 system 或 observer 来处理并更新列表项内容。
#[derive(Component)]
pub struct NeedRebind {
    pub target_index: usize,
}

/// Insert after replacing data without changing its length. The library consumes
/// this marker and requests rebinding of every currently visible slot.
#[derive(Component, Default)]
pub struct PooledListInvalidated;

/// Internal handoff from range calculation to managed slot allocation.
#[derive(Component, Debug, PartialEq, Eq)]
struct PooledListDeficit(Vec<usize>);

/// Library range/allocation phase, before content construction and binding.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PooledListRangeUpdate;

/// 更新可见范围 + 回收/复用池化项的系统
fn pooled_list_range_update_system(
    mut q_pooled_list: Query<(
        Entity,
        Ref<PooledList>,
        &PooledListBinding,
        &mut PooledListState,
        Option<&PooledListDeficit>,
        Option<&PendingPooledListScroll>,
        Option<&PooledListInvalidated>,
        Option<&ScrollPosition>,
        Option<&ComputedNode>,
        Option<&UiGlobalTransform>,
    )>,
    q_children: Query<&Children>,
    q_parents: Query<&ChildOf>,
    q_list_roots: Query<(), bevy::prelude::With<PooledList>>,
    q_scroll_position: Query<&ScrollPosition>,
    q_computed_node: Query<&ComputedNode>,
    q_ui_global_transform: Query<&UiGlobalTransform>,
    mut q_pooled_list_item: Query<(&mut PooledListItem, &mut Node, Option<Ref<ChildOf>>)>,
    mut focus: Option<bevy::prelude::ResMut<bevy::input_focus::InputFocus>>,
    mut interactions: Query<&mut bevy::ui::Interaction>,
    mut removed_rows: RemovedComponents<PooledListItem>,
    mut commands: Commands,
) {
    let rows_changed = removed_rows.read().count() > 0
        || q_pooled_list_item
            .iter_mut()
            .any(|(item, _, parent)| item.is_changed() || parent.is_some_and(|parent| parent.is_changed()));
    for (
        pooled_list_entity,
        config,
        binding,
        mut state,
        deficit,
        pending_scroll,
        invalidated,
        own_scroll_pos,
        own_computed,
        own_gt,
    ) in q_pooled_list.iter_mut()
    {
        let (scroll_top, container_height) = match binding {
            PooledListBinding::SelfOwned => {
                let (Some(scroll_pos), Some(computed)) = (own_scroll_pos, own_computed) else {
                    continue;
                };
                let container_height = computed.size().y * computed.inverse_scale_factor();
                if pending_scroll.is_some() && container_height <= 0.0 {
                    continue;
                }
                (effective_scroll_position(scroll_pos, computed).y, container_height)
            }
            PooledListBinding::External {
                scroll_owner,
                viewport_owner,
            } => {
                let Ok(scroll_pos) = q_scroll_position.get(*scroll_owner) else {
                    continue;
                };
                let Ok(viewport_computed) = q_computed_node.get(*viewport_owner) else {
                    continue;
                };
                let Ok(viewport_gt) = q_ui_global_transform.get(*viewport_owner) else {
                    continue;
                };
                let (Some(list_computed), Some(list_gt)) = (own_computed, own_gt) else {
                    continue;
                };
                resolve_external_metrics(scroll_pos.y, viewport_computed, viewport_gt, list_computed, list_gt)
            }
        };

        let new_range = calculate_visible_range(
            scroll_top,
            container_height,
            config.item_height,
            config.gap,
            config.total_count,
            config.buffer_count,
        );

        if state.container_height != container_height {
            state.container_height = container_height;
        }
        if state.scroll_top != scroll_top {
            state.scroll_top = scroll_top;
        }
        let range_changed = state.visible_range != new_range;
        if range_changed {
            state.visible_range = new_range.clone();
        }
        if !range_changed && !rows_changed && !config.is_changed() && deficit.is_none() && invalidated.is_none() {
            continue;
        }

        // Desired coverage is not proof that rows exist (initial layout, resize, late creation).
        let rows: Vec<_> = q_children
            .iter_descendants(pooled_list_entity)
            .filter(|entity| q_pooled_list_item.contains(*entity))
            .filter(|entity| {
                q_parents
                    .iter_ancestors(*entity)
                    .find(|parent| q_list_roots.contains(*parent))
                    == Some(pooled_list_entity)
            })
            .collect();
        let bound: HashSet<_> = rows
            .iter()
            .filter_map(|entity| {
                q_pooled_list_item
                    .get(*entity)
                    .ok()
                    .and_then(|(item, _, _)| item.data_index)
            })
            .collect();
        let mut needed = new_range.clone().filter(|index| !bound.contains(index));
        for entity in rows {
            let Ok((mut item, mut node, _)) = q_pooled_list_item.get_mut(entity) else {
                continue;
            };
            if item.data_index.is_some_and(|index| new_range.contains(&index)) {
                if invalidated.is_some() {
                    managed::release_slot_interaction(
                        entity,
                        &q_children,
                        &mut focus,
                        &mut interactions,
                        &mut commands,
                    );
                }
                if config.is_changed() || invalidated.is_some() {
                    let index = item
                        .data_index
                        .expect("the retained slot has an index in the visible range");
                    managed::set_slot_geometry(&mut node, index, &config);
                    commands.entity(entity).insert(NeedRebind { target_index: index });
                }
                continue;
            }
            if item.data_index.is_some() {
                managed::release_slot_interaction(entity, &q_children, &mut focus, &mut interactions, &mut commands);
            }
            if let Some(index) = needed.next() {
                item.data_index = Some(index);
                managed::set_slot_geometry(&mut node, index, &config);
                commands
                    .entity(entity)
                    .try_insert((NeedRebind { target_index: index }, Visibility::Inherited));
            } else if item.data_index.is_some() {
                item.data_index = None;
                node.top = Val::Px(0.0);
                node.display = bevy::ui::Display::None;
                commands
                    .entity(entity)
                    .try_insert(Visibility::Hidden)
                    .remove::<NeedRebind>();
            }
        }
        let missing: Vec<_> = needed.collect();
        if missing.is_empty() {
            if deficit.is_some() {
                commands.entity(pooled_list_entity).remove::<PooledListDeficit>();
            }
        } else if deficit.is_none_or(|current| current.0 != missing) {
            commands.entity(pooled_list_entity).insert(PooledListDeficit(missing));
        }
        if invalidated.is_some() {
            commands.entity(pooled_list_entity).remove::<PooledListInvalidated>();
        }
    }
}

/// 池化列表 Bundle
#[derive(Bundle)]
pub struct PooledListBundle {
    pub config: PooledList,
    pub binding: PooledListBinding,
    pub state: PooledListState,
    pub scroll_position: ScrollPosition,
}

impl PooledListBundle {
    pub fn new(total_count: usize, item_height: f32, gap: f32) -> Self {
        let buffer_count = 5usize;
        Self {
            config: PooledList {
                item_height,
                total_count,
                buffer_count,
                gap,
            },
            binding: PooledListBinding::SelfOwned,
            state: PooledListState::default(),
            scroll_position: ScrollPosition::default(),
        }
    }

    pub fn external(
        total_count: usize,
        item_height: f32,
        gap: f32,
        scroll_owner: Entity,
        viewport_owner: Entity,
    ) -> Self {
        let mut bundle = Self::new(total_count, item_height, gap);
        bundle.binding = PooledListBinding::External {
            scroll_owner,
            viewport_owner,
        };
        bundle
    }
}

/// Low-level slot bundle, primarily useful for isolated content-adapter fixtures.
/// Normal lists allocate these automatically; callers fill `NeedBuild` slots.
pub fn pooled_item_container(data_index: usize, item_height: f32, gap: f32) -> impl Bundle {
    let top = calculate_item_top(data_index, item_height, gap);
    (
        PooledListItem {
            data_index: Some(data_index),
        },
        Name::new("PooledListItem"),
        Visibility::Inherited,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(top),
            height: Val::Px(item_height),
            width: Val::Percent(100.0),
            ..Default::default()
        },
    )
}

pub struct PooledListPlugin;
impl Plugin for PooledListPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<bevy::window::RequestRedraw>()
            .configure_sets(
                Update,
                (PooledListRangeUpdate, PooledListSystems::Build, PooledListSystems::Bind).chain(),
            )
            .add_systems(
                bevy::app::PostUpdate,
                managed::request_layout_followup.after(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                Update,
                (
                    managed::ensure_list_contents,
                    apply_pending_pooled_list_scrolls,
                    pooled_list_range_update_system,
                    managed::grow_slots,
                )
                    .chain()
                    .in_set(PooledListRangeUpdate),
            )
            .add_observer(navigation::on_scroll_pooled_list_to_index)
            .add_observer(navigation::on_cancel_pooled_list_scroll)
            .add_observer(navigation::on_scroll_input)
            .add_observer(navigation::on_pointer_scroll);
    }
}

#[cfg(test)]
mod managed_tests;

#[cfg(test)]
mod tests;
