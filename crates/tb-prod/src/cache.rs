use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::{ProductiveClient, Query};
use crate::error::{Result, TbProdError};

const TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile<T> {
    data: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub workflow_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedWorkflow {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedWorkflowStatus {
    pub id: String,
    pub name: String,
    pub workflow_id: String,
    pub color_id: String,
    pub category_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedPerson {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(org_id: &str) -> Result<Self> {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("tb-prod")
            .join(org_id);
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub async fn ensure_fresh(&self, client: &ProductiveClient) -> Result<()> {
        if self.is_stale("projects.json")
            || self.is_stale("workflows.json")
            || self.is_stale("workflow_statuses.json")
            || self.is_stale("people.json")
        {
            self.sync(client).await?;
        }
        Ok(())
    }

    pub async fn sync(&self, client: &ProductiveClient) -> Result<()> {
        eprintln!("Syncing cache...");

        let projects_q = Query::new().filter("status", "1").include("workflow");
        let workflows_q = Query::new();
        let statuses_q = Query::new().include("workflow");
        // Match frontend: status=active, person_type=1 (User), person_type!=3, hrm_type_id=1
        let people_q = Query::new()
            .filter_indexed(0, "status", "eq", "1")
            .filter_indexed(1, "person_type", "eq", "1")
            .filter_indexed(2, "person_type", "not_eq", "3")
            .filter_indexed(3, "hrm_type_id", "eq", "1")
            .filter_op("and");

        let (projects_resp, workflows_resp, statuses_resp, people_resp) = tokio::join!(
            client.get_all("/projects", &projects_q, 5),
            client.get_all("/workflows", &workflows_q, 5),
            client.get_all("/workflow_statuses", &statuses_q, 5),
            client.get_all("/people", &people_q, 5),
        );

        // Projects
        let projects: Vec<CachedProject> = projects_resp?
            .data
            .iter()
            .map(|r| CachedProject {
                id: r.id.clone(),
                name: r.attr_str("name").to_string(),
                workflow_id: r.relationship_id("workflow").unwrap_or("").to_string(),
            })
            .collect();
        self.write_cache("projects.json", &CacheFile { data: projects })?;

        // Workflows
        let workflows: Vec<CachedWorkflow> = workflows_resp?
            .data
            .iter()
            .map(|r| CachedWorkflow {
                id: r.id.clone(),
                name: r.attr_str("name").to_string(),
            })
            .collect();
        self.write_cache("workflows.json", &CacheFile { data: workflows })?;

        // Workflow statuses
        let statuses: Vec<CachedWorkflowStatus> = statuses_resp?
            .data
            .iter()
            .map(|r| CachedWorkflowStatus {
                id: r.id.clone(),
                name: r.attr_str("name").to_string(),
                workflow_id: r.relationship_id("workflow").unwrap_or("").to_string(),
                color_id: r.attributes.get("color_id").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_default(),
                category_id: r.attributes.get("category_id").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_default(),
            })
            .collect();
        self.write_cache("workflow_statuses.json", &CacheFile { data: statuses })?;

        // People
        let people: Vec<CachedPerson> = people_resp?
            .data
            .iter()
            .map(|r| CachedPerson {
                id: r.id.clone(),
                first_name: r.attr_str("first_name").to_string(),
                last_name: r.attr_str("last_name").to_string(),
                email: r.attr_str("email").to_string(),
            })
            .collect();
        self.write_cache("people.json", &CacheFile { data: people })?;

        eprintln!("Cache synced.");
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        for name in &["projects.json", "workflows.json", "workflow_statuses.json", "people.json"] {
            let path = self.dir.join(name);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        eprintln!("Cache cleared.");
        Ok(())
    }

    pub fn projects(&self) -> Result<Vec<CachedProject>> {
        self.read_cache("projects.json")
    }

    pub fn workflows(&self) -> Result<Vec<CachedWorkflow>> {
        self.read_cache("workflows.json")
    }

    pub fn workflow_statuses(&self) -> Result<Vec<CachedWorkflowStatus>> {
        self.read_cache("workflow_statuses.json")
    }

    pub fn people(&self) -> Result<Vec<CachedPerson>> {
        self.read_cache("people.json")
    }

    // --- Name resolution ---

    pub fn resolve_project(&self, name_or_id: &str) -> Result<String> {
        if name_or_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(name_or_id.to_string());
        }
        let projects = self.projects()?;
        let needle = name_or_id.to_lowercase();
        let matches: Vec<_> = projects.iter().filter(|p| p.name.to_lowercase().contains(&needle)).collect();
        match matches.len() {
            0 => {
                let available: Vec<_> = projects.iter().map(|p| format!("  {} ({})", p.name, p.id)).collect();
                Err(TbProdError::Other(format!(
                    "No project matching '{}'. Available:\n{}", name_or_id, available.join("\n")
                )))
            }
            1 => Ok(matches[0].id.clone()),
            _ => {
                let ambiguous: Vec<_> = matches.iter().map(|p| format!("  {} ({})", p.name, p.id)).collect();
                Err(TbProdError::Other(format!(
                    "Ambiguous project '{}'. Matches:\n{}", name_or_id, ambiguous.join("\n")
                )))
            }
        }
    }

    pub fn resolve_person(&self, name_or_id: &str) -> Result<String> {
        if name_or_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(name_or_id.to_string());
        }
        let people = self.people()?;
        let needle = name_or_id.to_lowercase();
        let matches: Vec<_> = people.iter().filter(|p| {
            let full_name = format!("{} {}", p.first_name, p.last_name).to_lowercase();
            full_name.contains(&needle) || p.email.to_lowercase().contains(&needle)
        }).collect();
        match matches.len() {
            0 => {
                let available: Vec<_> = people.iter().map(|p| format!("  {} {} ({})", p.first_name, p.last_name, p.id)).collect();
                Err(TbProdError::Other(format!(
                    "No person matching '{}'. Available:\n{}", name_or_id, available.join("\n")
                )))
            }
            1 => Ok(matches[0].id.clone()),
            _ => {
                let ambiguous: Vec<_> = matches.iter().map(|p| format!("  {} {} ({})", p.first_name, p.last_name, p.id)).collect();
                Err(TbProdError::Other(format!(
                    "Ambiguous person '{}'. Matches:\n{}", name_or_id, ambiguous.join("\n")
                )))
            }
        }
    }

    pub fn workflow_id_for_project(&self, project_id: &str) -> Result<Option<String>> {
        let projects = self.projects()?;
        Ok(projects
            .iter()
            .find(|p| p.id == project_id)
            .map(|p| &p.workflow_id)
            .filter(|wid| !wid.is_empty())
            .cloned())
    }

    pub fn resolve_workflow_status(&self, name_or_id: &str, workflow_id: Option<&str>) -> Result<String> {
        if name_or_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(name_or_id.to_string());
        }
        let statuses = self.workflow_statuses()?;
        let needle = name_or_id.to_lowercase();
        let matches: Vec<_> = statuses.iter().filter(|s| {
            let name_match = s.name.to_lowercase().contains(&needle);
            let wf_match = workflow_id.map_or(true, |wid| s.workflow_id == wid);
            name_match && wf_match
        }).collect();
        match matches.len() {
            0 => {
                let available: Vec<_> = statuses.iter()
                    .filter(|s| workflow_id.map_or(true, |wid| s.workflow_id == wid))
                    .map(|s| format!("  {} ({})", s.name, s.id))
                    .collect();
                Err(TbProdError::Other(format!(
                    "No workflow status matching '{}'. Available:\n{}", name_or_id, available.join("\n")
                )))
            }
            1 => Ok(matches[0].id.clone()),
            _ => {
                let ambiguous: Vec<_> = matches.iter().map(|s| format!("  {} ({})", s.name, s.id)).collect();
                Err(TbProdError::Other(format!(
                    "Ambiguous status '{}'. Matches:\n{}", name_or_id, ambiguous.join("\n")
                )))
            }
        }
    }

    // --- Internal helpers ---

    fn is_stale(&self, filename: &str) -> bool {
        let path = self.dir.join(filename);
        match path.metadata().and_then(|m| m.modified()) {
            Ok(modified) => {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                age.as_secs() > TTL_SECS
            }
            Err(_) => true,
        }
    }

    fn write_cache<T: Serialize>(&self, filename: &str, data: &T) -> Result<()> {
        let path = self.dir.join(filename);
        let json = serde_json::to_string_pretty(data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    fn read_cache<T: for<'de> Deserialize<'de>>(&self, filename: &str) -> Result<Vec<T>> {
        let path = self.dir.join(filename);
        let content = std::fs::read_to_string(&path).map_err(|_| {
            TbProdError::Other(format!(
                "Cache file '{}' not found. Run `tb-prod cache sync` first.", filename
            ))
        })?;
        let cache: CacheFile<T> = serde_json::from_str(&content)?;
        Ok(cache.data)
    }
}

/// Resolve a task list by name or ID. Queries the API live (not cached).
pub async fn resolve_task_list(
    client: &ProductiveClient,
    name_or_id: &str,
    project_id: Option<&str>,
) -> Result<String> {
    if name_or_id.chars().all(|c| c.is_ascii_digit()) {
        return Ok(name_or_id.to_string());
    }
    let mut query = Query::new().filter("query", name_or_id);
    if let Some(pid) = project_id {
        query = query.filter_array("project_id", pid);
    }
    let resp = client.list_task_lists(&query).await?;
    match resp.data.len() {
        0 => Err(TbProdError::Other(format!("No task list matching '{}'", name_or_id))),
        1 => Ok(resp.data[0].id.clone()),
        _ => {
            let ambiguous: Vec<_> = resp.data.iter().map(|r| format!("  {} ({})", r.attr_str("name"), r.id)).collect();
            Err(TbProdError::Other(format!(
                "Ambiguous task list '{}'. Matches:\n{}", name_or_id, ambiguous.join("\n")
            )))
        }
    }
}
