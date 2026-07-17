use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Number, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use tiberius::{
    AuthMethod, Client, ColumnData, Config, EncryptionLevel, ExecuteResult, Query, Row,
};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "ctox-sqlserver-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

type SqlClient = Client<Compat<TcpStream>>;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SqlServerConfig {
    pub server: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: Option<String>,
    pub password_file: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub encrypt: bool,
    #[serde(default)]
    pub trust_server_certificate: bool,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
    #[serde(default)]
    pub allow_writes: bool,
    #[serde(default = "default_application_name")]
    pub application_name: String,
}

impl std::fmt::Debug for SqlServerConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqlServerConfig")
            .field("server", &self.server)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "[redacted]"))
            .field("password_file", &self.password_file)
            .field("encrypt", &self.encrypt)
            .field("trust_server_certificate", &self.trust_server_certificate)
            .field("request_timeout_ms", &self.request_timeout_ms)
            .field("max_rows", &self.max_rows)
            .field("allow_writes", &self.allow_writes)
            .field("application_name", &self.application_name)
            .finish()
    }
}

impl SqlServerConfig {
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut config: Self = serde_json::from_str(&raw).context("invalid config JSON")?;
        if config.password.is_none() {
            let password_file = config
                .password_file
                .as_ref()
                .context("config requires password or passwordFile")?;
            let password = fs::read_to_string(password_file).with_context(|| {
                format!("failed to read passwordFile {}", password_file.display())
            })?;
            config.password = Some(password.trim_end().to_string());
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("server", self.server.as_str()),
            ("database", self.database.as_str()),
            ("user", self.user.as_str()),
        ] {
            if value.trim().is_empty() {
                bail!("{name} must not be empty");
            }
        }
        if self.password.as_deref().unwrap_or_default().is_empty() {
            bail!("password must not be empty");
        }
        if self.max_rows == 0 || self.max_rows > 50_000 {
            bail!("maxRows must be between 1 and 50000");
        }
        if self.request_timeout_ms < 1_000 || self.request_timeout_ms > 300_000 {
            bail!("requestTimeoutMs must be between 1000 and 300000");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SqlParameter {
    NullString,
    String(String),
    Bytes(Vec<u8>),
    Boolean(bool),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl SqlParameter {
    fn bind<'a>(&'a self, query: &mut Query<'a>) {
        match self {
            Self::NullString => query.bind(Option::<&str>::None),
            Self::String(value) => query.bind(value.as_str()),
            Self::Bytes(value) => query.bind(value.as_slice()),
            Self::Boolean(value) => query.bind(*value),
            Self::I16(value) => query.bind(*value),
            Self::I32(value) => query.bind(*value),
            Self::I64(value) => query.bind(*value),
            Self::F32(value) => query.bind(*value),
            Self::F64(value) => query.bind(*value),
        }
    }
}

pub struct SqlServerAdapter {
    config: SqlServerConfig,
    client: Option<SqlClient>,
    transaction_open: bool,
}

impl SqlServerAdapter {
    pub fn new(config: SqlServerConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            client: None,
            transaction_open: false,
        })
    }

    pub fn config(&self) -> &SqlServerConfig {
        &self.config
    }

    async fn client(&mut self) -> Result<&mut SqlClient> {
        if self.client.is_none() {
            let mut config = Config::new();
            config.host(&self.config.server);
            config.port(self.config.port);
            config.database(&self.config.database);
            config.application_name(&self.config.application_name);
            config.authentication(AuthMethod::sql_server(
                &self.config.user,
                self.config
                    .password
                    .as_deref()
                    .context("password is missing")?,
            ));
            config.encryption(if self.config.encrypt {
                EncryptionLevel::Required
            } else {
                EncryptionLevel::Off
            });
            if self.config.trust_server_certificate {
                config.trust_cert();
            }
            let tcp = tokio::time::timeout(
                std::time::Duration::from_millis(self.config.request_timeout_ms),
                TcpStream::connect(config.get_addr()),
            )
            .await
            .context("SQL Server connection timed out")?
            .context("failed to connect to SQL Server")?;
            tcp.set_nodelay(true).context("failed to set TCP_NODELAY")?;
            let client = tokio::time::timeout(
                std::time::Duration::from_millis(self.config.request_timeout_ms),
                Client::connect(config, tcp.compat_write()),
            )
            .await
            .context("SQL Server authentication timed out")?
            .context("failed to authenticate with SQL Server")?;
            self.client = Some(client);
        }
        Ok(self.client.as_mut().expect("client initialized"))
    }

