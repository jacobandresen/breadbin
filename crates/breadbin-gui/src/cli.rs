// cli - Command-line interface for breadbin
//
// This module provides CLI commands for breadbin, allowing it to be used
// from the command line in addition to the GUI.

use std::env;
use std::process::ExitCode;

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
        "search" | "csdb-search" | "scene-search" => {
            Some(handle_search_command(&sub_args))
        }
        "sync" | "csdb-sync" | "scene-sync" => {
            Some(handle_sync_command(&sub_args))
        }
        "has-data" | "csdb-has-data" => {
            Some(handle_has_data_command())
        }
        "help" | "--help" | "-h" => {
            Some(handle_help_command())
        }
        _ => {
            // Unknown command or no command - check if any args were given
            if command.starts_with('-') {
                Some(handle_help_command())
            } else {
                None // Start GUI
            }
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
    
    match crate::scene::search(&query, limit) {
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
    // Parse optional --count argument (currently ignored as sync_database uses hardcoded 100)
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
    
    // Create a tokio runtime for the sync operation
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create runtime: {}", e);
            return ExitCode::from(1);
        }
    };
    
    match rt.block_on(async { crate::scene::sync_database().await }) {
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
    if crate::scene::has_data() {
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
    println!("  breadbin                          Launch the GUI");
    println!("  breadbin search <query>           Search CSDb releases");
    println!("  breadbin sync                    Sync CSDb data to local database");
    println!("  breadbin has-data                Check if CSDb data is available");
    println!("  breadbin help                    Show this help message");
    println!();
    println!("GUI Options:");
    println!("  Once the GUI is running, use the menu to:");
    println!("  - Refresh catalogue");
    println!("  - Sync CSDb data");
    println!("  - Switch between Games, Tunes, Demos, and Scene views");
    ExitCode::SUCCESS
}
