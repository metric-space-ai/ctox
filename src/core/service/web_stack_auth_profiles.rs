// Origin: CTOX
// License: AGPL-3.0-only
// Compiled provider boundaries for native auth assist. No credential values.

#[derive(Clone, Copy)]
pub(crate) struct WebStackAuthSourceDefaults {
    pub(crate) login_url: &'static str,
    pub(crate) allowed_domains: &'static [&'static str],
    pub(crate) secret_name: &'static str,
}

pub(crate) fn web_stack_auth_source_defaults(
    source_id: &str,
) -> Option<WebStackAuthSourceDefaults> {
    match source_id.trim().to_ascii_lowercase().as_str() {
        "google.de" | "google" | "google-search" | "maps.google.com" => {
            Some(WebStackAuthSourceDefaults {
                login_url: "https://accounts.google.com/ServiceLogin",
                allowed_domains: &["google.de", "google.com"],
                secret_name: "GOOGLE_BROWSER_LOGIN",
            })
        }
        "rocketreach.com" => Some(WebStackAuthSourceDefaults {
            login_url: "https://rocketreach.co/login",
            allowed_domains: &["rocketreach.com", "rocketreach.co"],
            secret_name: "ROCKETREACH_BROWSER_LOGIN",
        }),
        _ => None,
    }
}

pub(super) struct WebStackEmailOtpRecipe {
    sender_domains: &'static [&'static str],
}

pub(super) fn web_stack_email_otp_recipe(source_id: &str) -> Option<WebStackEmailOtpRecipe> {
    let sender_domains: &'static [&'static str] =
        match source_id.trim().to_ascii_lowercase().as_str() {
            "google.de" | "google" | "google-search" | "maps.google.com" => &["google.com"],
            "dnbhoovers.com" | "app.dnbhoovers.com" => &[
                "mail.dnb.com",
                "dnb.com",
                "hoovers.com",
                "okta.com",
                "okta-emea.com",
            ],
            "leadfeeder.com" | "app.leadfeeder.com" => &["leadfeeder.com", "dealfront.com"],
            "xing.com" => &["xing.com"],
            "linkedin.com" => &["linkedin.com"],
            "rocketreach.com" | "rocketreach.co" => &["rocketreach.co", "rocketreach.com"],
            _ => return None,
        };
    Some(WebStackEmailOtpRecipe { sender_domains })
}

pub(super) fn email_otp_sender_matches(recipe: &WebStackEmailOtpRecipe, sender: &str) -> bool {
    let sender = sender.trim().to_ascii_lowercase();
    let sender_domain = sender.rsplit('@').next().unwrap_or("");
    recipe
        .sender_domains
        .iter()
        .any(|domain| sender_domain == *domain || sender_domain.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_login_is_independent_of_bright_data() {
        for source in [
            "google.de",
            "google",
            "google-search",
            " Google.DE ",
            "maps.google.com",
            " Maps.Google.COM ",
        ] {
            let profile = web_stack_auth_source_defaults(source).expect("Google profile");
            assert_eq!(
                profile.login_url,
                "https://accounts.google.com/ServiceLogin"
            );
            assert_eq!(profile.secret_name, "GOOGLE_BROWSER_LOGIN");
            assert_eq!(profile.allowed_domains, &["google.de", "google.com"]);
        }
        for source in [
            "brightdata.com",
            "linkedin.com",
            "google.de.evil.example",
            "maps.google.com.evil.example",
        ] {
            assert!(web_stack_auth_source_defaults(source).is_none());
        }
    }

    #[test]
    fn google_email_codes_require_google_sender_domain() {
        let recipe = web_stack_email_otp_recipe("google.de").expect("Google OTP profile");
        for sender in [
            "account-verification-noreply@google.com",
            "no-reply@accounts.google.com",
        ] {
            assert!(email_otp_sender_matches(&recipe, sender));
        }
        for sender in [
            "attacker@google.com.evil.example",
            "attacker@evilgoogle.com",
            "noreply@brightdata.com",
            "noreply@gmail.com",
        ] {
            assert!(!email_otp_sender_matches(&recipe, sender));
        }
    }

    #[test]
    fn google_maps_uses_the_google_code_boundary_without_enabling_foreign_sources() {
        for source in ["maps.google.com", " Maps.Google.COM ", " Google.DE "] {
            let recipe = web_stack_email_otp_recipe(source).expect("Google account OTP profile");
            assert!(email_otp_sender_matches(
                &recipe,
                "no-reply@accounts.google.com"
            ));
            for sender in [
                "no-reply@brightdata.com",
                "attacker@google.com.evil.example",
                "attacker@evilgoogle.com",
            ] {
                assert!(!email_otp_sender_matches(&recipe, sender));
            }
        }
        for source in [
            "brightdata.com",
            "maps.google.com.evil.example",
            "google.de.evil.example",
        ] {
            assert!(web_stack_email_otp_recipe(source).is_none());
        }
    }

    #[test]
    fn existing_auth_profiles_keep_their_provider_boundaries() {
        let rocket = web_stack_auth_source_defaults("rocketreach.com").unwrap();
        assert_eq!(rocket.secret_name, "ROCKETREACH_BROWSER_LOGIN");
        let dnb = web_stack_email_otp_recipe("dnbhoovers.com").unwrap();
        assert!(email_otp_sender_matches(&dnb, "no-reply@mail.dnb.com"));
        assert!(!email_otp_sender_matches(
            &dnb,
            "no-reply@accounts.google.com"
        ));
        assert!(web_stack_email_otp_recipe("brightdata.com").is_none());
    }
}
