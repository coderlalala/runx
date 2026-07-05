use std::collections::BTreeMap;
use std::path::Path;

use runx_runtime::registry::{
    HttpMethod, HttpRequest, RegistryPublishHarnessReport, RuntimeHttpHeader, Transport,
};
use serde::{Deserialize, Serialize};

use super::package::{HostedSkillPackageFile, SkillPackage};
use super::{RegistryCliError, RegistryPlan, internal_error, usage_error};

pub(super) fn publish_remote_skill_package(
    registry_url: &str,
    plan: &RegistryPlan,
    package: &SkillPackage,
    harness: &RegistryPublishHarnessReport,
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<HostedSkillPublishResult, RegistryCliError> {
    if plan.trust_tier.is_some() {
        return Err(usage_error(
            "remote registry publish derives trust from hosted verification; --trust-tier is local-only",
        ));
    }
    if admin_publish_requested(plan) {
        let token = admin_publish_token(env).ok_or_else(|| {
            usage_error(
                "remote registry admin publish requires RUNX_HOSTED_REGISTRY_PUBLISH_TOKEN or RUNX_HOSTED_API_ADMIN_TOKEN",
            )
        })?;
        let owner = admin_publish_owner(plan, env)?;
        let transport = crate::public_api::transport(registry_private_network_allowed(env))
            .map_err(|error| internal_error(error.to_string()))?;
        return publish_remote_admin_skill_package_with_transport(
            &transport,
            registry_url,
            &token,
            &owner,
            plan.version.as_deref(),
            plan.upsert,
            package,
            harness,
        );
    }
    let token = crate::public_api_token::resolve(None, env, cwd)?.ok_or_else(|| {
        usage_error("remote registry publish requires `runx login` or RUNX_PUBLIC_API_TOKEN")
    })?;
    let transport = crate::public_api::transport(registry_private_network_allowed(env))
        .map_err(|error| internal_error(error.to_string()))?;
    publish_remote_skill_package_with_transport(
        &transport,
        registry_url,
        &token,
        plan.version.as_deref(),
        package,
    )
}

fn admin_publish_requested(plan: &RegistryPlan) -> bool {
    plan.owner.is_some() || plan.upsert
}

fn admin_publish_token(env: &BTreeMap<String, String>) -> Option<String> {
    [
        "RUNX_HOSTED_REGISTRY_PUBLISH_TOKEN",
        "RUNX_HOSTED_API_ADMIN_TOKEN",
    ]
    .iter()
    .find_map(|name| {
        env.get(*name)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn admin_publish_owner(
    plan: &RegistryPlan,
    env: &BTreeMap<String, String>,
) -> Result<String, RegistryCliError> {
    plan.owner
        .as_deref()
        .or_else(|| {
            env.get("RUNX_HOSTED_REGISTRY_PUBLISH_OWNER")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            usage_error("remote registry admin publish requires --owner or RUNX_HOSTED_REGISTRY_PUBLISH_OWNER")
        })
}

pub(super) fn publish_remote_skill_package_with_transport<T: Transport>(
    transport: &T,
    registry_url: &str,
    token: &str,
    version: Option<&str>,
    package: &SkillPackage,
) -> Result<HostedSkillPublishResult, RegistryCliError> {
    let body = serde_json::to_string(&HostedSkillPublishRequest {
        markdown: &package.markdown,
        profile_document: package.profile_document.as_deref(),
        version,
        package_files: &package.package_files,
    })
    .map_err(|error| internal_error(error.to_string()))?;
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Post,
            url: format!("{}/v1/skills", registry_url.trim_end_matches('/')),
            headers: vec![
                RuntimeHttpHeader::new("authorization", format!("Bearer {token}")),
                RuntimeHttpHeader::new("content-type", "application/json"),
            ],
            body: Some(body),
        })
        .map_err(|error| internal_error(error.to_string()))?;
    if !(200..=299).contains(&response.status) {
        if let Some(error) = crate::public_api::parse_error(&response.body) {
            return Err(internal_error(format!(
                "remote registry publish failed [{}]: {}",
                error.code, error.detail
            )));
        }
        return Err(internal_error(format!(
            "remote registry publish returned HTTP {}: {}",
            response.status, response.body
        )));
    }
    let envelope =
        serde_json::from_str::<HostedSkillPublishEnvelope>(&response.body).map_err(|error| {
            internal_error(format!(
                "remote registry publish returned invalid JSON: {error}"
            ))
        })?;
    if envelope.status != "success" || envelope.publish.status != "published" {
        return Err(internal_error(format!(
            "remote registry publish returned unsuccessful status: envelope={}, publish={}",
            envelope.status, envelope.publish.status
        )));
    }
    Ok(envelope.publish)
}

pub(super) fn publish_remote_admin_skill_package_with_transport<T: Transport>(
    transport: &T,
    registry_url: &str,
    token: &str,
    owner: &str,
    version: Option<&str>,
    upsert: bool,
    package: &SkillPackage,
    harness: &RegistryPublishHarnessReport,
) -> Result<HostedSkillPublishResult, RegistryCliError> {
    let body = serde_json::to_string(&HostedAdminSkillPublishRequest {
        owner,
        markdown: &package.markdown,
        profile_document: package.profile_document.as_deref(),
        version,
        upsert,
        package_files: &package.package_files,
        harness,
    })
    .map_err(|error| internal_error(error.to_string()))?;
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Post,
            url: format!(
                "{}/v1/admin/registry/publish",
                registry_url.trim_end_matches('/')
            ),
            headers: vec![
                RuntimeHttpHeader::new("authorization", format!("Bearer {token}")),
                RuntimeHttpHeader::new("content-type", "application/json"),
            ],
            body: Some(body),
        })
        .map_err(|error| internal_error(error.to_string()))?;
    if !(200..=299).contains(&response.status) {
        if let Some(error) = crate::public_api::parse_error(&response.body) {
            return Err(internal_error(format!(
                "remote registry admin publish failed [{}]: {}",
                error.code, error.detail
            )));
        }
        return Err(internal_error(format!(
            "remote registry admin publish returned HTTP {}: {}",
            response.status, response.body
        )));
    }
    let envelope = serde_json::from_str::<HostedAdminSkillPublishEnvelope>(&response.body)
        .map_err(|error| {
            internal_error(format!(
                "remote registry admin publish returned invalid JSON: {error}"
            ))
        })?;
    if envelope.status != "success"
        || !matches!(envelope.publish.status.as_str(), "published" | "unchanged")
    {
        return Err(internal_error(format!(
            "remote registry admin publish returned unsuccessful status: envelope={}, publish={}",
            envelope.status, envelope.publish.status
        )));
    }
    Ok(envelope.publish.into_hosted_result())
}

