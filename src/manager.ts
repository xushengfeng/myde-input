/**
 * InputManager - 设备管理器
 */

import { EventEmitter } from "node:events";
import { InputDevice } from "./device.ts";
import { type RustMessage, convertDeviceInfo } from "./protocol.ts";
import { type ErrorCode, type Result, err, okVoid } from "./result.ts";
import { RustBridge } from "./subprocess.ts";
import type { DeviceInfo, InputManagerEvents } from "./types.ts";

/** 输入设备管理器 */
export class InputManager extends EventEmitter {
  private bridge: RustBridge;
  private devices = new Map<string, InputDevice>();
  private initialized = false;

  constructor() {
    super();
    this.bridge = new RustBridge();
  }

  /**
   * 初始化管理器，启动 Rust 子进程
   */
  async init(): Promise<Result<void>> {
    if (this.initialized) {
      return okVoid();
    }

    // 设置消息处理器
    this.bridge.onMessage((msg) => this.handleMessage(msg));

    // 启动子进程
    const spawnResult = this.bridge.spawn();
    if (!spawnResult.ok) {
      return spawnResult;
    }

    this.initialized = true;
    return okVoid();
  }

  /**
   * 处理从 Rust 收到的消息
   */
  private handleMessage(msg: RustMessage) {
    switch (msg.type) {
      case "device_list":
        for (const rustDevice of msg.devices) {
          const info = convertDeviceInfo(rustDevice);
          this.addDevice(info);
        }
        break;

      case "device_added": {
        const info = convertDeviceInfo(msg.device);
        this.addDevice(info);
        break;
      }

      case "device_removed": {
        this.removeDevice(msg.path);
        break;
      }

      case "input_event": {
        const device = this.devices.get(msg.path);
        if (device) {
          device.handleRawEvent({
            devicePath: msg.path,
            type: msg.event_type,
            code: msg.code,
            value: msg.value,
            timestamp: BigInt(msg.timestamp_sec) * 1000000n + BigInt(msg.timestamp_usec),
          });
        }
        break;
      }

      case "error": {
        this.emit("error", {
          code: msg.code as ErrorCode,
          message: msg.message,
          detail: msg.detail,
        });
        break;
      }

      case "ok":
        // 命令成功响应，忽略
        break;
    }
  }

  /**
   * 添加设备
   */
  private addDevice(info: DeviceInfo) {
    if (this.devices.has(info.path)) {
      return; // 已存在
    }

    const device = new InputDevice(info, (cmd) => this.bridge.send(cmd));
    this.devices.set(info.path, device);
    this.emit("deviceAdded", info);
  }

  /**
   * 移除设备
   */
  private removeDevice(path: string) {
    const device = this.devices.get(path);
    if (device) {
      device.destroy();
      this.devices.delete(path);
      this.emit("deviceRemoved", path);
    }
  }

  /**
   * 获取所有设备列表
   */
  getDevices(): DeviceInfo[] {
    return Array.from(this.devices.values()).map((d) => d.info);
  }

  /**
   * 获取单个设备
   */
  getDevice(path: string): InputDevice | undefined {
    return this.devices.get(path);
  }

  /**
   * 销毁管理器
   */
  async destroy(): Promise<void> {
    // 销毁所有设备
    for (const device of this.devices.values()) {
      device.destroy();
    }
    this.devices.clear();

    // 销毁子进程
    this.bridge.destroy();

    this.initialized = false;
    this.removeAllListeners();
  }

  /**
   * 检查是否已初始化
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  // 类型安全的事件监听
  on<K extends keyof InputManagerEvents>(event: K, handler: InputManagerEvents[K]): this {
    return super.on(event, handler);
  }

  emit<K extends keyof InputManagerEvents>(
    event: K,
    ...args: Parameters<InputManagerEvents[K]>
  ): boolean {
    return super.emit(event, ...args);
  }
}
