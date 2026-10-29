// cli - command-line subcommands for the CSDb cache; with none, the GUI starts.

use std::env;
use std::process::ExitCode;

use breadbin_core::csdb;

/// Handle CLI commands if present, otherwise return None to start GUI
pub fn handle_cli_args() -> Option<ExitCode> {
    let args: Vec<String> = env::args().collect();
    
    // Skip the program name (first argument)
    if args.len() < 2 {
        return None; // No CLI command, start GUI
    }
    
    let command = &args[1];
    let sub_args: Vec<String> = args[2..].to_vec();
    
    match command.as_str() {
        "search" => {
            Some(handle_search_command(&sub_args))
        }
        "sync" => {
            Some(handle_sync_command(&sub_args))
        }
        "has-data" => {
            Some(handle_has_data_command())
        }
        "help" | "--help" | "-h" => {
            Some(handle_help_command())
        }
        _ => {
            eprintln!("unknown command: {command}");
            handle_help_command();
            Some(ExitCode::from(2))
        }
    }
}

/// Handle the search command
fn handle_search_command(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: breadbin search <query> [--limit N]");
        eprintln!("Search CSDb releases by name.");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --limit N    Maximum number of results to return (default: 10)");
        return ExitCode::from(1);
    }
    
    let mut query = String::new();
    let mut limit: usize = 10;
    let mut i = 0;
    
    while i < args.len() {
        if args[i].starts_with("--limit") && i + 1 < args.len() {
            if let Ok(n) = args[i + 1].parse::<usize>() {
                limit = n;
                i += 2;
            } else {
                i += 1;
            }
        } else if !args[i].starts_with('-') {
            if !query.is_empty() {
                query.push(' ');
            }
            query.push_str(&args[i]);
            i += 1;
        } else {
            i += 1;
        }
    }
    
    if query.is_empty() {
        eprintln!("Error: No search query provided");
        return ExitCode::from(1);
    }
    
    match csdb::search_csdb(&query, limit) {
        Ok(results) => {
            if results.is_empty() {
                println!("No results found for: {}", query);
            } else {
                for (id, name) in results {
                    println!("{:6} {}", id, name);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Search failed: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Handle the sync command
fn handle_sync_command(args: &[String]) -> ExitCode {
    // --count N: how many top demos (and top music releases) to sync
    let mut count: usize = 100;
    let mut i = 0;
    
    while i < args.len() {
        if args[i].starts_with("--count") && i + 1 < args.len() {
            if let Ok(n) = args[i + 1].parse::<usize>() {
                count = n;
                i += 2;
            } else {
                i += 1;
            }
        } else if args[i].starts_with('-') {
            eprintln!("Unknown option: {}", args[i]);
            eprintln!("Usage: breadbin sync [--count N]");
            return ExitCode::from(1);
        } else {
            i += 1;
        }
    }
    
    println!("Syncing top {} releases from CSDb...", count);
    
    match csdb::sync_top_releases(count) {
        Ok(stats) => {
            println!("Synced {} releases", stats.releases_fetched);
            if !stats.errors.is_empty() {
                eprintln!("Encountered {} errors during sync", stats.errors.len());
                for error in &stats.errors {
                    eprintln!("  - {}", error);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Sync failed: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Handle the has-data command
fn handle_has_data_command() -> ExitCode {
    if csdb::has_csdb_data() {
        println!("CSDb data is available");
        ExitCode::SUCCESS
    } else {
        println!("No CSDb data found. Run 'breadbin sync' to fetch data.");
        ExitCode::from(1)
    }
}

/// Handle the help command
fn handle_help_command() -> ExitCode {
    println!("breadbin - Commodore 64 kiosk and CSDb browser");
    println!();
    println!("Usage:");
    println!("  breadbin                             Launch the GUI");
    println!("  breadbin search <query> [--limit N]  Search cached CSDb releases (default 10)");
    println!("  breadbin sync [--count N]            Sync the top N CSDb demos and music (default 100)");
    println!("  breadbin has-data                    Check if CSDb data is cached");
    println!("  breadbin help                        Show this help message");
    ExitCode::SUCCESS
}
