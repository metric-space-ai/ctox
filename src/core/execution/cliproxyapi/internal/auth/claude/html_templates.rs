// ref: internal/auth/claude/html_templates.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;

/// Byte-for-byte copy of upstream's `LoginSuccessHtml` template.
pub const LOGIN_SUCCESS_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Authentication Successful - Claude</title>
    <link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='%2310b981'%3E%3Cpath d='M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z'/%3E%3C/svg%3E">
    <style>
        * {
            box-sizing: border-box;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            padding: 1rem;
        }
        .container {
            text-align: center;
            background: white;
            padding: 2.5rem;
            border-radius: 12px;
            box-shadow: 0 10px 25px rgba(0,0,0,0.1);
            max-width: 480px;
            width: 100%;
            animation: slideIn 0.3s ease-out;
        }
        @keyframes slideIn {
            from {
                opacity: 0;
                transform: translateY(-20px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
        .success-icon {
            width: 64px;
            height: 64px;
            margin: 0 auto 1.5rem;
            background: #10b981;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            color: white;
            font-size: 2rem;
            font-weight: bold;
        }
        h1 {
            color: #1f2937;
            margin-bottom: 1rem;
            font-size: 1.75rem;
            font-weight: 600;
        }
        .subtitle {
            color: #6b7280;
            margin-bottom: 1.5rem;
            font-size: 1rem;
            line-height: 1.5;
        }
        .setup-notice {
            background: #fef3c7;
            border: 1px solid #f59e0b;
            border-radius: 6px;
            padding: 1rem;
            margin: 1rem 0;
        }
        .setup-notice h3 {
            color: #92400e;
            margin: 0 0 0.5rem 0;
            font-size: 1rem;
        }
        .setup-notice p {
            color: #92400e;
            margin: 0;
            font-size: 0.875rem;
        }
        .setup-notice a {
            color: #1d4ed8;
            text-decoration: none;
        }
        .setup-notice a:hover {
            text-decoration: underline;
        }
        .actions {
            display: flex;
            gap: 1rem;
            justify-content: center;
            flex-wrap: wrap;
            margin-top: 2rem;
        }
        .button {
            padding: 0.75rem 1.5rem;
            border-radius: 8px;
            font-size: 0.875rem;
            font-weight: 500;
            text-decoration: none;
            transition: all 0.2s;
            cursor: pointer;
            border: none;
            display: inline-flex;
            align-items: center;
            gap: 0.5rem;
        }
        .button-primary {
            background: #3b82f6;
            color: white;
        }
        .button-primary:hover {
            background: #2563eb;
            transform: translateY(-1px);
        }
        .button-secondary {
            background: #f3f4f6;
            color: #374151;
            border: 1px solid #d1d5db;
        }
        .button-secondary:hover {
            background: #e5e7eb;
        }
        .countdown {
            color: #9ca3af;
            font-size: 0.75rem;
            margin-top: 1rem;
        }
        .footer {
            margin-top: 2rem;
            padding-top: 1.5rem;
            border-top: 1px solid #e5e7eb;
            color: #9ca3af;
            font-size: 0.75rem;
        }
        .footer a {
            color: #3b82f6;
            text-decoration: none;
        }
        .footer a:hover {
            text-decoration: underline;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="success-icon">✓</div>
        <h1>Authentication Successful!</h1>
        <p class="subtitle">You have successfully authenticated with Claude. You can now close this window and return to your terminal to continue.</p>

        {{SETUP_NOTICE}}

        <div class="actions">
            <button class="button button-primary" onclick="window.close()">
                <span>Close Window</span>
            </button>
            <a href="{{PLATFORM_URL}}" target="_blank" class="button button-secondary">
                <span>Open Platform</span>
                <span>↗</span>
            </a>
        </div>

        <div class="countdown">
            This window will close automatically in <span id="countdown">10</span> seconds
        </div>

        <div class="footer">
            <p>Powered by <a href="https://chatgpt.com" target="_blank">ChatGPT</a></p>
        </div>
    </div>

    <script>
        let countdown = 10;
        const countdownElement = document.getElementById('countdown');

        const timer = setInterval(() => {
            countdown--;
            countdownElement.textContent = countdown;

            if (countdown <= 0) {
                clearInterval(timer);
                window.close();
            }
        }, 1000);

        // Close window when user presses Escape
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                window.close();
            }
        });

        // Focus the close button for keyboard accessibility
        document.querySelector('.button-primary').focus();
    </script>
</body>
</html>"##;

/// Byte-for-byte copy of upstream's `SetupNoticeHtml` fragment.
pub const SETUP_NOTICE_HTML: &str = r##"
        <div class="setup-notice">
            <h3>Additional Setup Required</h3>
            <p>To complete your setup, please visit the <a href="{{PLATFORM_URL}}" target="_blank">Claude</a> to configure your account.</p>
        </div>"##;

/// Rejection returned before an untrusted URL reaches an HTML attribute.
/// The attacker-controlled value is never retained by this error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlTemplateError {
    InvalidPlatformUrl,
    UnsupportedPlatformUrlScheme,
    PlatformUrlContainsControlCharacter,
}

