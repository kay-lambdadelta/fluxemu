use std::{borrow::Cow, collections::HashMap};

use crate::{
    ComponentPath,
    component::Component,
    scheduler::{
        Period,
        task::{DynTask, Mode, Task},
    },
};

#[derive(Debug)]
pub struct ComponentHandle {
    pub data: Option<MutableData>,
    pub path: ComponentPath,
}

#[derive(Debug)]
pub struct MutableData {
    pub component: Box<dyn Component>,
    pub systems: HashMap<Cow<'static, str>, TaskEntry>,
}

#[derive(Debug)]
pub struct TaskEntry {
    /// The timestamp the task is currently at
    pub(super) current_timestamp: Period,
    /// The timestamp the task requires to be allocated next in order to continue
    pub(super) required_next_allocation: Period,
    /// For checks if requeuing needs to occur
    pub(super) last_requeued_deadline: Period,
    pub(super) mode: Mode,
    pub(super) task: Box<dyn DynTask>,
}

impl TaskEntry {
    pub fn new(mode: Mode, task: impl Task) -> Self {
        Self {
            current_timestamp: Period::ZERO,
            required_next_allocation: Period::ZERO,
            last_requeued_deadline: Period::ZERO,
            mode,
            task: Box::new(task),
        }
    }
}
