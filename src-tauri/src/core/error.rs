use serde::Serialize;

/// 所有面向用户的错误文案都写成大白话：调用方（GUI / CLI）直接展示，不再二次翻译。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("网络请求失败：{0}")]
    Network(String),
    #[error("读写文件失败：{0}")]
    Io(String),
    #[error("配置文件读不懂：{0}")]
    Config(String),
    #[error("校验没过：下载包的内容和官方记录不一致，可能包坏了或下载中断")]
    Verify { expected: String, actual: String },
    #[error("没找到：{0}")]
    NotFound(String),
    #[error("{0} {1} 还没安装")]
    NotInstalled(String, String),
    #[error("{0} {1} 已经装过了")]
    AlreadyInstalled(String, String),
    #[error("任务取消了")]
    Cancelled,
    #[error("这台电脑的系统还不支持：{0}")]
    Unsupported(String),
    #[error("权限不够：{0}")]
    Permission(String),
    #[error("数据目录还没选好，请先完成首次设置")]
    DataRootMissing,
    #[error("{0}")]
    Detail(String),
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Network(_) => "network",
            Error::Io(_) => "io",
            Error::Config(_) => "config",
            Error::Verify { .. } => "verify",
            Error::NotFound(_) => "notFound",
            Error::NotInstalled(..) => "notInstalled",
            Error::AlreadyInstalled(..) => "alreadyInstalled",
            Error::Cancelled => "cancelled",
            Error::Unsupported(_) => "unsupported",
            Error::Permission(_) => "permission",
            Error::DataRootMissing => "dataRootMissing",
            Error::Detail(_) => "detail",
        }
    }

    /// 给日志/开发者看的补充信息，普通界面不必展示。
    pub fn detail(&self) -> Option<String> {
        match self {
            Error::Verify { expected, actual } => Some(format!("期望 {expected}，实际 {actual}")),
            _ => None,
        }
    }

    pub fn with_detail(self, ctx: impl std::fmt::Display) -> Error {
        match self {
            Error::Io(m) | Error::Network(m) | Error::Config(m) | Error::Permission(m)
            | Error::NotFound(m) | Error::Unsupported(m) => {
                let merged = if m.is_empty() {
                    format!("{ctx}")
                } else {
                    format!("{ctx}：{m}")
                };
                Error::Detail(merged)
            }
            other => other,
        }
    }
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            kind: &'a str,
            message: String,
            detail: Option<String>,
        }
        Payload {
            kind: self.kind(),
            message: self.to_string(),
            detail: self.detail(),
        }
        .serialize(serializer)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Config(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Error {
        if e.is_connect() {
            Error::Network(format!("连不上：{e}"))
        } else if e.is_timeout() {
            Error::Network(format!("等太久，超时了：{e}"))
        } else {
            Error::Network(e.to_string())
        }
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Error {
        Error::Config(format!("地址拼错了：{e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
