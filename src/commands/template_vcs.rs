use crate::utils::{print as p, template_vcs};
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum TemplateVcsCommands {
    /// Initialize version control for a template directory
    Init {
        /// Path to the template directory
        path: PathBuf,
        /// Template name
        #[arg(long)]
        name: String,
    },
    /// Commit a new version of the template
    Commit {
        /// Path to the template directory
        path: PathBuf,
        /// Version number (semver, e.g. "1.0.0")
        version: String,
        /// Commit message describing changes
        message: String,
        /// Author name
        #[arg(long)]
        author: Option<String>,
    },
    /// Create a new branch for template development
    Branch {
        /// Path to the template directory
        path: PathBuf,
        /// Branch name to create
        name: Option<String>,
        /// Switch to this branch after creation
        #[arg(long)]
        checkout: Option<String>,
    },
    /// Show version history
    Log {
        /// Path to the template directory
        path: PathBuf,
        /// Number of versions to show
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Show uncommitted changes
    Diff {
        /// Path to the template directory
        path: PathBuf,
    },
    /// Create a release (tag + changelog)
    Release {
        /// Path to the template directory
        path: PathBuf,
        /// Version number (semver)
        version: String,
        /// Release notes
        message: String,
        /// Author name
        #[arg(long)]
        author: Option<String>,
    },
    /// Generate or view the changelog
    Changelog {
        /// Path to the template directory
        path: PathBuf,
    },
    /// Show VCS status
    Status {
        /// Path to the template directory
        path: PathBuf,
    },
    /// Initialize a collaborative editing session for a template
    Collaborate {
        /// Path to the template directory
        path: PathBuf,
        /// Optional template display name
        #[arg(long)]
        name: Option<String>,
        /// Participant identifiers to seed the session
        #[arg(long)]
        participant: Vec<String>,
    },
    /// Generate AI-assisted review suggestions for a template
    Review {
        /// Path to the template directory
        path: PathBuf,
        /// Optional focus area for the review
        #[arg(long)]
        focus: Option<String>,
    },
    /// Detect merge-conflict markers in a template tree
    ResolveConflicts {
        /// Path to the template directory
        path: PathBuf,
    },
    /// Share a knowledge note with the template team
    ShareKnowledge {
        /// Path to the template directory
        path: PathBuf,
        /// Knowledge title
        title: String,
        /// Knowledge content
        content: String,
        /// Author name
        #[arg(long)]
        author: Option<String>,
    },
    /// Show collaboration analytics for a template
    Analytics {
        /// Path to the template directory
        path: PathBuf,
    },
}

pub async fn handle(cmd: TemplateVcsCommands) -> Result<()> {
    match cmd {
        TemplateVcsCommands::Init { path, name } => init(path, name),
        TemplateVcsCommands::Commit {
            path,
            version,
            message,
            author,
        } => commit(path, version, message, author),
        TemplateVcsCommands::Branch {
            path,
            name,
            checkout,
        } => branch(path, name, checkout),
        TemplateVcsCommands::Log { path, limit } => log(path, limit),
        TemplateVcsCommands::Diff { path } => diff(path),
        TemplateVcsCommands::Release {
            path,
            version,
            message,
            author,
        } => release(path, version, message, author),
        TemplateVcsCommands::Changelog { path } => changelog(path),
        TemplateVcsCommands::Status { path } => status(path),
        TemplateVcsCommands::Collaborate {
            path,
            name,
            participant,
        } => collaborate(path, name, participant),
        TemplateVcsCommands::Review { path, focus } => review(path, focus),
        TemplateVcsCommands::ResolveConflicts { path } => resolve_conflicts(path),
        TemplateVcsCommands::ShareKnowledge {
            path,
            title,
            content,
            author,
        } => share_knowledge(path, title, content, author),
        TemplateVcsCommands::Analytics { path } => analytics(path),
    }
}

fn init(path: PathBuf, name: String) -> Result<()> {
    p::header("Template Version Control — Init");
    p::step(1, 2, "Initializing git repository...");
    template_vcs::init_vcs(&path, &name)?;

    p::step(2, 2, "Creating version tracking...");
    println!();
    p::success(&format!("Version control initialized for '{}'", name));
    p::kv("Path", &path.display().to_string());
    p::info("Use `starforge template-vcs commit` to record versions.");
    Ok(())
}

fn commit(path: PathBuf, version: String, message: String, author: Option<String>) -> Result<()> {
    p::header("Template Version Control — Commit");
    let author_name = author.unwrap_or_else(|| "Anonymous".to_string());

    p::step(1, 2, &format!("Recording version {}...", version));
    let entry = template_vcs::commit_version(&path, &version, &message, &author_name)?;

    p::step(2, 2, "Updating changelog...");
    println!();
    p::success(&format!("Version {} committed", entry.tag));
    p::kv("Version", &entry.version);
    p::kv("Tag", &entry.tag);
    p::kv("Author", &entry.author);
    p::kv("Changes", &entry.message);
    Ok(())
}

fn branch(path: PathBuf, name: Option<String>, checkout: Option<String>) -> Result<()> {
    if let Some(branch_name) = checkout {
        p::header("Template Version Control — Switch Branch");
        template_vcs::switch_branch(&path, &branch_name)?;
        p::success(&format!("Switched to branch '{}'", branch_name));
        return Ok(());
    }

    if let Some(branch_name) = name {
        p::header("Template Version Control — Create Branch");
        template_vcs::create_branch(&path, &branch_name)?;
        p::success(&format!("Branch '{}' created", branch_name));
        return Ok(());
    }

    p::header("Template Version Control — Branches");
    let branches = template_vcs::list_branches(&path)?;

    if branches.is_empty() {
        p::info("No branches found. Initialize VCS first.");
        return Ok(());
    }

    for branch in &branches {
        let marker = if branch.current { "* " } else { "  " };
        println!(
            "{}{} {} {}",
            marker, branch.name, branch.last_commit, branch.last_message
        );
    }
    Ok(())
}

fn log(path: PathBuf, limit: usize) -> Result<()> {
    p::header("Template Version Control — Log");
    let versions = template_vcs::view_log(&path, limit)?;

    if versions.is_empty() {
        p::info("No version history found. Commit a version first.");
        return Ok(());
    }

    for version in &versions {
        println!(
            "  {} ({}) — {}",
            version.tag,
            &version.timestamp[..10],
            version.message.lines().next().unwrap_or("")
        );
        p::kv("Author", &version.author);
        if version.changes.len() > 1 {
            for change in &version.changes {
                println!("    - {}", change);
            }
        }
        println!();
    }

    p::kv("Showing", &format!("{} versions", versions.len()));
    Ok(())
}

fn diff(path: PathBuf) -> Result<()> {
    p::header("Template Version Control — Diff");
    let diff_output = template_vcs::show_diff(&path)?;

    if diff_output.trim().is_empty() {
        p::info("No changes detected.");
    } else {
        println!("{}", diff_output);
    }
    Ok(())
}

fn release(path: PathBuf, version: String, message: String, author: Option<String>) -> Result<()> {
    p::header("Template Version Control — Release");
    let author_name = author.unwrap_or_else(|| "Anonymous".to_string());

    p::step(1, 2, &format!("Creating release {}...", version));
    let entry = template_vcs::create_release(&path, &version, &message, &author_name)?;

    p::step(2, 2, "Generating changelog...");
    template_vcs::generate_changelog(&path)?;

    println!();
    p::success(&format!("Release {} created", entry.tag));
    p::kv("Version", &entry.version);
    p::kv("Tag", &entry.tag);
    p::kv("Author", &entry.author);
    Ok(())
}

fn changelog(path: PathBuf) -> Result<()> {
    p::header("Template Version Control — Changelog");
    let content = template_vcs::generate_changelog(&path)?;
    println!("{}", content);
    Ok(())
}

fn status(path: PathBuf) -> Result<()> {
    p::header("Template Version Control — Status");
    p::kv("Path", &path.display().to_string());

    let versions = template_vcs::get_version_history(&path)?;
    p::kv("Versions", &versions.versions.len().to_string());

    if !versions.versions.is_empty() {
        if let Some(latest) = versions
            .versions
            .iter()
            .max_by(|a, b| a.version.cmp(&b.version))
        {
            p::kv("Latest", &latest.version);
        }
    }

    let branches = template_vcs::list_branches(&path).unwrap_or_default();
    p::kv("Branches", &branches.len().to_string());

    let diff_output = template_vcs::show_diff(&path).unwrap_or_default();
    let has_changes = !diff_output.trim().is_empty();
    p::kv("Uncommitted", if has_changes { "Yes" } else { "No" });

    println!();
    p::info("Use `starforge template-vcs commit` to record changes.");
    Ok(())
}

fn collaborate(path: PathBuf, name: Option<String>, participants: Vec<String>) -> Result<()> {
    p::header("Template Collaboration — Session");
    let template_name = name.unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());
    let session = template_vcs::init_collaboration(&path, &template_name, &participants)?;
    p::success("Collaboration session created");
    p::kv("Session ID", &session.id);
    p::kv("Template", &template_name);
    p::kv("Participants", &session.participants.join(", "));
    Ok(())
}

