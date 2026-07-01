//! AWS preflight for services that declare `needs_aws` / `needs_aws_caller_email`.
//!
//! `health` *detects* AWS SSO state (read-only); this module *acts* on it —
//! running `aws sso login` when the session is invalid and resolving the caller
//! email that AI services need to namespace their DynamoDB tables.

use std::collections::BTreeMap;
use std::process::Command;

use colored::Colorize;

use crate::config::ServiceConfig;
use crate::error::{Error, Result};
use crate::health::{self, AwsSsoStatus};

/// Run the AWS preflight for a set of services and return env vars to inject.
///
/// If any service declares `needs_aws` (or `needs_aws_caller_email`, which
/// implies it), a valid SSO session is ensured first. If any declares
/// `needs_aws_caller_email`, `AWS_CALLER_EMAIL` is resolved and returned.
pub fn preflight<'a>(
    services: impl IntoIterator<Item = &'a ServiceConfig>,
) -> Result<BTreeMap<String, String>> {
    let mut needs_session = false;
    let mut needs_email = false;
    for svc in services {
        needs_session |= svc.needs_aws || svc.needs_aws_caller_email;
        needs_email |= svc.needs_aws_caller_email;
    }

    let mut env = BTreeMap::new();
    if needs_session {
        ensure_session()?;
    }
    if needs_email {
        env.insert("AWS_CALLER_EMAIL".to_string(), caller_email()?);
    }
    Ok(env)
}

/// Ensure a valid AWS SSO session, running `aws sso login` if it is expired.
pub fn ensure_session() -> Result<()> {
    match health::aws_sso_status() {
        AwsSsoStatus::Valid(_) => Ok(()),
        AwsSsoStatus::NotInstalled => Err(Error::Other(
            "AWS CLI not installed — install it and run `aws sso login`.".into(),
        )),
        AwsSsoStatus::Expired => {
            println!(
                "{}",
                "AWS SSO session expired — running `aws sso login`...".blue()
            );
            let status = Command::new("aws")
                .args(["sso", "login"])
                .status()
                .map_err(|e| Error::Other(format!("Failed to run `aws sso login`: {e}")))?;
            if !status.success() {
                return Err(Error::Other("`aws sso login` failed.".into()));
            }
            if !health::aws_sso_is_valid() {
                return Err(Error::Other(
                    "AWS SSO session still invalid after `aws sso login`.".into(),
                ));
            }
            Ok(())
        }
    }
}

/// Resolve the caller's email from `aws sts get-caller-identity`.
///
/// The SSO `UserId` has the form `<role-id>:<role-session-name>`, and Productive's
/// SSO sets the session name to the user's email — same mechanism as `paws -w`.
pub fn caller_email() -> Result<String> {
    let output = Command::new("aws")
        .args([
            "sts",
            "get-caller-identity",
            "--no-cli-pager",
            "--output",
            "json",
        ])
        .output()
        .map_err(|e| Error::Other(format!("Failed to run `aws sts get-caller-identity`: {e}")))?;
    if !output.status.success() {
        return Err(Error::Other(
            "`aws sts get-caller-identity` failed — is your AWS SSO session valid?".into(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_caller_email(&stdout).ok_or_else(|| {
        Error::Other("Could not resolve AWS_CALLER_EMAIL from caller identity.".into())
    })
}

/// Extract the email (role-session name) from a `get-caller-identity` payload.
fn parse_caller_email(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let user_id = value.get("UserId")?.as_str()?;
    let (_, email) = user_id.split_once(':')?;
    if email.is_empty() {
        None
    } else {
        Some(email.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceConfig;

    fn svc(needs_aws: bool, needs_email: bool) -> ServiceConfig {
        let mut s: ServiceConfig = toml::from_str("").unwrap();
        s.needs_aws = needs_aws;
        s.needs_aws_caller_email = needs_email;
        s
    }

    #[test]
    fn parse_caller_email_extracts_session_name() {
        let json = r#"{"UserId":"AROAEXAMPLEID:tibor.rogulja@productive.io","Account":"123","Arn":"arn:..."}"#;
        assert_eq!(
            parse_caller_email(json).as_deref(),
            Some("tibor.rogulja@productive.io")
        );
        // Missing/empty session name and malformed input yield None
        assert_eq!(parse_caller_email(r#"{"UserId":"AROAEXAMPLEID"}"#), None);
        assert_eq!(parse_caller_email(r#"{"UserId":"AROAEXAMPLEID:"}"#), None);
        assert_eq!(parse_caller_email("not json"), None);
    }

    #[test]
    fn preflight_without_flags_skips_aws() {
        // No service requests AWS → no shell-out, empty env.
        let a = svc(false, false);
        let b = svc(false, false);
        let env = preflight([&a, &b]).unwrap();
        assert!(env.is_empty());
    }
}
