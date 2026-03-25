use std::path::Path;
use std::process::Command;

use crate::config::{Config, ServiceConfig};
use crate::error::{Error, Result};

/// Default runtime versions — must match Dockerfile.base ARGs.
const DEFAULT_RUBY: &str = "3.4.7";
const DEFAULT_NODE: &str = "22.16.0";

/// Generate a Procfile for overmind from the selected services.
/// Writes to `.docker-sessions/.dev/Procfile.dev`.
pub fn generate_procfile(config: &Config, services: &[String], project_root: &Path) -> Result<()> {
    let procfile_dir = project_root.join(".docker-sessions/.dev");
    std::fs::create_dir_all(&procfile_dir)?;
    let procfile_path = procfile_dir.join("Procfile.dev");

    let mut lines = Vec::new();

    for svc_name in services {
        let svc = config
            .services
            .get(svc_name)
            .ok_or_else(|| Error::Config(format!("Unknown service: {}", svc_name)))?;

        if let Some(entry) = procfile_entry(svc_name, svc, project_root) {
            lines.push(entry);
        }

        // Add companion (e.g., sidekiq for api)
        if let Some(companion) = &svc.companion
            && let Some(comp_svc) = config.services.get(companion)
            && let Some(entry) = procfile_entry(companion, comp_svc, project_root)
        {
            lines.push(entry);
        }
    }

    std::fs::write(&procfile_path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Build a single Procfile entry, with runtime version wrappers if needed.
fn procfile_entry(name: &str, svc: &ServiceConfig, project_root: &Path) -> Option<String> {
    let repo = svc.repo.as_deref()?;
    let cmd = svc.cmd.as_deref()?;

    let repos_dir = project_root.join("repos");
    let mut wrapper = String::new();

    // Check if repo needs a different Ruby version
    let ruby_version_file = repos_dir.join(repo).join(".ruby-version");
    if ruby_version_file.exists()
        && let Ok(version) = std::fs::read_to_string(&ruby_version_file)
    {
        let version = version.trim();
        if version != DEFAULT_RUBY {
            wrapper.push_str(&format!("rvm use {} && ", version));
        }
    }

    // Check if repo needs a different Node version
    let node_version = read_node_version(&repos_dir.join(repo));
    if let Some(version) = node_version
        && version != DEFAULT_NODE
    {
        wrapper.push_str(&format!(
            ". /usr/local/nvm/nvm.sh && nvm use {} && ",
            version
        ));
    }

    let full_cmd = if wrapper.is_empty() {
        format!("{}: cd /workspace/{} && {}", name, repo, cmd)
    } else {
        format!(
            "{}: bash -lc '{} cd /workspace/{} && {}'",
            name, wrapper, repo, cmd
        )
    };

    Some(full_cmd)
}

/// Read Node version from .node-version or .nvmrc
fn read_node_version(repo_path: &Path) -> Option<String> {
    for filename in &[".node-version", ".nvmrc"] {
        let path = repo_path.join(filename);
        if path.exists()
            && let Ok(version) = std::fs::read_to_string(&path)
        {
            return Some(version.trim().to_string());
        }
    }
    None
}

/// Query overmind inside the container to get running service names and their status.
/// Returns a map of service_name → "running" | "stopped" | "dead".
pub fn overmind_status(config: &Config) -> std::collections::BTreeMap<String, String> {
    let mut result = std::collections::BTreeMap::new();

    let output = Command::new("docker")
        .args(["exec", &config.docker.container, "overmind", "status"])
        .output();

    let Ok(output) = output else {
        return result;
    };

    // overmind status output:
    // PROCESS   PID       STATUS
    // api       5796      running
    // sidekiq   5797      running
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let name = parts[0].to_string();
            let status = parts[2].to_string();
            result.insert(name, status);
        }
    }

    result
}

