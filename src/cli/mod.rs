mod actions;
mod commands;
mod ui;

pub use commands::AppCli;
use commands::AppCommands;

use goat_hooker::errors::GoatHookerError;

pub fn parse_commands(commands: AppCli) -> Result<(), GoatHookerError> {
    let Some(cmd) = commands.cmd else {
        return actions::status::show_status();
    };
    match cmd {
        AppCommands::Init { work_dir, template } => {
            actions::init::init_configuration(&work_dir, &template)
        }
        AppCommands::Install { force } => actions::install::install_hooks(force),
        AppCommands::Completion { shell } => {
            actions::generator::print_completion(shell);
            Ok(())
        }
        AppCommands::Run {
            hook_type,
            config_path,
            dry_run,
            verbose,
            no_cache,
            from_git,
            args,
        } => actions::run::run_hooks(
            hook_type,
            config_path,
            &args,
            actions::run::RunFlags {
                dry_run,
                verbose,
                from_git,
                no_cache,
            },
        ),
        AppCommands::Status => actions::status::show_status(),
        AppCommands::Add {
            name,
            trigger,
            commands,
            include,
            exclude,
            stage_fixed,
            project,
        } => actions::add::add_hook(actions::add::AddArgs {
            name,
            trigger,
            commands,
            include,
            exclude,
            stage_fixed,
            project,
        }),
        AppCommands::List { config_path } => actions::list::list_hooks(config_path),
        AppCommands::Update { config_path } => actions::update::update_remotes(config_path),
        AppCommands::Import { force, dry_run } => actions::import::import_configs(force, dry_run),
        AppCommands::Search { query, project } => {
            actions::search::search_hooks(&query.join(" "), project)
        }
        AppCommands::Outdated => actions::remotes::outdated(),
        AppCommands::Pin { name } => actions::remotes::pin(name),
        AppCommands::Trust { all } => actions::trust::trust_hooks(all),
        AppCommands::Vars { reset } => actions::vars::variables(reset),
        AppCommands::Ci {
            trigger,
            base,
            verbose,
        } => actions::ci::run_ci(trigger, base, verbose),
        AppCommands::History {
            limit,
            export,
            output,
        } => match export {
            Some(format) => actions::history::export(limit, &format, output.as_deref()),
            None => actions::history::show_history(limit.unwrap_or(20)),
        },
        AppCommands::Doctor => actions::doctor::check(),
        AppCommands::Watch => actions::watch::watch(),
        AppCommands::Fix { files, verbose } => actions::fix::fix_files(files, verbose),
        AppCommands::CacheClear => {
            goat_hooker::cache::clear(&cli_root()?)?;
            println!("✔ Cache cleared");
            Ok(())
        }
        AppCommands::Schema => {
            print!("{}", goat_hooker::config::json_schema());
            Ok(())
        }
    }
}

fn cli_root() -> Result<std::path::PathBuf, GoatHookerError> {
    goat_hooker::git::repo_root(&std::env::current_dir()?)
}
