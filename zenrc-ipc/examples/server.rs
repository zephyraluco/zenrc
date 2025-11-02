use std::os::unix::net::UnixDatagram;
use std::{fs, io};

// 定义两个 socket 文件的路径，必须与接收者一致
const RECEIVER_PATH: &str = "/tmp/uds_dgram_receiver.sock";
const SENDER_PATH: &str = "/tmp/uds_dgram_sender.sock";

fn main() -> io::Result<()> {
    // 1. 清理发送者的 socket 文件
    if fs::metadata(SENDER_PATH).is_ok() {
        fs::remove_file(SENDER_PATH)?;
    }

    println!("🚀 发送者正在启动...");

    // 2. 绑定发送者 socket (允许它接收回复)
    let socket = UnixDatagram::bind(SENDER_PATH)?;
    println!("✅ 发送者已绑定到：{}", SENDER_PATH);

    let mut message: i32 = 0;
    loop {
        // 3. 发送数据到接收者
        message += 1;
        let message_bytes = message.to_be_bytes();
        let bytes_sent = socket.send_to(&message_bytes, RECEIVER_PATH)?;
    }
    // let mut buffer = [0; 128];
    // let bytes_read = socket.recv(&mut buffer)?;
    // let response = String::from_utf8_lossy(&buffer[..bytes_read]);

    // 5. 程序退出时清理 socket 文件
    fs::remove_file(SENDER_PATH)?;
    println!("\n🚪 发送者关闭，已删除 socket 文件。");

    Ok(())
}
