//! 返回给前端的错误，序列化为 `{ code, message }`。

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum ErrorCode {
    /// 当前没有输出设备。
    NoDevice,
    /// 应用已没有音频会话（可能刚刚退出）。
    AppNotFound,
    /// Windows 音频接口调用失败。
    ComFailure,
    /// 音频线程未运行。
    AudioThreadDown,
}

#[derive(Debug, Clone, Serialize, Type, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<windows_core::Error> for AppError {
    fn from(e: windows_core::Error) -> Self {
        // 原始 HRESULT 只写日志，界面展示统一文案。
        eprintln!("[audio] COM 调用失败：{e}");
        Self::new(ErrorCode::ComFailure, "音频操作失败，请稍后重试")
    }
}
