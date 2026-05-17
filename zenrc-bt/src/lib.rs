//! # zenrc-bt
//!
//! 轻量级行为树（Behavior Tree）库。
//!
//! ## 核心概念
//!
//! - **[`Node`]**：所有行为树节点必须实现的 trait，通过 [`Node::tick`] 驱动状态机。
//! - **[`Status`]**：节点执行结果（`Invalid` / `Running` / `Success` / `Failure`）。
//! - **[`BlackboardPtr`]**：基于 `Arc<RefCell<HashMap>>` 的共享黑板，用于节点间数据交换。
//! - **[`Composite`]**：组合节点 trait，允许子节点的动态增删。
//!
//! ## 示例
//!
//! ```no_run
//! use zenrc_bt::{BlackboardPtr, Node, Status};
//!
//! struct MyAction { status: Status, bb: Option<BlackboardPtr> }
//! impl Node for MyAction {
//!     fn update(&mut self) -> Status { Status::Success }
//!     fn get_blackboard(&self) -> Option<BlackboardPtr> { self.bb.clone() }
//!     fn set_blackboard(&mut self, bb: BlackboardPtr) { self.bb = Some(bb); }
//!     fn get_status(&self) -> Status { self.status }
//!     fn set_status(&mut self, s: Status) { self.status = s; }
//! }
//!
//! let mut action = MyAction { status: Status::Invalid, bb: None };
//! assert_eq!(action.tick(), Status::Success);
//! ```

use std::any::Any;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

// box<dyn Any> 可以存储任何类型的数据
// 通过 downcast_ref::<Type>() 来获取具体类型的引用
/// 共享黑板句柄，封装了 `Arc<RefCell<HashMap<String, Box<dyn Any>>>>`。
///
/// 可在各节点间克隆共享，用于读写异质类型的运行时数据。
#[derive(Clone)]
pub struct BlackboardPtr(Arc<RefCell<HashMap<String, Box<dyn Any>>>>);

impl BlackboardPtr {
    /// 创建一个新的空黑板。
    pub fn new() -> Self {
        BlackboardPtr(Arc::new(RefCell::new(HashMap::new())))
    }
    /// 按 `key` 获取类型为 `T` 的项。
    ///
    /// 若键不存在或类型不匹配，返回 `None`。
    pub fn get<'a, T: 'static>(&'a self, key: &str) -> Option<Ref<'a, T>> {
         Ref::filter_map(self.borrow(), |map| {
            map.get(key)?.downcast_ref::<T>()
        })
        .ok()
    }
}

impl Deref for BlackboardPtr {
    type Target = Arc<RefCell<HashMap<String, Box<dyn Any>>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// 节点执行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Invalid,
    Success,
    Failure,
    Running,
}

/// 行为树节点 Trait
///
/// 所有可以加入行为树的节点必须实现此 trait。
/// 驾驶方式为周期性调用 [`Node::tick`]，内部状态机按如下逻辑流转：
///
/// 1. 若当前状态不是 `Running`，先调用 [`initialize`](Node::initialize)。
/// 2. 调用 [`update`](Node::update) 获取新状态。
/// 3. 若新状态不是 `Running`，调用 [`terminate`](Node::terminate)。
pub trait Node {
    /// 获取黑板
    fn get_blackboard(&self) -> Option<BlackboardPtr>;
    /// 设置黑板
    fn set_blackboard(&mut self, bb: BlackboardPtr);

    /// 每个节点必须实现 update()
    fn update(&mut self) -> Status;

    /// 可以覆盖：初始化
    fn initialize(&mut self) {}
    /// 可以覆盖：结束时调用
    fn terminate(&mut self) {}

    /// 状态机逻辑：tick
    fn tick(&mut self) -> Status {
        let status = self.get_status();
        if status != Status::Running {
            self.initialize();
        }

        let new_status = self.update();
        self.set_status(new_status);

        if new_status != Status::Running {
            self.terminate();
        }

        new_status
    }

    fn is_success(&self) -> bool {
        self.get_status() == Status::Success
    }
    fn is_failure(&self) -> bool {
        self.get_status() == Status::Failure
    }
    fn is_running(&self) -> bool {
        self.get_status() == Status::Running
    }
    fn is_terminated(&self) -> bool {
        self.is_success() || self.is_failure()
    }

    fn reset(&mut self) {
        self.set_status(Status::Invalid);
    }

    // ------ 内部状态管理接口 ------
    fn get_status(&self) -> Status;
    fn set_status(&mut self, s: Status);
}

/// 组合节点 trait，允许动态管理子节点集合。
///
/// 常见实现有：序列节点（Sequence）、选择节点（Selector）、并行节点（Parallel）。
pub trait Composite: Node {
    fn add_child(&mut self, child: Box<dyn Node>);
    fn remove_child(&mut self, index: usize) -> Option<Box<dyn Node>>;
    fn clear_children(&mut self);
    fn get_children(&self) -> &Vec<Box<dyn Node>>;
}

/// 一个可复用的 Node 基础实现
pub struct BaseNode {
    status: Status,
    blackboard: Option<BlackboardPtr>,
}

impl BaseNode {
    pub fn new() -> Self {
        Self {
            status: Status::Invalid,
            blackboard: None,
        }
    }
}

impl Node for BaseNode {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.blackboard.clone()
    }

    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.blackboard = Some(bb);
    }

    fn update(&mut self) -> Status {
        // 默认什么都不做，直接返回成功
        Status::Success
    }

    fn get_status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, s: Status) {
        self.status = s;
    }
}

