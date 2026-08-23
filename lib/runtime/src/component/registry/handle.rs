use std::{borrow::Cow, collections::HashMap};

use crate::{
    ComponentPath,
    component::Component,
    scheduler::{
        Period,
        task::{DynTask, Mode},
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
    pub current_timestamp: Period,
    pub mode: Mode,
    // Make sure the task is bootstrapped
    pub started: bool,
    pub task: Box<dyn DynTask>,
}