fn registry_private_network_allowed(env: &BTreeMap<String, String>) -> bool {
    crate::public_api::private_network_allowed(false, env, "RUNX_REGISTRY_ALLOW_LOCAL_API")
}

#[derive(Serialize)]
struct HostedSkillPublishRequest<'a> {
    markdown: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_document: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    package_files: &'a Vec<HostedSkillPackageFile>,
}

#[derive(Serialize)]
struct HostedAdminSkillPublishRequest<'a> {
    owner: &'a str,
    markdown: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_document: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    upsert: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    package_files: &'a Vec<HostedSkillPackageFile>,
    harness: &'a RegistryPublishHarnessReport,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HostedSkillPublishEnvelope {
    status: String,
    publish: HostedSkillPublishResult,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HostedAdminSkillPublishEnvelope {
    status: String,
    publish: HostedAdminSkillPublishResult,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HostedAdminSkillPublishResult {
    status: String,
    skill_id: String,
    name: String,
    version: String,
    digest: String,
    #[serde(default)]
    profile_digest: Option<String>,
    #[serde(default)]
    record: Option<HostedAdminSkillRecord>,
    link: HostedSkillPublishLink,
}

impl HostedAdminSkillPublishResult {
    fn into_hosted_result(self) -> HostedSkillPublishResult {
        let owner = self
            .record
            .as_ref()
            .map(|record| record.owner.clone())
            .or_else(|| {
                self.skill_id
                    .split_once('/')
                    .map(|(owner, _)| owner.to_owned())
            })
            .unwrap_or_default();
        let trust_tier = self
            .record
            .as_ref()
            .and_then(|record| record.trust_tier.clone())
            .unwrap_or_else(|| "first_party".to_owned());
        HostedSkillPublishResult {
            status: self.status,
            public_url: self.link.public_url(&self.skill_id, &self.version),
            skill_id: self.skill_id,
            owner,
            name: self.name,
            version: self.version,
            digest: self.digest,
            profile_digest: self.profile_digest,
            trust_tier,
            install_command: self.link.install_command,
            run_command: self.link.run_command,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HostedAdminSkillRecord {
    owner: String,
    #[serde(default)]
    trust_tier: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct HostedSkillPublishLink {
    install_command: String,
    run_command: String,
    #[serde(default)]
    public_url: Option<String>,
    #[serde(default)]
    link: Option<String>,
}

impl HostedSkillPublishLink {
    fn public_url(&self, skill_id: &str, version: &str) -> String {
        self.public_url
            .as_deref()
            .or(self
                .link
                .as_deref()
                .filter(|link| link.starts_with("http://") || link.starts_with("https://")))
            .map(str::to_owned)
            .unwrap_or_else(|| runx_skill_public_url(skill_id, version))
    }
}

fn runx_skill_public_url(skill_id: &str, version: &str) -> String {
    let (owner, name) = skill_id.split_once('/').unwrap_or(("", skill_id));
    format!(
        "https://runx.ai/x/{}/{}@{}",
        encode_path_component(owner),
        encode_path_component(name),
        encode_path_component(version)
    )
}

fn encode_path_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct HostedSkillPublishResult {
    pub(super) status: String,
    pub(super) skill_id: String,
    pub(super) owner: String,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) digest: String,
    #[serde(default)]
    pub(super) profile_digest: Option<String>,
    pub(super) trust_tier: String,
    pub(super) install_command: String,
    pub(super) run_command: String,
    pub(super) public_url: String,
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use runx_runtime::registry::{
        HttpMethod, HttpRequest, HttpResponse, RuntimeHttpError, Transport,
    };

    use super::*;

    #[test]
    fn remote_registry_publish_posts_skill_artifacts() -> Result<(), Box<dyn std::error::Error>> {
        let transport = StubTransport::new(HttpResponse {
            status: 201,
            body: serde_json::json!({
                "status": "success",
                "publish": {
                    "status": "published",
                    "skill_id": "kam/hello",
                    "owner": "kam",
                    "name": "hello",
                    "version": "sha-123",
                    "digest": "abc",
                    "trust_tier": "community",
                    "install_command": "runx add kam/hello@sha-123",
                    "run_command": "runx skill kam/hello@sha-123",
                    "public_url": "https://runx.test/x/kam/hello"
                }
            })
            .to_string(),
        });
        let package = SkillPackage {
            markdown: "---\nname: hello\nsource:\n  type: cli-tool\n  command: echo\n---\nHello.\n"
                .to_owned(),
            profile_document: Some("skill: hello\nversion: \"0.1.0\"\nrunners: {}\n".to_owned()),
            harness_path: None,
            harness_temp_dir: None,
            harness_fixture_paths: Vec::new(),
            package_files: vec![HostedSkillPackageFile {
                path: "run.mjs".to_owned(),
                content: "console.log('hello');\n".to_owned(),
            }],
        };

        let result = publish_remote_skill_package_with_transport(
            &transport,
            "https://runx.test/",
            "rxk_secret",
            Some("sha-123"),
            &package,
        )?;

        assert_eq!(result.skill_id, "kam/hello");
        let requests = transport.requests.borrow();
        assert_eq!(requests[0].method, HttpMethod::Post);
        assert_eq!(requests[0].url, "https://runx.test/v1/skills");
        assert!(requests[0].headers.iter().any(|header| {
            header.name == "authorization" && header.value == "Bearer rxk_secret"
        }));
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap_or_default())?;
        assert_eq!(body["markdown"], package.markdown);
        let profile_document = package
            .profile_document
            .as_deref()
            .ok_or("profile document missing")?;
        assert_eq!(body["profile_document"], profile_document);
        assert_eq!(body["version"], "sha-123");
        assert_eq!(body["package_files"][0]["path"], "run.mjs");
        assert_eq!(
            body["package_files"][0]["content"],
            "console.log('hello');\n"
        );
        Ok(())
    }

    #[test]
    fn remote_registry_publish_rejects_unsuccessful_2xx_envelope() {
        let transport = StubTransport::new(HttpResponse {
            status: 200,
            body: serde_json::json!({
                "status": "failure",
                "publish": {
                    "status": "rejected",
                    "skill_id": "kam/hello",
                    "owner": "kam",
                    "name": "hello",
                    "version": "sha-123",
                    "digest": "abc",
                    "trust_tier": "community",
                    "install_command": "runx add kam/hello@sha-123",
                    "run_command": "runx skill kam/hello@sha-123",
                    "public_url": "https://runx.test/x/kam/hello"
                }
            })
            .to_string(),
        });
        let package = SkillPackage {
            markdown: "---\nname: hello\nsource:\n  type: cli-tool\n  command: echo\n---\nHello.\n"
                .to_owned(),
            profile_document: None,
            harness_path: None,
            harness_temp_dir: None,
            harness_fixture_paths: Vec::new(),
            package_files: Vec::new(),
        };

        let error = publish_remote_skill_package_with_transport(
            &transport,
            "https://runx.test/",
            "rxk_secret",
            None,
            &package,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unsuccessful status"));
    }

    #[test]
    fn remote_registry_admin_publish_posts_owner_harness_and_upsert()
    -> Result<(), Box<dyn std::error::Error>> {
        let transport = StubTransport::new(HttpResponse {
            status: 200,
            body: serde_json::json!({
                "status": "success",
                "publish": {
                    "status": "published",
                    "skill_id": "runx/hello",
                    "name": "hello",
                    "version": "sha-123",
                    "digest": "abc",
                    "profile_digest": "profile-abc",
                    "link": {
                        "link": "runx://skill/runx%2Fhello@sha-123",
                        "install_command": "runx add runx/hello@sha-123",
                        "run_command": "runx skill runx/hello@sha-123"
                    },
                    "record": {
                        "owner": "runx",
                        "trust_tier": "first_party"
                    }
                },
                "harness": {
                    "status": "passed",
                    "case_count": 1,
                    "assertion_error_count": 0,
                    "assertion_errors": [],
                    "case_names": ["smoke"],
                    "receipt_ids": ["rx_harness_1"],
                    "graph_case_count": 0
                }
            })
            .to_string(),
        });
        let package = SkillPackage {
            markdown: "---\nname: hello\n---\nHello.\n".to_owned(),
            profile_document: Some("skill: hello\nrunners: {}\n".to_owned()),
            harness_path: None,
            harness_temp_dir: None,
            harness_fixture_paths: Vec::new(),
            package_files: vec![HostedSkillPackageFile {
                path: "run.mjs".to_owned(),
                content: "console.log('hello');\n".to_owned(),
            }],
        };
        let harness = RegistryPublishHarnessReport {
            status: "passed".to_owned(),
            case_count: 1,
            assertion_error_count: 0,
            assertion_errors: Vec::new(),
            case_names: vec!["smoke".to_owned()],
            receipt_ids: vec!["rx_harness_1".to_owned()],
            graph_case_count: 0,
        };

        let result = publish_remote_admin_skill_package_with_transport(
            &transport,
            "https://runx.test/",
            "admin-token",
            "runx",
            Some("sha-123"),
            true,
            &package,
            &harness,
        )?;

        assert_eq!(result.skill_id, "runx/hello");
        assert_eq!(result.owner, "runx");
        assert_eq!(result.trust_tier, "first_party");
        assert_eq!(result.public_url, "https://runx.ai/x/runx/hello@sha-123");
        let requests = transport.requests.borrow();
        assert_eq!(requests[0].method, HttpMethod::Post);
        assert_eq!(
            requests[0].url,
            "https://runx.test/v1/admin/registry/publish"
        );
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap_or_default())?;
        assert_eq!(body["owner"], "runx");
        assert_eq!(body["version"], "sha-123");
        assert_eq!(body["upsert"], true);
        assert_eq!(body["harness"]["status"], "passed");
        assert_eq!(body["package_files"][0]["path"], "run.mjs");
        Ok(())
    }

    struct StubTransport {
        requests: RefCell<Vec<HttpRequest>>,
        response: RefCell<Option<HttpResponse>>,
    }

    impl StubTransport {
        fn new(response: HttpResponse) -> Self {
            Self {
                requests: RefCell::new(Vec::new()),
                response: RefCell::new(Some(response)),
            }
        }
    }

    impl Transport for StubTransport {
        fn send(&self, request: HttpRequest) -> Result<HttpResponse, RuntimeHttpError> {
            self.requests.borrow_mut().push(request);
            self.response
                .borrow_mut()
                .take()
                .ok_or_else(|| RuntimeHttpError::Transport {
                    message: "missing stub response".to_owned(),
                })
        }
    }
}
