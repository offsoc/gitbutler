use crate::command::discard_change::IndicesOrHeaders;
use crate::command::{debug_print, indices_or_headers_to_hunk_headers, path_to_rela_path};
use anyhow::bail;
use but_core::TreeChange;
use but_workspace::commit_engine::{
    DiffSpec, ReferenceFrame, StackSegmentId, create_commit_and_update_refs,
};
use gitbutler_project::Project;
use gitbutler_stack::{VirtualBranchesHandle, VirtualBranchesState};
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub fn commit(
    repo: gix::Repository,
    project: Option<Project>,
    message: Option<&str>,
    amend: bool,
    parent_revspec: Option<&str>,
    stack_segment_ref: Option<&str>,
    workspace_tip: Option<&str>,
    current_rela_path: Option<&Path>,
    previous_rela_path: Option<&Path>,
    headers: Option<&[u32]>,
    diff_spec: Option<Vec<DiffSpec>>,
) -> anyhow::Result<()> {
    if message.is_none() && !amend {
        bail!("Need a message when creating a new commit");
    }

    let parent_id = parent_revspec
        .map(|revspec| repo.rev_parse_single(revspec).map_err(anyhow::Error::from))
        .map(|id| id.map(|id| id.detach()))
        .transpose()?;

    let changes = match (current_rela_path, previous_rela_path, headers, diff_spec) {
        (None, None, None, Some(diff_spec)) => diff_spec,
        (None, None, None, None) => {
            to_whole_file_diffspec(but_core::diff::worktree_changes(&repo)?.changes)
        }
        (Some(current_path), previous_path, Some(headers), None) => {
            let path = path_to_rela_path(current_path)?;
            let previous_path = previous_path.map(path_to_rela_path).transpose()?;
            let hunk_headers = indices_or_headers_to_hunk_headers(
                &repo,
                Some(IndicesOrHeaders::Headers(headers)),
                &path,
                previous_path.as_ref(),
            )?;

            vec![DiffSpec {
                previous_path,
                path,
                hunk_headers,
            }]
        }
        _ => unreachable!("BUG: specifying this shouldn't be possible"),
    };
    if let Some(project) = project.as_ref() {
        let destination = if amend {
            if message.is_some() {
                bail!("Messages aren't used when amending");
            }
            let parent_id = parent_id.unwrap_or(repo.head_id()?.detach());
            but_workspace::commit_engine::Destination::AmendCommit(parent_id)
        } else {
            let (stack_segment, parent_commit_id) =
                get_stack_segment_info(&repo, stack_segment_ref, parent_id, project)?;

            but_workspace::commit_engine::Destination::NewCommit {
                parent_commit_id,
                message: message.unwrap_or_default().to_owned(),
                stack_segment,
            }
        };
        let mut guard = project.exclusive_worktree_access();
        debug_print(
            but_workspace::commit_engine::create_commit_and_update_refs_with_project(
                &repo,
                project,
                None,
                destination,
                None,
                changes,
                0, /* context-lines */
                guard.write_permission(),
            )?,
        )?;
    } else {
        let destination = if amend {
            if message.is_some() {
                bail!("Messages aren't used when amending");
            }
            let parent_id = parent_id.unwrap_or(repo.head_id()?.detach());
            but_workspace::commit_engine::Destination::AmendCommit(parent_id)
        } else {
            but_workspace::commit_engine::Destination::NewCommit {
                parent_commit_id: parent_id,
                message: message.unwrap_or_default().to_owned(),
                stack_segment: None,
            }
        };
        debug_print(create_commit_and_update_refs(
            &repo,
            ReferenceFrame {
                workspace_tip: workspace_tip
                    .map(|spec| repo.rev_parse_single(spec))
                    .transpose()?
                    .map(|id| id.detach()),
                branch_tip: Some(
                    stack_segment_ref
                        .map(|name| repo.find_reference(name).map(|r| r.id().detach()))
                        .transpose()?
                        .unwrap_or(repo.head_id()?.detach()),
                ),
            },
            &mut VirtualBranchesState::default(),
            destination,
            None,
            changes,
            0,
        )?)?;
    }
    Ok(())
}

/// Determines the target stack segment (branch) and the target parent commit ID based on the provided `stack_segment_ref` and `parent_id`.
///
/// If a branch is provided is provided:
/// - Normalizes the reference and attempts to find the corresponding stack segment in the workspace.
/// - Constructs a `StackSegmentId` if a matching stack is found.
/// - Determines the parent commit ID:
///   - Uses the provided `parent_id` if defined.
///   - Otherwise, tries to find the branch reference in the repository and peels it to a commit to get its ID.
///
/// If a branch is not provided:
/// - Returns `None` for the stack segment and uses the provided `parent_id`.
///
/// Returns a tuple containing:
/// - An `Option<StackSegmentId>` representing the target stack segment (if found).
/// - An `Option<CommitId>` representing the target parent commit ID (if found).
fn get_stack_segment_info(
    repo: &gix::Repository,
    stack_segment_ref: Option<&str>,
    parent_id: Option<gix::ObjectId>,
    project: &Project,
) -> Result<(Option<StackSegmentId>, Option<gix::ObjectId>), anyhow::Error> {
    let (stack_segment, parent_commit_id) = if let Some(stack_segment_ref) = stack_segment_ref {
        let full_name = normalize_stack_segment_ref(stack_segment_ref)?;
        let stack_segment = VirtualBranchesHandle::new(project.gb_dir())
            .list_stacks_in_workspace()?
            .iter()
            .find(|s| s.heads(false).contains(&stack_segment_ref.to_string()))
            .map(|s| s.id)
            .map(|id| StackSegmentId {
                segment_ref: full_name,
                stack_id: id,
            });

        let parent_commit_id = match parent_id {
            Some(id) => Some(id),
            None => {
                let reference = repo
                    .try_find_reference(stack_segment_ref)
                    .map_err(anyhow::Error::from)?;
                if let Some(mut r) = reference {
                    Some(r.peel_to_commit().map_err(anyhow::Error::from)?.id)
                } else {
                    None
                }
            }
        };

        (stack_segment, parent_commit_id)
    } else {
        (None, parent_id)
    };
    Ok((stack_segment, parent_commit_id))
}

fn normalize_stack_segment_ref(
    stack_segment_ref: &str,
) -> Result<gix::refs::FullName, gix::refs::name::Error> {
    let full_name = if stack_segment_ref.starts_with("refs/heads/") {
        stack_segment_ref.to_string()
    } else {
        format!("refs/heads/{}", stack_segment_ref)
    };
    gix::refs::FullName::try_from(full_name)
}

fn to_whole_file_diffspec(changes: Vec<TreeChange>) -> Vec<DiffSpec> {
    changes
        .into_iter()
        .map(|change| DiffSpec {
            previous_path: change.previous_path().map(ToOwned::to_owned),
            path: change.path,
            hunk_headers: Vec::new(),
        })
        .collect()
}