    pub async fn query(&mut self, sql: &str, parameters: &[SqlParameter]) -> Result<Vec<Value>> {
        validate_read_statement(sql)?;
        self.query_inner(sql, parameters).await
    }

    async fn query_inner(&mut self, sql: &str, parameters: &[SqlParameter]) -> Result<Vec<Value>> {
        ensure_sql_is_nonempty(sql)?;
        let max_rows = self.config.max_rows;
        let request_timeout = std::time::Duration::from_millis(self.config.request_timeout_ms);
        let client = self.client().await?;
        let mut query = Query::new(sql.to_owned());
        for parameter in parameters {
            parameter.bind(&mut query);
        }
        let rows = tokio::time::timeout(request_timeout, async {
            query
                .query(client)
                .await
                .with_context(|| format!("SQL query failed: {}", one_line(sql)))?
                .into_first_result()
                .await
                .context("failed to read SQL result")
        })
        .await
        .context("SQL query timed out")??;
        if rows.len() > max_rows {
            bail!(
                "query returned {} rows, exceeding maxRows {max_rows}",
                rows.len()
            );
        }
        Ok(rows.iter().map(row_to_json).collect())
    }

    pub async fn execute(
        &mut self,
        sql: &str,
        parameters: &[SqlParameter],
    ) -> Result<ExecuteResult> {
        self.ensure_writes()?;
        validate_write_statement(sql)?;
        let request_timeout = std::time::Duration::from_millis(self.config.request_timeout_ms);
        let client = self.client().await?;
        let mut query = Query::new(sql.to_owned());
        for parameter in parameters {
            parameter.bind(&mut query);
        }
        tokio::time::timeout(request_timeout, query.execute(client))
            .await
            .context("SQL statement timed out")?
            .with_context(|| format!("SQL statement failed: {}", one_line(sql)))
    }

    pub async fn execute_returning(
        &mut self,
        sql: &str,
        parameters: &[SqlParameter],
    ) -> Result<Vec<Value>> {
        self.ensure_writes()?;
        validate_write_statement(sql)?;
        self.query_inner(sql, parameters).await
    }

    pub async fn begin_transaction(&mut self) -> Result<()> {
        self.ensure_writes()?;
        if self.transaction_open {
            bail!("nested SQL transactions are not supported");
        }
        self.client()
            .await?
            .simple_query("SET XACT_ABORT ON; BEGIN TRANSACTION;")
            .await
            .context("failed to begin SQL transaction")?
            .into_results()
            .await
            .context("failed to drain begin-transaction result")?;
        self.transaction_open = true;
        Ok(())
    }

    pub async fn commit_transaction(&mut self) -> Result<()> {
        if !self.transaction_open {
            bail!("no SQL transaction is open");
        }
        let result = self
            .client()
            .await?
            .simple_query("COMMIT TRANSACTION;")
            .await
            .context("failed to commit SQL transaction")?
            .into_results()
            .await
            .context("failed to drain commit result");
        if result.is_ok() {
            self.transaction_open = false;
        }
        result.map(|_| ())
    }

    pub async fn rollback_transaction(&mut self) -> Result<()> {
        if !self.transaction_open {
            return Ok(());
        }
        let result = self
            .client()
            .await?
            .simple_query("IF XACT_STATE() <> 0 ROLLBACK TRANSACTION;")
            .await
            .context("failed to roll back SQL transaction")?
            .into_results()
            .await
            .context("failed to drain rollback result");
        self.transaction_open = false;
        result.map(|_| ())
    }

    pub async fn status(&mut self) -> Result<Value> {
        let rows = self
            .query(
                "SELECT @@SERVERNAME AS server_name, DB_NAME() AS database_name, SUSER_SNAME() AS login_name, @@VERSION AS version",
                &[],
            )
            .await?;
        let mut value = rows.into_iter().next().unwrap_or_else(|| json!({}));
        if let Some(object) = value.as_object_mut() {
            object.insert("connected".into(), Value::Bool(true));
            object.insert("allow_writes".into(), Value::Bool(self.config.allow_writes));
            object.insert("max_rows".into(), json!(self.config.max_rows));
        }
        Ok(value)
    }

    pub async fn list_schemas(&mut self) -> Result<Vec<Value>> {
        self.query(
            "SELECT name FROM sys.schemas WHERE principal_id = 1 OR schema_id BETWEEN 5 AND 16383 ORDER BY name",
            &[],
        )
        .await
    }

