use bevy::{
    picking::events::{Pointer, Scroll},
    prelude::*,
};

use crate::{PooledList, PooledListBinding, calculate_item_top, clamped_pending_scroll_target, list_extent_is_ready};

/// Align the indexed row with the viewport start, clamped to the content extent.
/// The newest request replaces any pending request. Unlaid-out lists wait.
#[derive(Event, Clone, Copy, Debug)]
pub struct ScrollPooledListToIndex {
    pub list_entity: Entity,
    pub index: usize,
}

/// Cancel a list's pending index request without changing its current offset.
#[derive(Event, Clone, Copy, Debug)]
pub struct CancelPooledListScroll {
    pub list_entity: Entity,
}

/// Notify real input intent on a scroll owner, including input at a boundary.
/// Native `Pointer<Scroll>` is handled automatically. Custom touch/scrollbar
/// adapters should emit this event before applying their input.
#[derive(Event, Clone, Copy, Debug)]
pub struct PooledListScrollInput {
    pub scroll_owner: Entity,
}

/// Resolved index destination, targeted at the scroll owner. By default the
/// library writes `offset` to its native `ScrollPosition.y` immediately.
/// A motion adapter may call [`Self::take_over`] and apply the destination itself.
#[derive(EntityEvent, Debug)]
pub struct PooledListScrollTarget {
    pub entity: Entity,
    pub list_entity: Entity,
    pub offset: f32,
    handled: bool,
}

impl PooledListScrollTarget {
    /// Suppress the default immediate write. Exactly one adapter should own motion.
    pub fn take_over(&mut self) {
        self.handled = true;
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PendingPooledListScroll {
    pub index: usize,
}

pub(crate) fn on_scroll_pooled_list_to_index(
    event: On<ScrollPooledListToIndex>,
    lists: Query<(), With<PooledList>>,
    mut commands: Commands,
) {
    if lists.contains(event.list_entity) {
        commands
            .entity(event.list_entity)
            .insert(PendingPooledListScroll { index: event.index });
    }
}

pub(crate) fn on_cancel_pooled_list_scroll(event: On<CancelPooledListScroll>, mut commands: Commands) {
    commands
        .entity(event.list_entity)
        .try_remove::<PendingPooledListScroll>();
}

fn scroll_owner(list: Entity, binding: &PooledListBinding) -> Entity {
    match binding {
        PooledListBinding::SelfOwned => list,
        PooledListBinding::External { scroll_owner, .. } => *scroll_owner,
    }
}

pub(crate) fn on_scroll_input(
    event: On<PooledListScrollInput>,
    pending: Query<(Entity, &PooledListBinding), With<PendingPooledListScroll>>,
    mut commands: Commands,
) {
    cancel_owner(event.scroll_owner, &pending, &mut commands);
}

fn cancel_owner(
    owner: Entity,
    pending: &Query<(Entity, &PooledListBinding), With<PendingPooledListScroll>>,
    commands: &mut Commands,
) {
    for (list, binding) in pending {
        if scroll_owner(list, binding) == owner {
            commands.entity(list).remove::<PendingPooledListScroll>();
        }
    }
}

pub(crate) fn on_pointer_scroll(
    event: On<Pointer<Scroll>>,
    pending: Query<(Entity, &PooledListBinding), With<PendingPooledListScroll>>,
    mut commands: Commands,
) {
    if event.x != 0.0 || event.y != 0.0 {
        cancel_owner(event.entity, &pending, &mut commands);
    }
}

pub(crate) fn apply_pending_pooled_list_scrolls(
    lists: Query<(Entity, &PooledList, &PooledListBinding, &PendingPooledListScroll)>,
    positions: Query<&ScrollPosition>,
    computed: Query<&ComputedNode>,
    transforms: Query<&UiGlobalTransform>,
    entities: Query<Entity>,
    mut commands: Commands,
) {
    for (list, config, binding, pending) in &lists {
        let owner = scroll_owner(list, binding);
        let viewport = match binding {
            PooledListBinding::SelfOwned => list,
            PooledListBinding::External { viewport_owner, .. } => *viewport_owner,
        };
        if config.total_count == 0 || !entities.contains(owner) || !entities.contains(viewport) {
            commands.entity(list).remove::<PendingPooledListScroll>();
            continue;
        }
        let Some((owner, offset)) =
            resolve_target(list, config, binding, pending.index, &positions, &computed, &transforms)
        else {
            continue;
        };
        commands.entity(list).remove::<PendingPooledListScroll>();
        commands.queue(move |world: &mut World| {
            let mut target = PooledListScrollTarget {
                entity: owner,
                list_entity: list,
                offset,
                handled: false,
            };
            world.trigger_ref(&mut target);
            if !target.handled
                && let Some(mut position) = world.get_mut::<ScrollPosition>(owner)
            {
                position.y = offset;
            }
        });
    }
}

/// Resolve only once the current extent has usable layout geometry.
pub(crate) fn resolve_target(
    list: Entity,
    config: &PooledList,
    binding: &PooledListBinding,
    index: usize,
    positions: &Query<&ScrollPosition>,
    computed: &Query<&ComputedNode>,
    transforms: &Query<&UiGlobalTransform>,
) -> Option<(Entity, f32)> {
    if config.total_count == 0 {
        return None;
    }
    let owner = scroll_owner(list, binding);
    let viewport = match binding {
        PooledListBinding::SelfOwned => list,
        PooledListBinding::External { viewport_owner, .. } => *viewport_owner,
    };
    let view = computed.get(viewport).ok()?;
    positions.get(owner).ok()?;
    let height = (view.size().y - view.scrollbar_size.y) * view.inverse_scale_factor();
    if height <= 0.0 {
        return None;
    }
    let offset = match binding {
        PooledListBinding::SelfOwned => {
            // A new content Node extent reaches native layout after Update.
            // Keep the index request until the motion owner can use that range,
            // otherwise its old max would permanently clamp the resolved target.
            if !list_extent_is_ready(config, binding, view) {
                return None;
            }
            let Some(target) = clamped_pending_scroll_target(config, height, index) else {
                return None;
            };
            target
        }
        PooledListBinding::External { .. } => {
            let (Ok(list_node), Ok(list_gt), Ok(view_gt)) =
                (computed.get(list), transforms.get(list), transforms.get(viewport))
            else {
                return None;
            };
            if list_node.size().y <= 0.0 {
                return None;
            }
            if !list_extent_is_ready(config, binding, list_node) {
                // The managed extent changed in Update; wait for its layout
                // before using the enclosing scroll area's maximum offset.
                return None;
            }
            let inset = view.content_inset().min_inset.y;
            let viewport_start = view_gt.translation.y - view.size().y * 0.5 + inset;
            let list_start = list_gt.translation.y - list_node.size().y * 0.5;
            let start = (list_start - viewport_start + view.scroll_position.y) * view.inverse_scale_factor();
            let index = index.min(config.total_count - 1);
            let target = start + calculate_item_top(index, config.item_height, config.gap);
            let max = ((view.content_size().y - view.size().y + view.scrollbar_size.y) * view.inverse_scale_factor())
                .max(0.0);
            target.clamp(0.0, max)
        }
    };
    Some((owner, offset))
}
