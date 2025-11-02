use std::os::unix::net::UnixDatagram;
use std::{fs, io};

// 定义两个 socket 文件的路径
const RECEIVER_PATH: &str = "/tmp/uds_dgram_receiver.sock";
const SENDER_PATH: &str = "/tmp/uds_dgram_sender.sock";

fn main() -> io::Result<()> {
    // 1. 清理：如果 socket 文件已存在，先删除它
    if fs::metadata(RECEIVER_PATH).is_ok() {
        fs::remove_file(RECEIVER_PATH)?;
    }

    println!("🌍 接收者正在启动...");

    // 2. 绑定到 Unix Datagram Socket
    let socket = UnixDatagram::bind(RECEIVER_PATH)?;
    println!("✅ 接收者已绑定到：{}", RECEIVER_PATH);

    // 3. 接收数据
    let mut buffer = [0u8; 4];
    println!("👂 正在等待消息...");

    loop {
        // recv_from 返回接收到的字节数和发送者的地址
        let (bytes_read, sender_addr) = socket.recv_from(&mut buffer)?;
        let received_data = i32::from_be_bytes(buffer);
        println!("\n📥 接收到 ({}):{}", bytes_read, received_data);
    }

    // let response = "收到！这是接收者的回复。";
    // let bytes_sent = socket.send_to(response.as_bytes(), SENDER_PATH)?;
    // println!("👍 已发送回复 ({} 字节) 到 {}", bytes_sent, SENDER_PATH);

    // 5. 程序退出时清理 socket 文件
    fs::remove_file(RECEIVER_PATH)?;
    println!("\n🚪 接收者关闭，已删除 socket 文件。");

    Ok(())
}
