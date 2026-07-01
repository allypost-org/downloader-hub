use config::{CmdList, ListConfig};

use super::CmdResult;

pub mod config;

pub fn run(config: ListConfig) -> CmdResult {
    app_actions::config::init(
        config.endpoint,
        config.dependency_paths,
        config.disabled_entries.entries,
        config.request,
    )?;

    match config.which {
        CmdList::Actions => list_actions(),
        CmdList::Downloaders => list_downloaders(),
        CmdList::Fixers => list_fixers(),
        CmdList::All => list_all(),
    }
    .into_iter()
    .for_each(|x| println!("{}", x));

    Ok(())
}

fn list_actions() -> Vec<String> {
    let mut v = vec![];

    v.push("Actions:".to_string());

    v.extend(
        app_actions::actions::AVAILABLE_ACTIONS
            .iter()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_downloaders() -> Vec<String> {
    let mut v = vec![];

    v.push("Downloaders:".to_string());

    v.extend(
        app_actions::downloaders::AVAILABLE_DOWNLOADERS
            .iter()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_fixers() -> Vec<String> {
    let mut v = vec![];

    v.push("Fixers:".to_string());

    v.extend(
        app_actions::fixers::AVAILABLE_FIXERS
            .iter()
            .map(|x| format!("  - {}: {}", x.name(), x.description())),
    );

    v
}

fn list_all() -> Vec<String> {
    let mut v = vec![];

    v.extend(list_actions());
    v.push(String::new());
    v.extend(list_downloaders());
    v.push(String::new());
    v.extend(list_fixers());

    v
}
