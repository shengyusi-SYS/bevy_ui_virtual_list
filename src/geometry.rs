use crate::{PooledList, PooledListBinding};
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};
use std::ops::Range;

/// 计算可见范围
pub(crate) fn calculate_visible_range(
    scroll_position: f32,
    container_height: f32,
    item_height: f32,
    gap: f32,
    total_count: usize,
    buffer_count: usize,
) -> Range<usize> {
    if total_count == 0
        || container_height <= 0.0
        || !item_height.is_finite()
        || item_height <= 0.0
        || !gap.is_finite()
        || gap < 0.0
    {
        return 0..0;
    }
    let item_with_gap = item_height + gap;
    let first_visible = (scroll_position / item_with_gap).floor() as usize;
    let first_visible = first_visible.saturating_sub(buffer_count).min(total_count);
    let visible_count = (container_height / item_with_gap).ceil() as usize + 1;
    let last_visible = (first_visible + visible_count + buffer_count * 2).min(total_count);
    first_visible..last_visible
}

/// Resolve the logical offset rendered by Bevy without changing the gesture/snap owner.
/// Layout clamps its physical offset; consumers must not use an out-of-range requested offset.
pub fn effective_scroll_position(position: &ScrollPosition, computed: &ComputedNode) -> bevy::math::Vec2 {
    let inverse_scale = computed.inverse_scale_factor();
    let max = (computed.content_size() - computed.size() + computed.scrollbar_size).max(bevy::math::Vec2::ZERO);
    (position.0 / inverse_scale).clamp(bevy::math::Vec2::ZERO, max).floor() * inverse_scale
}

/// Whether the list's computed extent reflects its configured content height.
/// Pass the list entity's [`ComputedNode`]; self-owned lists use its content size,
/// while external lists use its node size, with one physical pixel of tolerance.
/// This checks only extent; callers must still ensure data and a visible viewport are ready.
pub fn list_extent_is_ready(config: &PooledList, binding: &PooledListBinding, computed: &ComputedNode) -> bool {
    let expected_height = calculate_content_height(config.total_count, config.item_height, config.gap);
    let pixel = computed.inverse_scale_factor();
    match binding {
        PooledListBinding::SelfOwned => computed.content_size().y * pixel + pixel >= expected_height,
        PooledListBinding::External { .. } => (computed.size().y * pixel - expected_height).abs() <= pixel,
    }
}

pub(crate) fn resolve_external_metrics(
    scroll_top: f32,
    viewport_computed: &ComputedNode,
    viewport_gt: &UiGlobalTransform,
    list_computed: &ComputedNode,
    list_gt: &UiGlobalTransform,
) -> (f32, f32) {
    let scroll_top = effective_scroll_position(
        &ScrollPosition(bevy::math::Vec2::new(0.0, scroll_top)),
        viewport_computed,
    )
    .y;
    let viewport_isf = viewport_computed.inverse_scale_factor();
    let content_inset = viewport_computed.content_inset();
    let viewport_height =
        (viewport_computed.size().y - content_inset.min_inset.y - content_inset.max_inset.y) * viewport_isf;
    let viewport_content_start =
        viewport_gt.translation.y - viewport_computed.size().y * 0.5 + content_inset.min_inset.y;
    let layout_scroll = viewport_computed.scroll_position.y;

    let list_isf = list_computed.inverse_scale_factor();
    let list_height = list_computed.size().y * list_isf;
    let list_start = list_gt.translation.y - list_computed.size().y * 0.5;
    let list_start_in_content = (list_start - viewport_content_start + layout_scroll) * viewport_isf;
    let list_end_in_content = list_start_in_content + list_height;

    let relative_scroll_top = (scroll_top - list_start_in_content).max(0.0);
    let visible_start = scroll_top.max(list_start_in_content);
    let visible_end = (scroll_top + viewport_height).min(list_end_in_content);
    let visible_height = (visible_end - visible_start).max(0.0);

    (relative_scroll_top, visible_height)
}

/// 计算内容区域的总高度（用于设置滚动容器内容大小）
pub fn calculate_content_height(total_count: usize, item_height: f32, gap: f32) -> f32 {
    if total_count == 0 {
        return 0.0;
    }
    (item_height + gap) * total_count as f32 - gap
}

/// 计算列表项的顶部位置
pub fn calculate_item_top(data_index: usize, item_height: f32, gap: f32) -> f32 {
    (item_height + gap) * data_index as f32
}

fn calculate_max_scroll(content_height: f32, viewport_height: f32) -> f32 {
    (content_height - viewport_height).max(0.0)
}

pub(crate) fn clamped_pending_scroll_target(
    config: &PooledList,
    viewport_height: f32,
    requested_index: usize,
) -> Option<f32> {
    if viewport_height <= 0.0 {
        return None;
    }

    if config.total_count == 0 {
        return Some(0.0);
    }

    let max_index = config.total_count.saturating_sub(1);
    let target_index = requested_index.min(max_index);
    let target_top = calculate_item_top(target_index, config.item_height, config.gap);
    let content_height = calculate_content_height(config.total_count, config.item_height, config.gap);
    let max_scroll = calculate_max_scroll(content_height, viewport_height);

    Some(target_top.clamp(0.0, max_scroll))
}
