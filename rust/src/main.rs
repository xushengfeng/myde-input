mod device_info;
mod protocol;
mod reader;
mod scanner;

use std::io::{self, Read, Write};

use protocol::*;
use reader::EventReader;

fn main() {
    // 设置 stdout 为二进制模式（MessagePack）
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    // 创建事件读取器
    let mut event_reader = match EventReader::new() {
        Ok(r) => r,
        Err(e) => {
            let msg = RustMessage::Error {
                code: "INIT_FAILED".to_string(),
                message: e,
                detail: None,
            };
            send_message(&mut stdout, &msg);
            return;
        }
    };

    // 初始扫描设备
    let devices = scanner::scan_devices();
    let device_infos: Vec<DeviceFullInfo> = devices.into_iter().map(|(info, _)| info).collect();
    let msg = RustMessage::DeviceList {
        devices: device_infos,
    };
    send_message(&mut stdout, &msg);

    // 主循环：读取命令 + 读取事件
    let mut stdin_buf = vec![0u8; 64 * 1024];
    let mut stdin_pos = 0;

    loop {
        // 尝试从 stdin 读取命令
        let cmd = read_command(&stdin, &mut stdin_buf, &mut stdin_pos);
        if let Some(cmd) = cmd {
            match cmd {
                TsCommand::ListDevices => {
                    let devices = scanner::scan_devices();
                    let device_infos: Vec<DeviceFullInfo> =
                        devices.into_iter().map(|(info, _)| info).collect();
                    let msg = RustMessage::DeviceList {
                        devices: device_infos,
                    };
                    send_message(&mut stdout, &msg);
                }
                TsCommand::StartReading { paths } => {
                    let mut errors = Vec::new();
                    for path in &paths {
                        if let Err(e) = event_reader.add_device(path) {
                            errors.push(e);
                        }
                    }
                    if errors.is_empty() {
                        send_message(&mut stdout, &RustMessage::Ok);
                    } else {
                        send_message(
                            &mut stdout,
                            &RustMessage::Error {
                                code: "START_READING_FAILED".to_string(),
                                message: errors.join("; "),
                                detail: None,
                            },
                        );
                    }
                }
                TsCommand::StopReading { paths } => {
                    let mut errors = Vec::new();
                    for path in &paths {
                        if let Err(e) = event_reader.remove_device(path) {
                            errors.push(e);
                        }
                    }
                    if errors.is_empty() {
                        send_message(&mut stdout, &RustMessage::Ok);
                    } else {
                        send_message(
                            &mut stdout,
                            &RustMessage::Error {
                                code: "STOP_READING_FAILED".to_string(),
                                message: errors.join("; "),
                                detail: None,
                            },
                        );
                    }
                }
                TsCommand::Exit => {
                    break;
                }
            }
        }

        // 读取输入事件（非阻塞，超时 10ms）
        match event_reader.wait_event(10) {
            Ok(Some(event)) => {
                let msg = RustMessage::InputEvent {
                    path: event.path,
                    event_type: event.event_type,
                    code: event.code,
                    value: event.value,
                    timestamp_sec: event.timestamp_sec,
                    timestamp_usec: event.timestamp_usec,
                };
                send_message(&mut stdout, &msg);
            }
            Ok(None) => {
                // 没有事件，继续
            }
            Err(e) => {
                send_message(
                    &mut stdout,
                    &RustMessage::Error {
                        code: "EVENT_READ_ERROR".to_string(),
                        message: e,
                        detail: None,
                    },
                );
            }
        }
    }

    event_reader.close();
}

/// 读取命令（非阻塞）
fn read_command(stdin: &io::Stdin, buf: &mut Vec<u8>, pos: &mut usize) -> Option<TsCommand> {
    // 尝试读取 stdin
    let mut stdin = stdin.lock();
    let mut tmp = [0u8; 4096];

    match stdin.read(&mut tmp) {
        Ok(n) if n > 0 => {
            // 追加到缓冲区
            if *pos + n > buf.len() {
                buf.resize(*pos + n, 0);
            }
            buf[*pos..*pos + n].copy_from_slice(&tmp[..n]);
            *pos += n;
        }
        _ => {
            // 没有数据或错误
        }
    }

    // 尝试解析一个完整的命令
    // MessagePack 消息没有固定长度，我们需要尝试解析
    if *pos > 0 {
        match TsCommand::from_msgpack(&buf[..*pos]) {
            Ok(cmd) => {
                *pos = 0; // 重置缓冲区
                Some(cmd)
            }
            Err(_) => None, // 数据不完整，等待更多数据
        }
    } else {
        None
    }
}

/// 发送消息到 stdout
fn send_message(stdout: &mut io::StdoutLock, msg: &RustMessage) {
    match msg.to_msgpack() {
        Ok(data) => {
            // 写入长度前缀 + 数据
            let len = (data.len() as u32).to_le_bytes();
            let _ = stdout.write_all(&len);
            let _ = stdout.write_all(&data);
            let _ = stdout.flush();
        }
        Err(e) => {
            // 序列化失败，发送错误消息
            let err_msg = RustMessage::Error {
                code: "ENCODE_ERROR".to_string(),
                message: e,
                detail: None,
            };
            if let Ok(err_data) = err_msg.to_msgpack() {
                let len = (err_data.len() as u32).to_le_bytes();
                let _ = stdout.write_all(&len);
                let _ = stdout.write_all(&err_data);
                let _ = stdout.flush();
            }
        }
    }
}
