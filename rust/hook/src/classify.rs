//! Mirror of Python classify.py: first matching company wins, fallback to default.

use crate::config::Companies;

pub fn classify(
    cfg: &Companies,
    project_path: Option<&str>,
    jira_issue: Option<&str>,
) -> Option<String> {
    for c in &cfg.companies {
        if let Some(p) = project_path {
            if c.path_prefixes.iter().any(|prefix| p.starts_with(prefix)) {
                return Some(c.name.clone());
            }
        }
        if let Some(issue) = jira_issue {
            if let Some((project_key, _)) = issue.split_once('-') {
                if c.jira_projects.iter().any(|k| k == project_key) {
                    return Some(c.name.clone());
                }
            }
        }
    }
    cfg.default_company.clone()
}
