use std::sync::Arc;

use app_actions::Actions;
use config::{CmdList, ListConfig};

use super::CmdResult;

pub mod config;

#[allow(clippy::unnecessary_wraps)]
pub fn run(config: ListConfig) -> CmdResult {
    let actions = Actions::new(
        Arc::new(app_actions::ActionCtx::new(
            config.endpoint,
            config.dependency_paths,
            config.request,
        )),
        config.disabled_entries.entries,
    );

    match config.which {
        CmdList::Actions => list_actions(&actions),
        CmdList::Downloaders => list_downloaders(&actions),
        CmdList::Fixers => list_fixers(&actions),
        CmdList::All => list_all(&actions),
    }
    .into_iter()
    .for_each(|x| println!("{}", x));

    Ok(())
}

fn list_actions(actions: &Actions) -> Vec<String> {
    let mut v = vec![];

    v.push("Actions:".to_string());

    v.extend(
        actions
            .all_actions()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_downloaders(actions: &Actions) -> Vec<String> {
    let mut v = vec![];

    v.push("Downloaders:".to_string());

    v.extend(
        actions
            .all_downloaders()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_fixers(actions: &Actions) -> Vec<String> {
    let mut v = vec![];

    v.push("Fixers:".to_string());

    v.extend(
        actions
            .all_fixers()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_all(actions: &Actions) -> Vec<String> {
    let mut v = vec![];

    v.extend(list_actions(actions));
    v.push(String::new());
    v.extend(list_downloaders(actions));
    v.push(String::new());
    v.extend(list_fixers(actions));

    v
}
