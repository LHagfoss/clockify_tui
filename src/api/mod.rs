pub mod models;

use models::{Project, TimeEntry, User, Workspace};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::json;

pub struct Client {
    http_client: reqwest::Client,
}

impl Client {
    pub fn new(api_key: String) -> Self {
        let mut headers = HeaderMap::new();
        if let Ok(mut val) = HeaderValue::from_str(&api_key) {
            val.set_sensitive(true);
            headers.insert("X-Api-Key", val);
        }
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            http_client,
        }
    }

    pub async fn get_user(&self) -> Result<User, reqwest::Error> {
        let url = "https://api.clockify.me/api/v1/user";
        self.http_client.get(url).send().await?.json::<User>().await
    }

    pub async fn get_workspaces(&self) -> Result<Vec<Workspace>, reqwest::Error> {
        let url = "https://api.clockify.me/api/v1/workspaces";
        self.http_client.get(url).send().await?.json::<Vec<Workspace>>().await
    }

    pub async fn get_projects(&self, workspace_id: &str) -> Result<Vec<Project>, reqwest::Error> {
        let url = format!("https://api.clockify.me/api/v1/workspaces/{}/projects?page-size=1000", workspace_id);
        self.http_client.get(&url).send().await?.json::<Vec<Project>>().await
    }

    pub async fn get_time_entries(
        &self,
        workspace_id: &str,
        user_id: &str,
        start_date: &str, // e.g. "2026-06-12T00:00:00Z"
        end_date: &str,   // e.g. "2026-06-26T23:59:59Z"
    ) -> Result<Vec<TimeEntry>, reqwest::Error> {
        let url = format!(
            "https://api.clockify.me/api/v1/workspaces/{}/user/{}/time-entries?start={}&end={}&page-size=1000",
            workspace_id, user_id, start_date, end_date
        );
        self.http_client.get(&url).send().await?.json::<Vec<TimeEntry>>().await
    }

    pub async fn start_time_entry(
        &self,
        workspace_id: &str,
        project_id: Option<&str>,
        description: &str,
    ) -> Result<TimeEntry, reqwest::Error> {
        let url = format!("https://api.clockify.me/api/v1/workspaces/{}/time-entries", workspace_id);

        let mut body = json!({
            "start": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "description": description,
        });

        if let Some(p_id) = project_id {
            body.as_object_mut().unwrap().insert("projectId".to_string(), json!(p_id));
        }

        self.http_client.post(&url).json(&body).send().await?.json::<TimeEntry>().await
    }

    pub async fn create_time_entry(
        &self,
        workspace_id: &str,
        project_id: Option<&str>,
        description: &str,
        start: &str,
        end: &str,
    ) -> Result<TimeEntry, reqwest::Error> {
        let url = format!("https://api.clockify.me/api/v1/workspaces/{}/time-entries", workspace_id);

        let mut body = json!({
            "start": start,
            "end": end,
            "description": description,
        });

        if let Some(p_id) = project_id {
            body.as_object_mut().unwrap().insert("projectId".to_string(), json!(p_id));
        }

        self.http_client.post(&url).json(&body).send().await?.json::<TimeEntry>().await
    }

    pub async fn stop_time_entry(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("https://api.clockify.me/api/v1/workspaces/{}/user/{}/time-entries", workspace_id, user_id);
        let body = json!({
            "end": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });

        self.http_client.patch(&url).json(&body).send().await?;
        Ok(())
    }
}
