use anyhow::{Result, anyhow, ensure};
use reqwest::{StatusCode, Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{io::Read, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    DeepSeek,
    OpenAi,
    SiliconFlow,
    Qwen,
    Custom,
}

impl Provider {
    pub const ALL: [Self; 5] = [
        Self::DeepSeek,
        Self::OpenAi,
        Self::SiliconFlow,
        Self::Qwen,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => "DeepSeek",
            Self::OpenAi => "OpenAI",
            Self::SiliconFlow => "硅基流动",
            Self::Qwen => "通义千问 · 百炼",
            Self::Custom => "自定义兼容接口",
        }
    }

    pub fn fixed_base(self) -> Option<&'static str> {
        match self {
            Self::DeepSeek => Some("https://api.deepseek.com"),
            Self::OpenAi => Some("https://api.openai.com/v1"),
            Self::SiliconFlow => Some("https://api.siliconflow.cn/v1"),
            Self::Qwen | Self::Custom => None,
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek-flash",
            Self::OpenAi => "gpt-5-mini",
            Self::SiliconFlow => "deepseek-ai/DeepSeek-V4-Flash",
            Self::Qwen => "qwen-plus",
            Self::Custom => "",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    pub provider: Provider,
    pub base: String,
    pub model: String,
    pub output_limit: u32,
    pub modern_token_limit: bool,
}

impl Default for Connection {
    fn default() -> Self {
        Self::for_provider(Provider::DeepSeek)
    }
}

impl Connection {
    pub fn for_provider(provider: Provider) -> Self {
        Self {
            provider,
            base: provider.fixed_base().unwrap_or_default().into(),
            model: provider.default_model().into(),
            output_limit: 2048,
            modern_token_limit: provider == Provider::OpenAi,
        }
    }

    pub fn endpoint(&self) -> Result<Url> {
        let base = self.provider.fixed_base().unwrap_or(self.base.trim());
        let mut url = Url::parse(base).map_err(|_| anyhow!("请输入有效的 API 地址。"))?;
        ensure!(
            url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "API 地址应包含主机，且不能包含账号、密码、查询参数或片段。"
        );
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http" && self.provider == Provider::Custom && loopback(&url)),
            "请使用 HTTPS。本机模型可以使用 localhost 或回环 IP 的 HTTP 地址。"
        );
        let path = url.path().trim_end_matches('/');
        let path = if path.ends_with("/chat/completions") {
            path.to_owned()
        } else {
            format!("{path}/chat/completions")
        };
        url.set_path(&path);
        Ok(url)
    }

    pub fn validate_config(&self) -> Result<Url> {
        let endpoint = self.endpoint()?;
        let model = self.model.trim();
        ensure!(
            !model.is_empty() && model.len() <= 240 && !model.chars().any(char::is_control),
            "请输入有效的模型标识。"
        );
        Ok(endpoint)
    }

    pub fn validate(&self, key: &str) -> Result<Url> {
        let endpoint = self.validate_config()?;
        let key = key.trim();
        ensure!(
            !key.is_empty() || (self.provider == Provider::Custom && loopback(&endpoint)),
            "请填写该服务的 API 密钥。"
        );
        ensure!(
            key.len() <= 4096 && key.bytes().all(|byte| byte.is_ascii_graphic()),
            "API 密钥包含无效字符。"
        );
        Ok(endpoint)
    }
}

fn loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub connection: Connection,
    protected_key: Option<Vec<u8>>,
}

impl Preferences {
    fn path() -> PathBuf {
        opengalgame::paths::data_directory("ai-connection.json")
    }

    pub fn load() -> Result<Self> {
        match std::fs::read(Self::path()) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn key(&self) -> Result<Option<String>> {
        self.protected_key
            .as_deref()
            .map(|bytes| {
                let endpoint = self.connection.endpoint()?;
                let key = protect(bytes, endpoint.as_str().as_bytes(), false)?;
                Ok(String::from_utf8(key)?)
            })
            .transpose()
    }

    pub fn save(connection: &Connection, key: &str, remember: bool) -> Result<()> {
        connection.validate_config()?;
        let protected_key = if remember && !key.trim().is_empty() {
            let endpoint = connection.validate(key)?;
            Some(protect(
                key.trim().as_bytes(),
                endpoint.as_str().as_bytes(),
                true,
            )?)
        } else {
            None
        };
        opengalgame::project_io::write_json(
            &Self::path(),
            &Self {
                connection: connection.clone(),
                protected_key,
            },
        )
    }
}

#[cfg(windows)]
fn protect(bytes: &[u8], scope: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };

    let blob = |data: &[u8]| CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr().cast_mut(),
    };
    let input = blob(bytes);
    let entropy = blob(scope);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // DPAPI reads the input and entropy and allocates the returned buffer with LocalAlloc.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                null(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        ensure!(ok != 0, "当前 Windows 账户无法处理保存的 API 密钥。");
        if output.pbData.is_null() {
            return Ok(Vec::new());
        }
        let buffer = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = buffer.to_vec();
        if !encrypt {
            buffer.fill(0);
        }
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(not(windows))]
fn protect(_bytes: &[u8], _scope: &[u8], _encrypt: bool) -> Result<Vec<u8>> {
    anyhow::bail!("记住 API 密钥需要 Windows。")
}

pub struct Reply {
    pub text: String,
    pub truncated: bool,
    pub tokens: Option<u64>,
}

