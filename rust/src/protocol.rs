use serde::{Deserialize, Serialize};

/// Rust -> TypeScript 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RustMessage {
    /// 设备列表响应
    #[serde(rename = "device_list")]
    DeviceList { devices: Vec<DeviceFullInfo> },

    /// 设备添加（热插拔）
    #[serde(rename = "device_added")]
    DeviceAdded { device: DeviceFullInfo },

    /// 设备移除
    #[serde(rename = "device_removed")]
    DeviceRemoved { path: String },

    /// 输入事件
    #[serde(rename = "input_event")]
    InputEvent {
        path: String,
        event_type: u16,
        code: u16,
        value: i32,
        timestamp_sec: i64,
        timestamp_usec: i64,
    },

    /// 错误（不是 panic，是返回）
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
        detail: Option<String>,
    },

    /// 成功响应（无数据）
    #[serde(rename = "ok")]
    Ok,
}

/// TypeScript -> Rust 命令
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TsCommand {
    /// 列出所有设备
    #[serde(rename = "list_devices")]
    ListDevices,

    /// 开始读取指定设备
    #[serde(rename = "start_reading")]
    StartReading { paths: Vec<String> },

    /// 停止读取指定设备
    #[serde(rename = "stop_reading")]
    StopReading { paths: Vec<String> },

    /// 退出
    #[serde(rename = "exit")]
    Exit,
}

/// 设备完整信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFullInfo {
    pub path: String,
    pub name: String,
    pub device_type: String,
    pub phys: Option<String>,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub capabilities: DeviceCapabilities,
    pub touch_info: Option<TouchInfo>,
    pub errors: Vec<DeviceError>,
}

/// 设备能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub event_types: Vec<u16>,
    pub key_codes: Vec<u16>,
    pub rel_axes: Vec<u16>,
    pub abs_axes: Vec<u16>,
    pub max_touch_slots: Option<u32>,
    pub has_keyboard: bool,
    pub has_mouse: bool,
    pub has_touchpad: bool,
    pub has_touchscreen: bool,
}

/// 触屏物理信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouchInfo {
    pub position_x: AxisInfo,
    pub position_y: AxisInfo,
    pub pressure: Option<AxisInfo>,
    pub touch_major: Option<AxisInfo>,
    pub touch_minor: Option<AxisInfo>,
    pub width_major: Option<AxisInfo>,
    pub width_minor: Option<AxisInfo>,
    pub orientation: Option<AxisInfo>,
    pub tracking_id: Option<AxisInfo>,
    pub slot: Option<AxisInfo>,
    pub mt_protocol: Option<String>,
}

/// 轴信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisInfo {
    pub min: i32,
    pub max: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

/// 设备读取错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceError {
    pub field: String,
    pub message: String,
}

impl RustMessage {
    /// 序列化为 MessagePack
    pub fn to_msgpack(&self) -> Result<Vec<u8>, String> {
        rmp_serde::to_vec(self).map_err(|e| format!("序列化失败: {}", e))
    }

    /// 从 MessagePack 反序列化
    pub fn from_msgpack(data: &[u8]) -> Result<Self, String> {
        rmp_serde::from_slice(data).map_err(|e| format!("反序列化失败: {}", e))
    }
}

impl TsCommand {
    /// 从 MessagePack 反序列化
    pub fn from_msgpack(data: &[u8]) -> Result<Self, String> {
        rmp_serde::from_slice(data).map_err(|e| format!("反序列化命令失败: {}", e))
    }
}

/// 设备类型检测
pub fn detect_device_type(
    has_keyboard: bool,
    has_mouse: bool,
    has_touchpad: bool,
    has_touchscreen: bool,
    name: &str,
) -> String {
    let name_lower = name.to_lowercase();

    if has_touchscreen || name_lower.contains("touch") && !name_lower.contains("pad") {
        "touchscreen".to_string()
    } else if has_touchpad || name_lower.contains("touchpad") || name_lower.contains("synaptics")
    {
        "touchpad".to_string()
    } else if has_keyboard || name_lower.contains("keyboard") || name_lower.contains("kbd") {
        "keyboard".to_string()
    } else if has_mouse || name_lower.contains("mouse") || name_lower.contains("pointer") {
        "mouse".to_string()
    } else if name_lower.contains("gamepad") || name_lower.contains("joystick") {
        "gamepad".to_string()
    } else if name_lower.contains("tablet") || name_lower.contains("pen") {
        "tablet".to_string()
    } else {
        "unknown".to_string()
    }
}