    pub async fn list_tables(&mut self, schema: Option<&str>) -> Result<Vec<Value>> {
        self.query(
            "SELECT s.name AS schema_name, t.name AS table_name, SUM(p.rows) AS row_count FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id LEFT JOIN sys.partitions p ON p.object_id=t.object_id AND p.index_id IN (0,1) WHERE (@P1 IS NULL OR s.name=@P1) GROUP BY s.name,t.name ORDER BY s.name,t.name",
            &[schema.map(|value| SqlParameter::String(value.to_owned())).unwrap_or(SqlParameter::NullString)],
        ).await
    }

    pub async fn describe_table(&mut self, schema: &str, table: &str) -> Result<Vec<Value>> {
        self.query(
            "SELECT c.column_id, c.name, ty.name AS data_type, c.max_length, c.precision, c.scale, c.is_nullable, c.is_identity, CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS is_primary_key FROM sys.columns c JOIN sys.types ty ON ty.user_type_id=c.user_type_id JOIN sys.tables t ON t.object_id=c.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id LEFT JOIN (SELECT ic.object_id,ic.column_id FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id WHERE i.is_primary_key=1) pk ON pk.object_id=c.object_id AND pk.column_id=c.column_id WHERE s.name=@P1 AND t.name=@P2 ORDER BY c.column_id",
            &[SqlParameter::String(schema.to_owned()), SqlParameter::String(table.to_owned())],
        ).await
    }

    fn ensure_writes(&self) -> Result<()> {
        if !self.config.allow_writes {
            bail!("SQL writes are disabled for this connection");
        }
        Ok(())
    }
}

pub fn run_mcp_from_args() -> Result<()> {
    let config_path = parse_config_arg()?;
    let config = SqlServerConfig::from_path(config_path)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build SQL adapter runtime")?;
    runtime.block_on(McpServer::new(config).serve())
}

fn parse_config_arg() -> Result<String> {
    let mut args = std::env::args().skip(1);
    let mut config = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = args.next(),
            "--help" | "-h" => {
                println!("Usage: ctox-sqlserver-mcp --config /path/to/config.json");
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    config.context("missing --config")
}

struct McpServer {
    adapter: SqlServerAdapter,
}

impl McpServer {
    fn new(config: SqlServerConfig) -> Self {
        Self {
            adapter: SqlServerAdapter::new(config).expect("validated SQL Server config"),
        }
    }

    async fn serve(&mut self) -> Result<()> {
        let mut input = std::io::stdin().lock();
        let mut output = std::io::stdout().lock();
        loop {
            let Some(request) = read_message(&mut input)? else {
                break;
            };
            let response = self.handle(request).await;
            write_message(&mut output, &response)?;
        }
        Ok(())
    }

    async fn handle(&mut self, request: Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION}
            })),
            "notifications/initialized" => return Value::Null,
            "tools/list" => Ok(json!({"tools": mcp_tools()})),
            "tools/call" => {
                self.call_tool(request.get("params").unwrap_or(&Value::Null))
                    .await
            }
            _ => Err(anyhow!("unsupported MCP method: {method}")),
        };
        match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(error) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":format!("{error:#}")}})
            }
        }
    }

    async fn call_tool(&mut self, params: &Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .context("tool name is required")?;
        let args = params.get("arguments").unwrap_or(&Value::Null);
        let value = match name {
            "sql.status" => self.adapter.status().await?,
            "sql.query_readonly" => {
                let sql = required_string(args, "sql")?;
                validate_read_statement(sql)?;
                let parameters = args
                    .get("parameters")
                    .cloned()
                    .map(serde_json::from_value::<Vec<SqlParameter>>)
                    .transpose()
                    .context("invalid typed SQL parameters")?
                    .unwrap_or_default();
                Value::Array(self.adapter.query(sql, &parameters).await?)
            }
            "sql.list_schemas" => Value::Array(self.adapter.list_schemas().await?),
            "sql.list_tables" => Value::Array(
                self.adapter
                    .list_tables(args.get("schema").and_then(Value::as_str))
                    .await?,
            ),
            "sql.describe_table" => Value::Array(
                self.adapter
                    .describe_table(
                        required_string(args, "schema")?,
                        required_string(args, "table")?,
                    )
                    .await?,
            ),
            _ => bail!("unknown tool: {name}"),
        };
        Ok(
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&value)?}],"structuredContent":value}),
        )
    }
}

