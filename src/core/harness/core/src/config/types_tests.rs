use super::*;
use pretty_assertions::assert_eq;













#[test]
fn deserialize_rejects_command_and_url() {
    toml::from_str::<McpServerConfig>(
        r#"
            command = "echo"
            url = "https://example.com"
        "#,
    )
    .expect_err("should reject command+url");
}

#[test]
fn deserialize_rejects_env_for_http_transport() {
    toml::from_str::<McpServerConfig>(
        r#"
            url = "https://example.com"
            env = { "FOO" = "BAR" }
        "#,
    )
    .expect_err("should reject env for http transport");
}

#[test]
fn deserialize_rejects_headers_for_stdio() {
    toml::from_str::<McpServerConfig>(
        r#"
            command = "echo"
            http_headers = { "X-Foo" = "bar" }
        "#,
    )
    .expect_err("should reject http_headers for stdio transport");

    toml::from_str::<McpServerConfig>(
        r#"
            command = "echo"
            env_http_headers = { "X-Foo" = "BAR_ENV" }
        "#,
    )
    .expect_err("should reject env_http_headers for stdio transport");

    let err = toml::from_str::<McpServerConfig>(
        r#"
            command = "echo"
            oauth_resource = "https://api.example.com"
        "#,
    )
    .expect_err("should reject oauth_resource for stdio transport");

    assert!(
        err.to_string()
            .contains("oauth_resource is not supported for stdio"),
        "unexpected error: {err}"
    );
}

#[test]
fn deserialize_rejects_inline_bearer_token_field() {
    let err = toml::from_str::<McpServerConfig>(
        r#"
            url = "https://example.com"
            bearer_token = "secret"
        "#,
    )
    .expect_err("should reject bearer_token field");

    assert!(
        err.to_string().contains("bearer_token is not supported"),
        "unexpected error: {err}"
    );
}
