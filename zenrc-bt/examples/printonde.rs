use zenrc_bt::*;

/// 一个简单的打印节点
struct PrintNode {
    base: BaseNode,
    msg: String,
}

impl PrintNode {
    fn new(msg: &str) -> Self {
        Self {
            base: BaseNode::new(),
            msg: msg.to_string(),
        }
    }
}

impl Node for PrintNode {
    fn get_blackboard(&self) -> Option<BlackboardPtr> {
        self.base.get_blackboard()
    }
    fn set_blackboard(&mut self, bb: BlackboardPtr) {
        self.base.set_blackboard(bb);
    }
    fn get_status(&self) -> Status {
        self.base.get_status()
    }
    fn set_status(&mut self, s: Status) {
        self.base.set_status(s);
    }

    fn update(&mut self) -> Status {
        println!("PrintNode says: {}", self.msg);
        self.get_blackboard().unwrap().borrow_mut().insert("last_message".to_string(), Box::new("sdsdsd".to_string()));
        self.get_blackboard().unwrap().borrow_mut().insert("tow_message".to_string(), Box::new("zxczxc"));
        Status::Success
    }
}

fn main() {
    // 创建一个黑板
    let bb: BlackboardPtr = BlackboardPtr::new();

    // 创建行为树：Sequence( Print("Hello"), Print("World") )
    let mut root = Sequence::new(Vec::new());
    root.add_child(Box::new(PrintNode::new("Hello")));
    root.add_child(Box::new(PrintNode::new("World")));
    root.set_blackboard(bb);
    // 执行 tick
    let status = root.tick();
    println!("bb = {:?}", root.get_blackboard().unwrap().get::<String>("last_message").unwrap());
    println!("bb = {:?}", root.get_blackboard().unwrap().get::<&str>("tow_message").unwrap());
    println!("Root status = {:?}", status);
}