/// 一个序列节点（依次执行子节点）
pub struct Sequence {
    base: BaseNode,
    children: Vec<Box<dyn Node>>,
    current: usize,
}
impl Sequence {
    pub fn new(children: Vec<Box<dyn Node>>) -> Self {
        Self {
            base: BaseNode::new(),
            children,
            current: 0,
        }
    }
}
impl Node for Sequence {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.base.get_blackboard()
    }
    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.base.set_blackboard(bb.clone());
        for child in self.children.iter_mut() {
            child.set_blackboard(bb.clone());
        }
    }
    fn get_status(&self) -> Status {
        self.base.get_status()
    }
    fn set_status(&mut self, s: Status) {
        self.base.set_status(s);
    }

    fn initialize(&mut self) {
        self.current = 0;
    }

    fn update(&mut self) -> Status {
        while self.current < self.children.len() {
            let status = self.children[self.current].tick();
            match status {
                Status::Running => return Status::Running,
                Status::Failure => return Status::Failure,
                Status::Success => self.current += 1,
                _ => {}
            }
        }
        Status::Success
    }
}

// 构建器trait
impl Composite for Sequence {
    fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }

    fn remove_child(&mut self, index: usize) -> Option<Box<dyn Node>> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }

    fn clear_children(&mut self) {
        self.children.clear();
    }

    fn get_children(&self) -> &Vec<Box<dyn Node>> {
        &self.children
    }
}

// selector 节点（依次尝试子节点，直到一个成功）
pub struct Selector {
    base: BaseNode,
    children: Vec<Box<dyn Node>>,
    current: usize,
}
impl Selector {
    pub fn new(children: Vec<Box<dyn Node>>) -> Self {
        Self {
            base: BaseNode::new(),
            children,
            current: 0,
        }
    }
}
impl Node for Selector {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.base.get_blackboard()
    }
    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.base.set_blackboard(bb.clone());
        for child in self.children.iter_mut() {
            child.set_blackboard(bb.clone());
        }
    }
    fn get_status(&self) -> Status {
        self.base.get_status()
    }
    fn set_status(&mut self, s: Status) {
        self.base.set_status(s);
    }
    fn initialize(&mut self) {
        self.current = 0;
    }
    fn update(&mut self) -> Status {
        while self.current < self.children.len() {
            let status = self.children[self.current].tick();
            match status {
                Status::Running => return Status::Running,
                Status::Success => return Status::Success,
                Status::Failure => self.current += 1,
                _ => {}
            }
        }
        Status::Failure
    }
}
impl Composite for Selector {
    fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }
    fn remove_child(&mut self, index: usize) -> Option<Box<dyn Node>> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }
    fn clear_children(&mut self) {
        self.children.clear();
    }
    fn get_children(&self) -> &Vec<Box<dyn Node>> {
        &self.children
    }
}

// 状态顺序节点 （记住上次执行到哪个子节点，下次从该节点继续）
pub struct StatefulSequence {
    base: BaseNode,
    children: Vec<Box<dyn Node>>,
    current: usize,
}
impl StatefulSequence {
    pub fn new(children: Vec<Box<dyn Node>>) -> Self {
        Self {
            base: BaseNode::new(),
            children,
            current: 0,
        }
    }
}
impl Node for StatefulSequence {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.base.get_blackboard()
    }
    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.base.set_blackboard(bb.clone());
        for child in self.children.iter_mut() {
            child.set_blackboard(bb.clone());
        }
    }
    fn get_status(&self) -> Status {
        self.base.get_status()
    }
    fn set_status(&mut self, s: Status) {
        self.base.set_status(s);
    }
    fn initialize(&mut self) {
        // 不重置 current
    }
    fn update(&mut self) -> Status {
        while self.current < self.children.len() {
            let status = self.children[self.current].tick();
            match status {
                Status::Running => return Status::Running,
                Status::Failure => return Status::Failure,
                Status::Success => self.current += 1,
                _ => {}
            }
        }
        self.current = 0; // 重置 current
        Status::Success
    }
}
impl Composite for StatefulSequence {
    fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }
    fn remove_child(&mut self, index: usize) -> Option<Box<dyn Node>> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }
    fn clear_children(&mut self) {
        self.children.clear();
    }
    fn get_children(&self) -> &Vec<Box<dyn Node>> {
        &self.children
    }
}

// 状态选择节点 （记住上次执行到哪个子节点，下次从该节点继续）
pub struct StatefulSelector {
    base: BaseNode,
    children: Vec<Box<dyn Node>>,
    current: usize,
}
impl StatefulSelector {
    pub fn new(children: Vec<Box<dyn Node>>) -> Self {
        Self {
            base: BaseNode::new(),
            children,
            current: 0,
        }
    }
}
impl Node for StatefulSelector {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.base.get_blackboard()
    }
    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.base.set_blackboard(bb.clone());
        for child in self.children.iter_mut() {
            child.set_blackboard(bb.clone());
        }
    }
    fn get_status(&self) -> Status {
        self.base.get_status()
    }
    fn set_status(&mut self, s: Status) {
        self.base.set_status(s);
    }
    fn initialize(&mut self) {
        // 不重置 current
    }
    fn update(&mut self) -> Status {
        while self.current < self.children.len() {
            let status = self.children[self.current].tick();
            match status {
                Status::Running => return Status::Running,
                Status::Success => return Status::Success,
                Status::Failure => self.current += 1,
                _ => {}
            }
        }
        self.current = 0; // 重置 current
        Status::Failure
    }
}
impl Composite for StatefulSelector {
    fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }
    fn remove_child(&mut self, index: usize) -> Option<Box<dyn Node>> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }
    fn clear_children(&mut self) {
        self.children.clear();
    }
    fn get_children(&self) -> &Vec<Box<dyn Node>> {
        &self.children
    }
}
