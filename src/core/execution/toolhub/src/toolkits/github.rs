//! Pilot toolkit: GitHub REST API (https://docs.github.com/rest).
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "github";

pub struct GitHub {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::with_api_base("https://api.github.com")
    }
}

fn owner_repo_props() -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert(
        "owner".into(),
        json!({"type": "string", "description": "Repository owner (user or organization)"}),
    );
    m.insert(
        "repo".into(),
        json!({"type": "string", "description": "Repository name"}),
    );
    m
}

impl GitHub {
    pub fn with_api_base(api_base: &str) -> Self {
        let mut list_props = owner_repo_props();
        list_props.insert(
            "state".into(),
            json!({"type": "string", "enum": ["open", "closed", "all"], "default": "open"}),
        );
        list_props.insert(
            "per_page".into(),
            json!({"type": "integer", "minimum": 1, "maximum": 100, "default": 30}),
        );
        let mut create_props = owner_repo_props();
        create_props.insert("title".into(), json!({"type": "string"}));
        create_props.insert("body".into(), json!({"type": "string"}));
        create_props.insert(
            "labels".into(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        let tool = |slug: &str,
                    de_t: &str,
                    en_t: &str,
                    de: &str,
                    en: &str,
                    props: serde_json::Map<String, Value>,
                    req: &[&str],
                    scopes: &[&str],
                    mutating: bool| ToolSpec {
            slug: slug.into(),
            toolkit: SLUG.into(),
            title: LocalizedText::new(de_t, en_t),
            description: LocalizedText::new(de, en),
            input_schema: json!({"type": "object", "properties": props, "required": req, "additionalProperties": false}),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            mutating,
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "GitHub".into(),
            description: LocalizedText::new(
                "Repositories, Issues und Pull Requests auf GitHub.",
                "Repositories, issues and pull requests on GitHub.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://github.com/login/oauth/authorize".into(),
                    token_url: "https://github.com/login/oauth/access_token".into(),
                    default_scopes: vec!["repo".into()],
                    pkce: true,
                    scope_separator: " ".into(),
                    // GitHub Apps with expiring user tokens issue refresh tokens.
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // Fine-grained / classic personal access token.
                AuthScheme::Bearer,
            ],
            tools: vec![
                tool(
                    "GITHUB_GET_REPO",
                    "Repository abrufen",
                    "Get repository",
                    "Liefert Metadaten eines Repositorys.",
                    "Returns repository metadata.",
                    owner_repo_props(),
                    &["owner", "repo"],
                    &["repo"],
                    false,
                ),
                tool(
                    "GITHUB_LIST_ISSUES",
                    "Issues auflisten",
                    "List issues",
                    "Listet Issues eines Repositorys (ohne Pull Requests).",
                    "Lists repository issues (excluding pull requests).",
                    list_props,
                    &["owner", "repo"],
                    &["repo"],
                    false,
                ),
                tool(
                    "GITHUB_CREATE_ISSUE",
                    "Issue anlegen",
                    "Create issue",
                    "Legt ein neues Issue an.",
                    "Creates a new issue.",
                    create_props,
                    &["owner", "repo", "title"],
                    &["repo"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn base(&self, method: &str, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}{path}", self.api_base))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
    }
}

impl ToolkitAdapter for GitHub {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let owner = path_segment(args, "owner")?;
        let repo = path_segment(args, "repo")?;
        match tool {
            "GITHUB_GET_REPO" => Ok(self.base("GET", &format!("/repos/{owner}/{repo}"))),
            "GITHUB_LIST_ISSUES" => {
                let state = args.get("state").and_then(Value::as_str).unwrap_or("open");
                if !matches!(state, "open" | "closed" | "all") {
                    bail!("invalid argument `state`");
                }
                let per_page = args
                    .get("per_page")
                    .and_then(Value::as_u64)
                    .unwrap_or(30)
                    .clamp(1, 100);
                Ok(self.base(
                    "GET",
                    &format!("/repos/{owner}/{repo}/issues?state={state}&per_page={per_page}"),
                ))
            }
            "GITHUB_CREATE_ISSUE" => {
                let title = args
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing argument `title`"))?;
                let mut body = json!({"title": title});
                if let Some(b) = args.get("body").and_then(Value::as_str) {
                    body["body"] = json!(b);
                }
                if let Some(labels) = args.get("labels").and_then(Value::as_array) {
                    body["labels"] =
                        Value::Array(labels.iter().filter(|l| l.is_string()).cloned().collect());
                }
                Ok(self
                    .base("POST", &format!("/repos/{owner}/{repo}/issues"))
                    .json(&body))
            }
            other => bail!("unknown GitHub tool `{other}`"),
        }
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "GITHUB_LIST_ISSUES" => Value::Array(
                v.as_array().into_iter().flatten()
                    .filter(|i| i.get("pull_request").is_none())
                    .map(|i| json!({"number": i["number"], "title": i["title"], "state": i["state"],
                        "html_url": i["html_url"], "user": i["user"]["login"], "labels":
                        i["labels"].as_array().map(|l| l.iter().map(|x| x["name"].clone()).collect::<Vec<_>>())}))
                    .collect(),
            ),
            "GITHUB_GET_REPO" => json!({"full_name": v["full_name"], "description": v["description"],
                "private": v["private"], "default_branch": v["default_branch"], "html_url": v["html_url"],
                "open_issues_count": v["open_issues_count"], "stargazers_count": v["stargazers_count"]}),
            "GITHUB_CREATE_ISSUE" => json!({"number": v["number"], "html_url": v["html_url"], "title": v["title"]}),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_built_and_injection_is_rejected() {
        let gh = GitHub::with_api_base("https://gh.test/");
        let r = gh
            .build_request(
                "GITHUB_LIST_ISSUES",
                &json!({"owner": "metric-space-ai", "repo": "ctox", "per_page": 500}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://gh.test/repos/metric-space-ai/ctox/issues?state=open&per_page=100"
        );
        assert!(gh
            .build_request("GITHUB_GET_REPO", &json!({"owner": "a/../b", "repo": "x"}))
            .is_err());
        assert!(gh
            .build_request("GITHUB_GET_REPO", &json!({"owner": "a", "repo": "x?y=1"}))
            .is_err());
        assert!(gh
            .build_request(
                "GITHUB_LIST_ISSUES",
                &json!({"owner": "a", "repo": "b", "state": "x&y"})
            )
            .is_err());
        let c = gh
            .build_request(
                "GITHUB_CREATE_ISSUE",
                &json!({"owner": "a", "repo": "b", "title": "T", "labels": ["bug", 1]}),
            )
            .unwrap();
        assert_eq!(c.method, "POST");
        let body: Value = serde_json::from_slice(&c.body.unwrap()).unwrap();
        assert_eq!(body, json!({"title": "T", "labels": ["bug"]}));
        for t in &gh.spec().tools {
            assert!(!t.title.de.is_empty() && !t.title.en.is_empty());
        }
    }
}