fn mcp_tools() -> Vec<Value> {
    vec![
        tool(
            "sql.status",
            "Test the configured SQL Server connection.",
            json!({"type":"object","properties":{}}),
        ),
        tool(
            "sql.query_readonly",
            "Run a parameterized read-only diagnostic query.",
            json!({"type":"object","additionalProperties":false,"required":["sql"],"properties":{"sql":{"type":"string","minLength":1},"parameters":{"type":"array","items":{"type":"object","required":["type"],"properties":{"type":{"type":"string","enum":["null_string","string","bytes","boolean","i16","i32","i64","f32","f64"]},"value":{}},"additionalProperties":false}}}}),
        ),
        tool(
            "sql.list_schemas",
            "List database schemas.",
            json!({"type":"object","properties":{}}),
        ),
        tool(
            "sql.list_tables",
            "List tables and approximate row counts.",
            json!({"type":"object","properties":{"schema":{"type":"string"}}}),
        ),
        tool(
            "sql.describe_table",
            "Describe columns and primary-key membership.",
            json!({"type":"object","required":["schema","table"],"properties":{"schema":{"type":"string"},"table":{"type":"string"}}}),
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":input_schema})
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{field} is required"))
}

fn ensure_sql_is_nonempty(sql: &str) -> Result<()> {
    if sql.trim().is_empty() {
        bail!("SQL statement must not be empty");
    }
    Ok(())
}

pub fn validate_read_statement(sql: &str) -> Result<()> {
    ensure_sql_is_nonempty(sql)?;
    if sql.contains("${") || sql.contains("{{") || sql.contains("}}") {
        bail!("SQL interpolation is forbidden; use typed parameters");
    }
    let tokens = sql_tokens(sql);
    if !matches!(
        tokens.first().map(String::as_str),
        Some("SELECT" | "WITH" | "SET")
    ) {
        bail!("diagnostic SQL must be read-only");
    }
    for forbidden in [
        "INSERT", "UPDATE", "DELETE", "MERGE", "DROP", "ALTER", "TRUNCATE", "CREATE", "EXEC",
        "EXECUTE",
    ] {
        if tokens.iter().any(|token| token == forbidden) {
            bail!("diagnostic SQL contains forbidden token `{forbidden}`");
        }
    }
    Ok(())
}

pub fn validate_write_statement(sql: &str) -> Result<()> {
    ensure_sql_is_nonempty(sql)?;
    if sql.contains("${") || sql.contains("{{") || sql.contains("}}") {
        bail!("SQL interpolation is forbidden; use typed parameters");
    }
    let tokens = sql_tokens(sql);
    for forbidden in ["DROP", "ALTER", "TRUNCATE", "GRANT", "REVOKE"] {
        if tokens.iter().any(|token| token == forbidden) {
            bail!("write SQL contains forbidden administrative token `{forbidden}`");
        }
    }
    for pair in tokens.windows(2) {
        if pair == ["CREATE", "LOGIN"] || pair == ["CREATE", "USER"] {
            bail!("write SQL contains forbidden identity administration");
        }
    }
    Ok(())
}

fn sql_tokens(sql: &str) -> Vec<String> {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        String,
        Bracket,
        LineComment,
        BlockComment,
    }

    let chars = sql.chars().collect::<Vec<_>>();
    let mut state = State::Normal;
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    let flush = |current: &mut String, tokens: &mut Vec<String>| {
        if !current.is_empty() {
            tokens.push(std::mem::take(current));
        }
    };
    while index < chars.len() {
        let current_char = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            State::Normal if current_char == '\'' => {
                flush(&mut current, &mut tokens);
                state = State::String;
            }
            State::Normal if current_char == '[' => {
                flush(&mut current, &mut tokens);
                state = State::Bracket;
            }
            State::Normal if current_char == '-' && next == Some('-') => {
                flush(&mut current, &mut tokens);
                state = State::LineComment;
                index += 1;
            }
            State::Normal if current_char == '/' && next == Some('*') => {
                flush(&mut current, &mut tokens);
                state = State::BlockComment;
                index += 1;
            }
            State::Normal if current_char.is_ascii_alphanumeric() || current_char == '_' => {
                current.push(current_char.to_ascii_uppercase());
            }
            State::Normal => flush(&mut current, &mut tokens),
            State::String if current_char == '\'' && next == Some('\'') => index += 1,
            State::String if current_char == '\'' => state = State::Normal,
            State::Bracket if current_char == ']' && next == Some(']') => index += 1,
            State::Bracket if current_char == ']' => state = State::Normal,
            State::LineComment if current_char == '\n' => state = State::Normal,
            State::BlockComment if current_char == '*' && next == Some('/') => {
                state = State::Normal;
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }
    flush(&mut current, &mut tokens);
    tokens
}

fn default_port() -> u16 {
    1433
}
fn default_true() -> bool {
    true
}
fn default_request_timeout_ms() -> u64 {
    30_000
}
fn default_max_rows() -> usize {
    5_000
}
fn default_application_name() -> String {
    "ctox-external-sql-sync".to_owned()
}

fn one_line(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.len() > 240 {
        format!("{}...", &value[..240])
    } else {
        value
    }
}

fn row_to_json(row: &Row) -> Value {
    let mut object = Map::new();
    for (column, value) in row.cells() {
        object.insert(column.name().to_string(), column_to_json(value));
    }
    Value::Object(object)
}

fn column_to_json(value: &ColumnData<'_>) -> Value {
    match value {
        ColumnData::U8(value) => value.map(|value| json!(value)).unwrap_or(Value::Null),
        ColumnData::I16(value) => value.map(|value| json!(value)).unwrap_or(Value::Null),
        ColumnData::I32(value) => value.map(|value| json!(value)).unwrap_or(Value::Null),
        ColumnData::I64(value) => value.map(|value| json!(value)).unwrap_or(Value::Null),
        ColumnData::F32(value) => value
            .and_then(|value| Number::from_f64(value as f64))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ColumnData::F64(value) => value
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ColumnData::Bit(value) => value.map(Value::Bool).unwrap_or(Value::Null),
        ColumnData::String(value) => value
            .as_ref()
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Guid(value) => value
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Binary(value) => value
            .as_ref()
            .map(|value| {
                Value::String(base64::engine::general_purpose::STANDARD.encode(value.as_ref()))
            })
            .unwrap_or(Value::Null),
        ColumnData::Numeric(value) => value
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Xml(value) => value
            .as_ref()
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        other => Value::String(format!("{other:?}")),
    }
}

fn read_message(reader: &mut impl Read) -> Result<Option<Value>> {
    let mut headers = String::new();
    let mut byte = [0u8; 1];
    while reader.read(&mut byte)? == 1 {
        headers.push(byte[0] as char);
        if headers.ends_with("\r\n\r\n") {
            break;
        }
    }
    if headers.is_empty() {
        return Ok(None);
    }
    let length = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:"))
        .map(str::trim)
        .context("missing Content-Length")?
        .parse::<usize>()
        .context("invalid Content-Length")?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(
        serde_json::from_slice(&body).context("invalid MCP JSON")?,
    ))
}

fn write_message(writer: &mut impl Write, value: &Value) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    let body = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validation_is_vendor_neutral_and_fail_closed() {
        let config = SqlServerConfig {
            server: "sql.example.test".into(),
            port: 1433,
            database: "operations".into(),
            user: "sync".into(),
            password: Some("secret".into()),
            password_file: None,
            encrypt: true,
            trust_server_certificate: false,
            request_timeout_ms: 30_000,
            max_rows: 5_000,
            allow_writes: false,
            application_name: default_application_name(),
        };
        config.validate().expect("valid config");
        assert!(!config.allow_writes);
        let debug = format!("{config:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn parameter_contract_round_trips_all_supported_types() {
        let parameters = vec![
            SqlParameter::NullString,
            SqlParameter::String("text".into()),
            SqlParameter::Bytes(vec![1, 2]),
            SqlParameter::Boolean(true),
            SqlParameter::I16(1),
            SqlParameter::I32(2),
            SqlParameter::I64(3),
            SqlParameter::F32(4.5),
            SqlParameter::F64(6.5),
        ];
        let value = serde_json::to_value(&parameters).expect("serialize");
        let decoded: Vec<SqlParameter> = serde_json::from_value(value).expect("deserialize");
        assert_eq!(decoded, parameters);
    }

    #[test]
    fn diagnostic_query_contract_rejects_writes_and_interpolation() {
        validate_read_statement("SELECT name FROM sys.tables WHERE schema_id=@P1")
            .expect("read query");
        assert!(validate_read_statement("UPDATE dbo.items SET name=@P1").is_err());
        assert!(validate_read_statement("SELECT 1;UPDATE dbo.items SET name=@P1").is_err());
        assert!(validate_read_statement("SELECT 'UPDATE' AS label").is_ok());
        assert!(validate_read_statement("SELECT * FROM dbo.items WHERE id={{id}}").is_err());
    }
}
