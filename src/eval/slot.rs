//! A resolved place can borrow a frame slot or a guarded pool payload.
//! No guard survives a call back into the evaluator or scheduler.

use std::ops::{Deref, DerefMut};
use std::sync::MutexGuard;

use super::place::Proj;
use super::region::Store;
use super::value::{HandleValue, Slot, SlotState, Value};
use crate::diag::Span;

pub(super) enum SlotMut<'a> {
    Local(&'a mut Slot),
    Pool {
        store: MutexGuard<'a, Store>,
        handle: HandleValue,
        tail: Vec<Proj>,
    },
}

pub(super) fn project<'a>(slot: &'a Slot, step: &Proj) -> Option<&'a Slot> {
    match (&slot.value, step) {
        (Value::Struct { fields, .. }, Proj::Field(name)) => {
            fields.iter().find(|(n, _)| n == name).map(|(_, s)| s)
        }
        (Value::Tuple(items), Proj::Index(i)) => items.get(usize::try_from(*i).ok()?),
        (Value::List(items, _, _), Proj::Index(i)) => items.get(usize::try_from(*i).ok()?),
        (Value::Map(pairs, _), Proj::Key(key)) => {
            pairs.iter().find(|(k, _)| key.names(k)).map(|(_, s)| s)
        }
        _ => None,
    }
}

pub(super) fn project_mut<'a>(slot: &'a mut Slot, step: &Proj) -> Option<&'a mut Slot> {
    match (&mut slot.value, step) {
        (Value::Struct { fields, .. }, Proj::Field(name)) => {
            fields.iter_mut().find(|(n, _)| n == name).map(|(_, s)| s)
        }
        (Value::Tuple(items), Proj::Index(i)) => items.get_mut(usize::try_from(*i).ok()?),
        (Value::List(items, _, _), Proj::Index(i)) => {
            std::sync::Arc::make_mut(items).get_mut(usize::try_from(*i).ok()?)
        }
        (Value::Map(pairs, _), Proj::Key(key)) => {
            pairs.iter_mut().find(|(k, _)| key.names(k)).map(|(_, s)| s)
        }
        _ => None,
    }
}

pub(super) struct PoolAddress {
    pub handle: HandleValue,
    pub tail: Vec<Proj>,
    pub moved: Option<(Span, usize)>,
}

/// Resolve nested pool hops by identity while retaining the earliest move.
/// The final guard needs only the last hop and ordinary projections below it.
pub(super) fn pool_address(
    store: &Store,
    mut handle: HandleValue,
    tail: &[Proj],
    mut moved: Option<(Span, usize)>,
    depth: usize,
) -> Option<PoolAddress> {
    let mut slot = &store
        .slot(handle.pool, handle.index, handle.generation)?
        .payload;
    let mut suffix = Vec::new();
    if moved.is_none()
        && let SlotState::Moved(at) = slot.state
    {
        moved = Some((at, depth));
    }
    for (i, step) in tail.iter().enumerate() {
        slot = if let (Value::PoolRef(pool), Proj::Pool(next)) = (&slot.value, step) {
            if *pool != next.pool {
                return None;
            }
            handle = *next;
            suffix.clear();
            &store
                .slot(handle.pool, handle.index, handle.generation)?
                .payload
        } else {
            suffix.push(step.clone());
            project(slot, step)?
        };
        if moved.is_none()
            && let SlotState::Moved(at) = slot.state
        {
            moved = Some((at, depth + i + 1));
        }
    }
    Some(PoolAddress {
        handle,
        tail: suffix,
        moved,
    })
}

impl Deref for SlotMut<'_> {
    type Target = Slot;
    fn deref(&self) -> &Slot {
        match self {
            Self::Local(slot) => slot,
            Self::Pool {
                store,
                handle,
                tail,
            } => {
                let mut slot = &store
                    .slot(handle.pool, handle.index, handle.generation)
                    .expect("guarded handle")
                    .payload;
                for step in tail {
                    slot = project(slot, step).expect("guarded projection");
                }
                slot
            }
        }
    }
}
impl DerefMut for SlotMut<'_> {
    fn deref_mut(&mut self) -> &mut Slot {
        match self {
            Self::Local(slot) => slot,
            Self::Pool {
                store,
                handle,
                tail,
            } => {
                let mut slot = &mut store
                    .slot_mut(handle.pool, handle.index, handle.generation)
                    .expect("guarded handle")
                    .payload;
                for step in tail {
                    slot = project_mut(slot, step).expect("guarded projection");
                }
                slot
            }
        }
    }
}
