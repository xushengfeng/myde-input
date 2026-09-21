use std::collections::HashMap;
use std::fs::File;
use std::os::unix::io::{AsRawFd, RawFd};

use nix::sys::epoll::{Epoll, EpollEvent, EpollFlags};

/// 输入事件（与 Linux input_event 结构对应）
#[derive(Debug, Clone)]
pub struct RawInputEvent {
    pub path: String,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
    pub timestamp_sec: i64,
    pub timestamp_usec: i64,
}

/// 多设备事件读取器
pub struct EventReader {
    epoll: Epoll,
    fd_to_path: HashMap<RawFd, String>,
    fd_to_file: HashMap<RawFd, File>,
}

impl EventReader {
    /// 创建新的事件读取器
    pub fn new() -> Result<Self, String> {
        let epoll =
            Epoll::new(nix::sys::epoll::EpollCreateFlags::empty()).map_err(|e| format!("创建 epoll 失败: {}", e))?;

        Ok(EventReader {
            epoll,
            fd_to_path: HashMap::new(),
            fd_to_file: HashMap::new(),
        })
    }

    /// 添加设备到监听列表
    pub fn add_device(&mut self, path: &str) -> Result<(), String> {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_RDONLY | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("打开设备 {} 失败: {}", path, e))?;

        let fd = file.as_raw_fd();

        // 注册到 epoll
        let event = EpollEvent::new(EpollFlags::EPOLLIN, fd as u64);
        self.epoll
            .add(&file, event)
            .map_err(|e| format!("epoll_ctl 添加失败: {}", e))?;

        self.fd_to_path.insert(fd, path.to_string());
        self.fd_to_file.insert(fd, file);

        Ok(())
    }

    /// 移除设备
    pub fn remove_device(&mut self, path: &str) -> Result<(), String> {
        let fd = self
            .fd_to_path
            .iter()
            .find(|(_, p)| *p == path)
            .map(|(&fd, _)| fd);

        if let Some(fd) = fd {
            // 从 epoll 移除
            if let Some(file) = self.fd_to_file.get(&fd) {
                let _ = self.epoll.delete(file);
            }

            self.fd_to_path.remove(&fd);
            self.fd_to_file.remove(&fd);
        }

        Ok(())
    }

    /// 等待下一个事件（阻塞，带超时）
    pub fn wait_event(&self, timeout_ms: i32) -> Result<Option<RawInputEvent>, String> {
        let mut events = [EpollEvent::empty(); 1];

        // timeout: -1 = 无限等待, 0 = 立即返回, >0 = 等待毫秒数
        let timeout: isize = if timeout_ms < 0 { -1 } else { timeout_ms as isize };

        let nfds = self
            .epoll
            .wait(&mut events, timeout)
            .map_err(|e| format!("epoll_wait 失败: {}", e))?;

        if nfds == 0 {
            return Ok(None);
        }

        let fd = events[0].data() as RawFd;
        let path = self
            .fd_to_path
            .get(&fd)
            .ok_or_else(|| format!("未知 fd: {}", fd))?
            .clone();

        // 读取事件
        self.read_event(fd, &path)
    }

    /// 从文件描述符读取事件
    fn read_event(&self, fd: RawFd, path: &str) -> Result<Option<RawInputEvent>, String> {
        // input_event 结构体: timeval(16 bytes) + type(2) + code(2) + value(4) = 24 bytes
        let mut buf = [0u8; 24];

        let ret = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, 24) };

        if ret < 0 {
            let errno = nix::errno::Errno::last();
            if errno == nix::errno::Errno::EAGAIN || errno == nix::errno::Errno::EWOULDBLOCK {
                return Ok(None);
            }
            return Err(format!("读取事件失败: {}", errno));
        }

        if ret < 24 {
            return Err(format!("读取事件不完整: {} bytes", ret));
        }

        // 解析 input_event 结构
        let sec = i64::from_ne_bytes(buf[0..8].try_into().unwrap());
        let usec = i64::from_ne_bytes(buf[8..16].try_into().unwrap());
        let event_type = u16::from_ne_bytes(buf[16..18].try_into().unwrap());
        let code = u16::from_ne_bytes(buf[18..20].try_into().unwrap());
        let value = i32::from_ne_bytes(buf[20..24].try_into().unwrap());

        Ok(Some(RawInputEvent {
            path: path.to_string(),
            event_type,
            code,
            value,
            timestamp_sec: sec,
            timestamp_usec: usec,
        }))
    }

    /// 获取当前监听的设备列表
    pub fn get_devices(&self) -> Vec<String> {
        self.fd_to_path.values().cloned().collect()
    }

    /// 关闭所有设备
    pub fn close(&mut self) {
        self.fd_to_file.clear();
        self.fd_to_path.clear();
        // Epoll 会在 Drop 时自动关闭
    }
}