impl fmt::Display for HtmlTemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPlatformUrl => "platform URL is not a valid absolute URL",
            Self::UnsupportedPlatformUrlScheme => "platform URL must use HTTP or HTTPS",
            Self::PlatformUrlContainsControlCharacter => {
                "platform URL contains a control character"
            }
        })
    }
}

impl Error for HtmlTemplateError {}

/// Applies upstream's placeholder semantics after validating and escaping the
/// dynamic platform URL for a quoted HTML attribute.
pub fn render_login_success_html(
    setup_required: bool,
    platform_url: &str,
) -> Result<String, HtmlTemplateError> {
    validate_platform_url(platform_url)?;
    let escaped_platform_url = escape_html_attribute(platform_url);
    let setup_notice = if setup_required {
        SETUP_NOTICE_HTML.replace("{{PLATFORM_URL}}", &escaped_platform_url)
    } else {
        String::new()
    };

    Ok(LOGIN_SUCCESS_HTML
        .replace("{{PLATFORM_URL}}", &escaped_platform_url)
        .replacen("{{SETUP_NOTICE}}", &setup_notice, 1))
}

fn validate_platform_url(platform_url: &str) -> Result<(), HtmlTemplateError> {
    if platform_url.chars().any(char::is_control) {
        return Err(HtmlTemplateError::PlatformUrlContainsControlCharacter);
    }
    let parsed =
        url::Url::parse(platform_url).map_err(|_| HtmlTemplateError::InvalidPlatformUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(HtmlTemplateError::UnsupportedPlatformUrlScheme);
    }
    if parsed.host_str().is_none() {
        return Err(HtmlTemplateError::InvalidPlatformUrl);
    }
    Ok(())
}

fn escape_html_attribute(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn sha256_hex(value: &str) -> String {
        Sha256::digest(value.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

#[test]
    fn plain_render_matches_upstream_replacement_semantics() {
        let rendered = render_login_success_html(false, "https://claude.ai").unwrap();
        let expected = LOGIN_SUCCESS_HTML
            .replace("{{PLATFORM_URL}}", "https://claude.ai")
            .replacen("{{SETUP_NOTICE}}", "", 1);
        assert_eq!(rendered, expected);
    }

#[test]
    fn setup_render_replaces_both_url_sites() {
        let rendered = render_login_success_html(true, "https://claude.ai/settings").unwrap();
        assert_eq!(rendered.matches("https://claude.ai/settings").count(), 2);
        assert_eq!(rendered.matches("Additional Setup Required").count(), 1);
        assert!(!rendered.contains("{{PLATFORM_URL}}"));
        assert!(!rendered.contains("{{SETUP_NOTICE}}"));
    }

#[test]
    fn adversarial_attribute_text_is_escaped_everywhere() {
        let adversarial = "https://claude.ai/?a=1&b='\"><script>alert(1)</script>";
        let rendered = render_login_success_html(true, adversarial).unwrap();
        let escaped =
            "https://claude.ai/?a=1&amp;b=&#39;&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;";
        assert_eq!(rendered.matches(escaped).count(), 2);
        assert!(!rendered.contains(adversarial));
        assert!(!rendered.contains("<script>alert(1)</script>"));
    }

#[test]
    fn active_and_ambiguous_urls_are_rejected_without_reflection() {
        let cases = [
            (
                "javascript:alert(document.cookie)",
                HtmlTemplateError::UnsupportedPlatformUrlScheme,
            ),
            (
                "data:text/html,<script>alert(1)</script>",
                HtmlTemplateError::UnsupportedPlatformUrlScheme,
            ),
            ("//claude.ai", HtmlTemplateError::InvalidPlatformUrl),
            ("https://", HtmlTemplateError::InvalidPlatformUrl),
            (
                "https://claude.ai/\nmalicious",
                HtmlTemplateError::PlatformUrlContainsControlCharacter,
            ),
        ];
        for (input, expected) in cases {
            let error = render_login_success_html(true, input).unwrap_err();
            assert_eq!(error, expected);
            assert!(!error.to_string().contains(input));
            assert!(!format!("{error:?}").contains(input));
        }
    }
}