/// Generate a docker-compose.yml with only the ports and mounts needed
/// for the requested services. Written to `.docker-sessions/.dev/docker-compose.yml`.
pub fn generate_compose(
    config: &Config,
    services: &[String],
    project_root: &Path,
) -> Result<std::path::PathBuf> {
    let compose_dir = project_root.join(".docker-sessions/.dev");
    std::fs::create_dir_all(&compose_dir)?;
    let compose_path = compose_dir.join("docker-compose.yml");

    // Collect ports and repo names for selected services (+ companions)
    let mut ports = Vec::new();
    let mut selected_repos = Vec::new();

    for svc_name in services {
        if let Some(svc) = config.services.get(svc_name) {
            if let Some(port) = svc.port {
                ports.push(port);
            }
            if let Some(repo) = &svc.repo
                && !selected_repos.contains(repo)
            {
                selected_repos.push(repo.clone());
            }
            // Include companion
            if let Some(companion) = &svc.companion
                && let Some(comp) = config.services.get(companion)
            {
                if let Some(port) = comp.port {
                    ports.push(port);
                }
                if let Some(repo) = &comp.repo
                    && !selected_repos.contains(repo)
                {
                    selected_repos.push(repo.clone());
                }
            }
        }
    }

    // Service discovery env vars — always include all services so inter-service
    // communication works regardless of which are running
    let mut service_urls = Vec::new();
    for (name, svc) in &config.services {
        if let (Some(hostname), Some(port)) = (&svc.hostname, svc.port) {
            let env_key = format!("{}_SERVICE_URL", name.to_uppercase().replace('-', "_"));
            service_urls.push(format!("      - {}=http://{}:{}", env_key, hostname, port));
        }
    }

    // Build ports section
    let ports_yaml: Vec<String> = ports
        .iter()
        .map(|p| format!("      - \"{}:{}\"", p, p))
        .collect();

    // Build volume mounts — only for repos we need, plus always mount all
    // (Docker creates empty dirs for unmounted repos, harmless)
    let docker_dir = project_root.join("docker");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());

    let mut content = format!(
        r#"# Generated by devctl — do not edit manually
services:
  workspace:
    image: productive-dev:base
    container_name: {container}
    hostname: productive-dev
    env_file:
      - .env.session
    environment:
      - SESSION_NAME=productive-dev
      - SESSION_MODE=dev
      - SELECTED_REPOS={selected_repos}
      - MYSQL_HOST=productive-dev-mysql
      - MYSQL_PORT=3306
      - MYSQL_USER=root
      - MYSQL_PASSWORD=
      - REDIS_URL=redis://productive-dev-redis:6379/0
      - redis_host=productive-dev-redis
      - MEILISEARCH_URL=http://productive-dev-meilisearch:7700
      - MEMCACHE_SERVERS=productive-dev-memcached:11211
      - cache_url=productive-dev-memcached:11211
      - db_host=productive-dev-mysql
      - RAISE_ON_MISSING_FLAGS=false
      - RAISE_ON_MISSING_FEATURES=false
      - RAILS_ENV=development
      - NODE_ENV=development
      - COREPACK_ENABLE_DOWNLOAD_PROMPT=0
      - COREPACK_ENABLE_AUTO_PIN=0
      - CI=true
      - PRODUCTIVE_API_BASE_URL=http://api.productive.io.localhost:3000
{service_urls}
    ports:
{ports}
    volumes:
"#,
        container = config.docker.container,
        selected_repos = selected_repos.join(","),
        service_urls = service_urls.join("\n"),
        ports = ports_yaml.join("\n"),
    );

    // Mount all repos (static, same as dev-compose.yml)
    for svc in config.services.values() {
        if let Some(repo) = &svc.repo {
            content.push_str(&format!(
                "      - {}/repos/{}:/workspace/{}\n",
                project_root.display(),
                repo,
                repo
            ));
        }
    }

    // Procfile, entrypoint, config, AWS
    content.push_str(&format!(
        r#"      - {compose_dir}/Procfile.dev:/workspace/Procfile.dev
      - {docker_dir}/entrypoint.sh:/entrypoint.sh:ro
      - {docker_dir}/config:/docker-config:ro
      - {home}/.aws:/home/dev/.aws:ro
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - DAC_OVERRIDE
      - FOWNER
      - SETGID
      - SETUID
      - NET_BIND_SERVICE
    deploy:
      resources:
        limits:
          memory: 8G
          cpus: "4"
          pids: 2048
    healthcheck:
      test: ["CMD", "test", "-f", "/tmp/.session-ready"]
      interval: 10s
      timeout: 5s
      retries: 60
      start_period: 120s
    stdin_open: true
    tty: true
    networks:
      - productive-dev-net

networks:
  productive-dev-net:
    external: true
"#,
        compose_dir = compose_dir.display(),
        docker_dir = docker_dir.display(),
        home = home,
    ));

    std::fs::write(&compose_path, content)?;
    Ok(compose_path)
}

/// Check if the dev container is currently running.
pub fn container_is_running(config: &Config) -> bool {
    // Use ^name$ anchor for exact match (docker filter does substring by default)
    Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("name=^{}$", config.docker.container),
            "--filter",
            "status=running",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty())
}

/// Stop the dev container.
pub fn stop_container(config: &Config, project_root: &Path) -> Result<()> {
    let compose_file = generated_compose_path(project_root);
    // Fall back to static compose if generated doesn't exist
    let compose_file = if compose_file.exists() {
        compose_file
    } else {
        project_root.join(&config.docker.compose_file)
    };

    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.docker.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "down",
        ])
        .status()?;

    if !status.success() {
        return Err(Error::Other("Failed to stop dev container".into()));
    }
    Ok(())
}

fn generated_compose_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".docker-sessions/.dev/docker-compose.yml")
}

/// Start the dev container using the generated compose file.
pub fn start_container(config: &Config, project_root: &Path, services: &[String]) -> Result<()> {
    // Generate compose with only the needed ports
    let compose_file = generate_compose(config, services, project_root)?;

    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.docker.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "up",
            "-d",
        ])
        .status()?;
    if !status.success() {
        return Err(Error::Other("Failed to start dev container".into()));
    }
    Ok(())
}

/// Wait for the container healthcheck to pass.
/// Timeout: 10 minutes (first-time setup may compile Ruby/Node from source).
pub fn wait_for_healthy(config: &Config) -> Result<()> {
    let container = &config.docker.container;
    for i in 0..300 {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Health.Status}}", container])
            .output()?;

        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if status == "healthy" {
            return Ok(());
        }

        if i % 15 == 0 && i > 0 {
            eprint!(" {}s", i * 2);
        } else if i % 5 == 0 {
            eprint!(".");
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(Error::Other(
        "Container did not become healthy within 10 minutes".into(),
    ))
}
