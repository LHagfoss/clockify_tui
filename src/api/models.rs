use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub active_workspace: String,
    pub default_workspace: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Workspace {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub client_id: Option<String>,
    pub color: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeInterval {
    pub start: String,
    pub end: Option<String>,
    pub duration: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub id: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub time_interval: TimeInterval,
}
