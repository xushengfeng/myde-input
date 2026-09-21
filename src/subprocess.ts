/**
 * 子进程管理 - 与 Rust 二进制通信
 */

import { type ChildProcess, spawn } from "node:child_process";
import { arch, platform } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { pack, unpack } from "msgpackr";
import type { RustMessage, TsCommand } from "./protocol.ts";
import { ErrorCode, type Result, err, errFromUnknown, ok, okVoid } from "./result.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** 获取 Rust 二进制路径 */
function getBinaryPath(): string {
  const platformName = platform();
  const archName = arch();
  const binaryName = `myde-input-reader-${platformName}-${archName}`;
  return join(__dirname, "..", "bin", binaryName);
}

/** 子进程管理器 */
export class RustBridge {
  private proc: ChildProcess | null = null;
  private messageHandlers: ((msg: RustMessage) => void)[] = [];
  private buffer = Buffer.alloc(0);
  private messageLength: number | null = null;

  /**
   * 启动 Rust 子进程
   */
  spawn(): Result<void> {
    try {
      const binaryPath = getBinaryPath();

      this.proc = spawn(binaryPath, [], {
        stdio: ["pipe", "pipe", "pipe"],
      });

      this.proc.on("error", (e) => {
        this.notifyError(ErrorCode.ProcessSpawnFailed, `子进程启动失败: ${e.message}`, e);
      });

      this.proc.on("exit", (code, signal) => {
        if (code !== 0 && code !== null) {
          this.notifyError(
            ErrorCode.ProcessExited,
            `子进程异常退出: code=${code}, signal=${signal}`,
            { code, signal },
          );
        }
        this.proc = null;
      });

      // 设置 stdout 数据处理
      this.proc.stdout?.on("data", (chunk: Buffer) => {
        this.handleStdoutData(chunk);
      });

      // 设置 stderr 日志（可选）
      this.proc.stderr?.on("data", (chunk: Buffer) => {
        console.error(`[rust] ${chunk.toString()}`);
      });

      return okVoid();
    } catch (e) {
      return errFromUnknown(e);
    }
  }

  /**
   * 处理 stdout 数据流
   */
  private handleStdoutData(chunk: Buffer) {
    this.buffer = Buffer.concat([this.buffer, chunk]);

    while (true) {
      // 读取消息长度（4 bytes little-endian）
      if (this.messageLength === null) {
        if (this.buffer.length < 4) {
          return; // 等待更多数据
        }
        this.messageLength = this.buffer.readUInt32LE(0);
        this.buffer = this.buffer.subarray(4);
      }

      // 读取消息体
      if (this.buffer.length < this.messageLength) {
        return; // 等待更多数据
      }

      const msgData = this.buffer.subarray(0, this.messageLength);
      this.buffer = this.buffer.subarray(this.messageLength);
      this.messageLength = null;

      // 解码 MessagePack
      try {
        const msg = unpack(msgData) as RustMessage;
        this.notifyMessage(msg);
      } catch (e) {
        this.notifyError(
          ErrorCode.MessageDecodeFailed,
          `消息解码失败: ${e instanceof Error ? e.message : String(e)}`,
          e,
        );
      }
    }
  }

  /**
   * 发送命令到 Rust 子进程
   */
  send(cmd: TsCommand): Result<void> {
    if (!this.proc || !this.proc.stdin) {
      return err(ErrorCode.ProcessExited, "子进程未运行");
    }

    try {
      const data = pack(cmd);

      const lenBuf = Buffer.alloc(4);
      lenBuf.writeUInt32LE(data.length, 0);

      this.proc.stdin.write(lenBuf);
      this.proc.stdin.write(data);

      return okVoid();
    } catch (e) {
      return errFromUnknown(e);
    }
  }

  /**
   * 注册消息处理器
   */
  onMessage(handler: (msg: RustMessage) => void) {
    this.messageHandlers.push(handler);
  }

  /**
   * 通知所有消息处理器
   */
  private notifyMessage(msg: RustMessage) {
    for (const handler of this.messageHandlers) {
      try {
        handler(msg);
      } catch (e) {
        console.error("消息处理器错误:", e);
      }
    }
  }

  /**
   * 通知错误
   */
  private notifyError(code: ErrorCode, message: string, detail?: unknown) {
    const error: RustMessage = {
      type: "error",
      code,
      message,
      detail: detail ? String(detail) : undefined,
    };
    this.notifyMessage(error);
  }

  /**
   * 销毁子进程
   */
  destroy() {
    if (this.proc) {
      // 发送退出命令
      this.send({ type: "exit" });

      // 给子进程一点时间退出
      setTimeout(() => {
        if (this.proc) {
          this.proc.kill("SIGTERM");
          this.proc = null;
        }
      }, 100);
    }
  }

  /**
   * 检查子进程是否运行中
   */
  isRunning(): boolean {
    return this.proc !== null;
  }
}
