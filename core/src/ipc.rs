/// 进程间通信 — 基于 UDP 的轻量级命令通道
///
/// ctl → pet 单向命令传输
/// 使用 localhost UDP 端口，避免进程层级依赖

use std::net::UdpSocket;

use crate::bridge::PetCommand;

const DEFAULT_PORT: u16 = 19198;

pub struct IpcSender {
    socket: UdpSocket,
    addr: String,
}

pub struct IpcReceiver {
    socket: UdpSocket,
}

impl IpcSender {
    pub fn new(port: u16) -> Result<Self, String> {
        let addr = format!("127.0.0.1:{port}");
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("创建发送端 socket 失败: {e}"))?;
        Ok(Self { socket, addr })
    }

    pub fn send(&self, cmd: &PetCommand) -> Result<(), String> {
        let data = cmd.to_json_line();
        self.socket
            .send_to(data.as_bytes(), &self.addr)
            .map_err(|e| format!("IPC 发送失败: {e}"))?;
        Ok(())
    }
}

impl IpcReceiver {
    pub fn new(port: u16) -> Result<Self, String> {
        let addr = format!("127.0.0.1:{port}");
        let socket = UdpSocket::bind(&addr)
            .map_err(|e| format!("绑定接收端口失败 (端口可能被占用): {e}"))?;
        socket.set_nonblocking(true)
            .map_err(|e| format!("设置非阻塞失败: {e}"))?;
        Ok(Self { socket })
    }

    /// 非阻塞接收，返回 None 表示无数据
    pub fn try_recv(&self) -> Option<PetCommand> {
        let mut buf = [0u8; 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                let text = String::from_utf8_lossy(&buf[..len]);
                PetCommand::from_json_line(&text)
            }
            Err(_) => None,
        }
    }
}

pub fn default_port() -> u16 {
    DEFAULT_PORT
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_and_receive() {
        let port = 19199; // 测试用不同端口避免冲突
        let receiver = IpcReceiver::new(port).expect("创建接收端失败");
        let sender = IpcSender::new(port).expect("创建发送端失败");

        let cmd = PetCommand::SetState { state: crate::bridge::PetStateName::Happy };
        sender.send(&cmd).expect("发送失败");

        // 给一点时间让数据到达
        std::thread::sleep(std::time::Duration::from_millis(10));

        let received = receiver.try_recv();
        assert!(received.is_some(), "应该收到命令");
        matches!(received.unwrap(), PetCommand::SetState { state: crate::bridge::PetStateName::Happy });
    }

    #[test]
    fn test_no_data_returns_none() {
        let receiver = IpcReceiver::new(19200).expect("创建接收端失败");
        assert!(receiver.try_recv().is_none());
    }

    #[test]
    fn test_multiple_commands() {
        let port = 19201;
        let receiver = IpcReceiver::new(port).unwrap();
        let sender = IpcSender::new(port).unwrap();

        sender.send(&PetCommand::ShowBubble { text: "hello".into() }).unwrap();
        sender.send(&PetCommand::SetState { state: crate::bridge::PetStateName::Sleep }).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));

        let c1 = receiver.try_recv();
        let c2 = receiver.try_recv();
        assert!(c1.is_some());
        assert!(c2.is_some());
    }

    #[test]
    fn test_default_port_value() {
        assert_eq!(default_port(), 19198);
    }
}