pub fn complete(
    connection: &Connection,
    key: &str,
    instruction: &str,
    input: &str,
    testing: bool,
) -> Result<Reply> {
    let endpoint = connection.validate(key)?;
    ensure!(
        input.len() <= opengalgame::story_tools::INPUT_LIMIT,
        "本次输入过长，请先缩短要求或当前对白。"
    );
    let mut payload = json!({
        "model": connection.model.trim(),
        "messages": [
            {"role": "system", "content": instruction},
            {"role": "user", "content": input}
        ],
        "stream": false
    });
    let token_field = if connection.provider == Provider::OpenAi
        || (connection.provider == Provider::Custom && connection.modern_token_limit)
    {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    payload[token_field] = json!(if testing {
        256
    } else {
        connection.output_limit.clamp(256, 8192)
    });
    if connection.provider == Provider::DeepSeek {
        payload["thinking"] = json!({"type": "disabled"});
    }
    if connection.provider == Provider::OpenAi {
        payload["store"] = json!(false);
    }

    let mut builder = Client::builder()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(90));
    if loopback(&endpoint) {
        builder = builder.no_proxy();
        if endpoint.host_str() == Some("localhost") {
            builder = builder.resolve_to_addrs(
                "localhost",
                &["[::1]:0".parse().unwrap(), "127.0.0.1:0".parse().unwrap()],
            );
        }
    }
    let client = builder
        .build()
        .map_err(|_| anyhow!("无法初始化网络连接。"))?;
    let mut request = client.post(endpoint).json(&payload);
    let key = key.trim();
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }
    let response = request.send().map_err(|error| {
        if error.is_timeout() {
            anyhow!("请求超时。可以重试，或降低生成长度。")
        } else {
            anyhow!("连接失败。请检查 API 地址、网络和系统代理。")
        }
    })?;
    let status = response.status();
    ensure!(status.is_success(), "{}", status_message(status));
    const MAX_RESPONSE: u64 = 2 * 1024 * 1024;
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow!("接收回复失败或超时。"))?;
    ensure!(
        bytes.len() as u64 <= MAX_RESPONSE,
        "模型返回内容超过接收上限。"
    );
    let reply: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!("服务没有返回兼容的 JSON。请检查 API 地址。"))?;
    let choice = &reply["choices"][0];
    let text = choice["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow!("模型没有返回正文。请检查模型，或增加输出上限。"))?;
    let text = if key.is_empty() {
        text.to_owned()
    } else {
        text.replace(key, "[密钥已隐藏]")
    };
    Ok(Reply {
        text,
        truncated: choice["finish_reason"].as_str() == Some("length"),
        tokens: reply["usage"]["total_tokens"].as_u64(),
    })
}

fn status_message(status: StatusCode) -> String {
    let help = match status.as_u16() {
        300..=399 => "服务要求跳转，请直接填写最终 API 地址。",
        400 | 422 => "请检查模型名称和接口参数。",
        401 | 403 => "请检查密钥、模型权限和服务区域。",
        402 => "请检查账号余额。",
        404 | 405 => "请检查 API 地址和模型名称。",
        429 => "请求过于频繁或额度已用完。",
        500..=599 => "服务暂时不可用，请稍后重试。",
        _ => "服务拒绝了本次请求。",
    };
    format!("HTTP {} · {help}", status.as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn custom_endpoints_require_tls_except_for_loopback() {
        let mut config = Connection::for_provider(Provider::Custom);
        for rejected in [
            "http://example.test/v1",
            "https://secret@example.test/v1",
            "https://example.test/v1?api_key=hidden",
        ] {
            config.base = rejected.into();
            assert!(config.endpoint().is_err());
        }
        config.base = "http://[::1]:11434/v1/chat/completions/".into();
        assert_eq!(
            config.endpoint().unwrap().as_str(),
            "http://[::1]:11434/v1/chat/completions"
        );
    }

    #[test]
    fn compatible_request_sends_only_prompt_and_parses_reply() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream = BufReader::new(stream);
            let mut headers = String::new();
            let content_length = loop {
                let mut line = String::new();
                assert_ne!(stream.read_line(&mut line).unwrap(), 0);
                headers.push_str(&line);
                if line == "\r\n" {
                    let lower = headers.to_ascii_lowercase();
                    break lower
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")?
                                .trim()
                                .parse::<usize>()
                                .ok()
                        })
                        .unwrap();
                }
            };
            let mut body = vec![0; content_length];
            stream.read_exact(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-key")
            );
            assert_eq!(body["model"], "test-model");
            assert_eq!(body["messages"][1]["content"], "一段对白");
            assert_eq!(body["max_tokens"], 2048);
            assert_eq!(body["stream"], false);
            let response = json!({
                "choices":[{"message":{"content":"修改后的对白"},"finish_reason":"length"}],
                "usage":{"total_tokens":15}
            })
            .to_string();
            write!(
                stream.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            ).unwrap();
        });
        let config = Connection {
            provider: Provider::Custom,
            base: format!("http://127.0.0.1:{port}/v1"),
            model: "test-model".into(),
            ..Connection::default()
        };
        let reply = complete(&config, "test-key", "润色", "一段对白", false).unwrap();
        server.join().unwrap();
        assert_eq!(reply.text, "修改后的对白");
        assert!(reply.truncated);
        assert_eq!(reply.tokens, Some(15));
    }

    #[cfg(windows)]
    #[test]
    fn saved_key_can_only_be_opened_with_matching_endpoint_scope() {
        let key = b"synthetic-test-key";
        let scope = b"https://one.example/v1/chat/completions";
        let encrypted = protect(key, scope, true).unwrap();
        assert!(!encrypted.windows(key.len()).any(|part| part == key));
        assert_eq!(protect(&encrypted, scope, false).unwrap(), key);
        assert!(protect(&encrypted, b"https://other.example", false).is_err());
    }
}
