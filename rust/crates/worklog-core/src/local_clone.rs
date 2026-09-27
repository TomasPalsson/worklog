//! Repo → local folder and sha-presence checks for org commits/PRs (spec
//! 006, D-07). Calls `billing::work_folder_for_path` and its submodule map
//! rather than re-deriving folder mapping. Populated by T003:
//! `sha_is_local`, `folder_for_repo`.