fn review(path: PathBuf, focus: Option<String>) -> Result<()> {
    p::header("Template Collaboration — AI Review");
    let suggestions = template_vcs::generate_ai_review_suggestions(&path, focus.as_deref())?;

    if suggestions.is_empty() {
        p::info("No review suggestions generated.");
        return Ok(());
    }

    for suggestion in suggestions {
        println!();
        p::kv_accent("Suggestion", &suggestion.title);
        p::kv("Severity", &suggestion.severity);
        p::kv("Summary", &suggestion.summary);
        if let Some(file) = suggestion.file_path {
            p::kv("File", &file);
        }
    }

    Ok(())
}

fn resolve_conflicts(path: PathBuf) -> Result<()> {
    p::header("Template Collaboration — Conflict Resolution");
    let resolutions = template_vcs::resolve_template_conflicts(&path)?;

    if resolutions.is_empty() {
        p::info("No conflict markers found.");
        return Ok(());
    }

    for resolution in resolutions {
        println!();
        p::kv_accent("File", &resolution.file_path);
        p::kv("Resolved", if resolution.resolved { "Yes" } else { "No" });
        p::kv("Recommendation", &resolution.recommendation);
        if !resolution.conflicts.is_empty() {
            p::kv("Conflict markers", &resolution.conflicts.join(" | "));
        }
    }

    Ok(())
}

fn share_knowledge(path: PathBuf, title: String, content: String, author: Option<String>) -> Result<()> {
    p::header("Template Collaboration — Knowledge Sharing");
    let author_name = author.unwrap_or_else(|| "anonymous".to_string());
    let entry = template_vcs::share_knowledge(&path, &title, &content, &author_name)?;
    p::success("Knowledge shared with the team");
    p::kv("Entry ID", &entry.id);
    p::kv("Title", &entry.title);
    p::kv("Author", &entry.author);
    Ok(())
}

fn analytics(path: PathBuf) -> Result<()> {
    p::header("Template Collaboration — Analytics");
    let analytics = template_vcs::collect_team_analytics(&path)?;
    p::kv("Template", &analytics.template_name);
    p::kv("Participants", &analytics.participant_count.to_string());
    p::kv("Contributions", &analytics.contribution_count.to_string());
    p::kv("Review suggestions", &analytics.review_suggestion_count.to_string());
    p::kv("Knowledge entries", &analytics.knowledge_entry_count.to_string());
    p::kv("Last activity", &analytics.last_activity);
    Ok(())
}
